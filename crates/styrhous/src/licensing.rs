use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use reqwest::{Client, StatusCode, Url, redirect::Policy};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const CLIENT_ID: &str = "styrhous-desktop";
const SCOPE: &str = "styrhous.desktop offline_access";
const LEASE_AUDIENCE: &str = "styrhous-desktop";
const KEYRING_SERVICE: &str = "io.github.zlepper.styrhous.licensing";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(15 * 60);
const MAX_LEASE_LIFETIME: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const HOSTED_ORIGIN: Option<&str> = option_env!("STYRHOUS_HOSTED_LICENSE_ORIGIN");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LicenseServer {
    #[default]
    Hosted,
    Custom(String),
}

impl LicenseServer {
    pub(crate) fn origin(&self) -> Result<Option<Url>> {
        match self {
            Self::Hosted => HOSTED_ORIGIN
                .map(validate_origin)
                .transpose()
                .context("the hosted licensing origin compiled into this build is invalid"),
            Self::Custom(value) => validate_origin(value).map(Some),
        }
    }

    pub(crate) fn label(&self) -> &str {
        match self {
            Self::Hosted => "Hosted Styrhous service",
            Self::Custom(_) => "Custom compatible server",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct LicensingSettings {
    pub(crate) server: LicenseServer,
    pub(crate) installation_id: Uuid,
    pub(crate) display_name: String,
    pub(crate) lease: Option<String>,
    pub(crate) keys: Option<JsonWebKeySet>,
    pub(crate) last_check_unix: Option<u64>,
    pub(crate) signed_out: bool,
}

impl Default for LicensingSettings {
    fn default() -> Self {
        Self {
            server: LicenseServer::default(),
            installation_id: Uuid::now_v7(),
            display_name: format!("Styrhous on {}", std::env::consts::OS),
            lease: None,
            keys: None,
            last_check_unix: None,
            signed_out: false,
        }
    }
}

impl LicensingSettings {
    pub(crate) fn clear_account(&mut self) {
        self.lease = None;
        self.keys = None;
        self.last_check_unix = None;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LicenseStatus {
    Unconfigured,
    SignedOut,
    Checking,
    AwaitingApproval(DeviceAuthorization),
    Licensed(VerifiedLease),
    Offline(VerifiedLease),
    Evaluation { reason_code: String },
    Failed { message: String },
}

impl LicenseStatus {
    pub(crate) fn is_waiting_for_refresh(&self) -> bool {
        matches!(self, Self::Evaluation { reason_code }
            if matches!(reason_code.as_str(), SUBSCRIPTION_RENEWAL_PENDING | OFFLINE_LEASE_EXPIRED))
    }

    pub(crate) fn shows_warning(&self) -> bool {
        !matches!(self, Self::Licensed(lease) if lease.state == "commercial")
    }

    pub(crate) fn summary(&self) -> String {
        match self {
            Self::Unconfigured => "Licensing server is not configured.".into(),
            Self::SignedOut => "Sign in to refresh your Styrhous license.".into(),
            Self::Checking => "Checking license…".into(),
            Self::AwaitingApproval(_) => "Waiting for browser approval…".into(),
            Self::Licensed(lease) if lease.state == "commercial" => {
                "Commercial license active.".into()
            }
            Self::Licensed(lease) if lease.state == "trial" => {
                format!("Trial license active until {}.", lease.expiry_label())
            }
            Self::Licensed(lease) => {
                format!(
                    "License grace period active until {}.",
                    lease.expiry_label()
                )
            }
            Self::Offline(lease) => format!(
                "Licensing service is unreachable; offline {} lease is valid until {}.",
                lease.state,
                lease.expiry_label()
            ),
            Self::Evaluation { reason_code } => {
                format!("Evaluation mode: {}.", reason_label(reason_code))
            }
            Self::Failed { message } => format!("License check failed: {message}"),
        }
    }

    pub(crate) fn lease(&self) -> Option<&VerifiedLease> {
        match self {
            Self::Licensed(lease) | Self::Offline(lease) => Some(lease),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeviceAuthorization {
    pub(crate) user_code: String,
    pub(crate) verification_uri: String,
    pub(crate) verification_uri_complete: String,
    pub(crate) expires_in: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct VerifiedLease {
    pub(crate) user_id: Uuid,
    pub(crate) seat_id: Uuid,
    pub(crate) billing_account_id: Uuid,
    pub(crate) installation_id: Uuid,
    pub(crate) activation_id: Uuid,
    pub(crate) state: String,
    pub(crate) reason_code: String,
    pub(crate) expires_at: u64,
    pub(crate) refresh_after: u64,
}

impl VerifiedLease {
    pub(crate) fn expiry_label(&self) -> String {
        time::OffsetDateTime::from_unix_timestamp(self.expires_at as i64)
            .ok()
            .and_then(|value| {
                value
                    .format(&time::format_description::well_known::Rfc3339)
                    .ok()
            })
            .unwrap_or_else(|| "an unknown time".into())
    }
}

pub(crate) struct LicensingService {
    commands: Option<Sender<LicenseCommand>>,
    results: Option<Receiver<LicenseEventMessage>>,
    settings: LicensingSettings,
    status: LicenseStatus,
    keychain_warning: bool,
    operation_generation: u64,
}

impl Default for LicensingService {
    fn default() -> Self {
        Self {
            commands: None,
            results: None,
            settings: LicensingSettings::default(),
            status: LicenseStatus::SignedOut,
            keychain_warning: false,
            operation_generation: 0,
        }
    }
}

impl LicensingService {
    pub(crate) fn start(settings: LicensingSettings, repaint: egui::Context) -> Self {
        let status = initial_status(&settings);
        let (command_sender, command_receiver) = std::sync::mpsc::channel();
        let (event_sender, event_receiver) = std::sync::mpsc::channel();
        let worker_settings = settings.clone();
        std::thread::Builder::new()
            .name("styrhous-licensing".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to create licensing runtime");
                runtime.block_on(run_worker(
                    worker_settings,
                    command_receiver,
                    event_sender,
                    repaint,
                ));
            })
            .expect("failed to start licensing worker");
        let mut service = Self {
            commands: Some(command_sender),
            results: Some(event_receiver),
            settings,
            status,
            keychain_warning: false,
            operation_generation: 0,
        };
        if !service.settings.signed_out && service.settings.server.origin().ok().flatten().is_some()
        {
            service.refresh();
        }
        service
    }

    pub(crate) fn poll(&mut self, repaint: &egui::Context) {
        let now = unix_now();
        self.poll_at(now);
        if let Some(lease) = self.status.lease() {
            repaint
                .request_repaint_after(Duration::from_secs(lease.expires_at.saturating_sub(now)));
        }
    }

    fn poll_at(&mut self, now: u64) {
        self.poll_events();
        if self
            .status
            .lease()
            .is_some_and(|lease| now >= lease.expires_at)
        {
            self.settings.clear_account();
            self.status = LicenseStatus::Evaluation {
                reason_code: OFFLINE_LEASE_EXPIRED.into(),
            };
        }
    }

    fn poll_events(&mut self) {
        let Some(receiver) = &self.results else {
            return;
        };
        loop {
            let event = match receiver.try_recv() {
                Ok(message) if message.operation_generation == self.operation_generation => {
                    message.event
                }
                Ok(_) => continue,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.status = LicenseStatus::Failed {
                        message: "the licensing background service stopped".into(),
                    };
                    break;
                }
            };
            match event {
                LicenseEvent::Checking => {
                    // Keep the lease deadline and retained-session controls
                    // visible while a background request is in flight.
                    if self.status.lease().is_none() && !self.status.is_waiting_for_refresh() {
                        self.status = LicenseStatus::Checking;
                    }
                }
                LicenseEvent::AwaitingApproval(authorization) => {
                    self.status = LicenseStatus::AwaitingApproval(authorization);
                }
                LicenseEvent::Licensed {
                    lease,
                    encoded_lease,
                    keys,
                    checked_at,
                } => {
                    self.settings.lease = Some(encoded_lease);
                    self.settings.keys = Some(keys);
                    self.settings.last_check_unix = Some(checked_at);
                    self.settings.signed_out = false;
                    self.status = LicenseStatus::Licensed(lease);
                }
                LicenseEvent::Offline { lease, message } => {
                    tracing::warn!(error = %message, "license refresh failed; using cached lease");
                    self.status = LicenseStatus::Offline(lease);
                }
                LicenseEvent::Evaluation { reason_code } => {
                    self.settings.clear_account();
                    self.status = LicenseStatus::Evaluation { reason_code };
                }
                LicenseEvent::SignedOut {
                    suppress_auto_refresh,
                } => {
                    self.settings.clear_account();
                    self.settings.signed_out = suppress_auto_refresh;
                    self.status = if self.settings.server.origin().ok().flatten().is_some() {
                        LicenseStatus::SignedOut
                    } else {
                        LicenseStatus::Unconfigured
                    };
                }
                LicenseEvent::ClearCachedLease => self.settings.clear_account(),
                LicenseEvent::Failed(message) => {
                    self.status = LicenseStatus::Failed { message };
                }
                LicenseEvent::KeychainUnavailable => self.keychain_warning = true,
            }
        }
    }

    pub(crate) fn status(&self) -> &LicenseStatus {
        &self.status
    }

    pub(crate) fn settings(&self) -> &LicensingSettings {
        &self.settings
    }

    pub(crate) fn keychain_warning(&self) -> bool {
        self.keychain_warning
    }

    pub(crate) fn sign_in(&mut self) {
        self.settings.signed_out = false;
        let operation_generation = self.next_operation_generation();
        self.send(LicenseCommand::StartSignIn {
            operation_generation,
        });
    }

    pub(crate) fn refresh(&mut self) {
        let operation_generation = self.next_operation_generation();
        self.send(LicenseCommand::Refresh {
            operation_generation,
        });
    }

    pub(crate) fn sign_out(&mut self) {
        let operation_generation = self.next_operation_generation();
        self.settings.clear_account();
        self.settings.signed_out = true;
        self.status = LicenseStatus::SignedOut;
        self.send(LicenseCommand::SignOut {
            operation_generation,
        });
    }

    pub(crate) fn switch_server(&mut self, server: LicenseServer) {
        let operation_generation = self.next_operation_generation();
        self.settings.server = server.clone();
        self.settings.clear_account();
        self.settings.signed_out = true;
        self.keychain_warning = false;
        self.status = if server.origin().ok().flatten().is_some() {
            LicenseStatus::SignedOut
        } else {
            LicenseStatus::Unconfigured
        };
        self.send(LicenseCommand::SwitchServer {
            server,
            operation_generation,
        });
    }

    pub(crate) fn set_display_name(&mut self, display_name: String) {
        self.settings.display_name = display_name.clone();
        self.send(LicenseCommand::SetDisplayName(display_name));
    }

    pub(crate) fn account_url(&self) -> Option<String> {
        self.settings
            .server
            .origin()
            .ok()
            .flatten()
            .and_then(|origin| origin.join("/account").ok())
            .map(|url| url.to_string())
    }

    fn send(&self, command: LicenseCommand) {
        if let Some(sender) = &self.commands {
            let _ = sender.send(command);
        }
    }

    fn next_operation_generation(&mut self) -> u64 {
        self.operation_generation = self.operation_generation.wrapping_add(1);
        self.operation_generation
    }

    #[cfg(test)]
    pub(crate) fn set_status_for_test(&mut self, status: LicenseStatus) {
        // A deterministic UI fixture must not race a real worker response.
        self.commands = None;
        self.results = None;
        self.status = status;
    }
}

#[derive(Debug)]
enum LicenseCommand {
    StartSignIn {
        operation_generation: u64,
    },
    Refresh {
        operation_generation: u64,
    },
    SignOut {
        operation_generation: u64,
    },
    SwitchServer {
        server: LicenseServer,
        operation_generation: u64,
    },
    SetDisplayName(String),
}

#[derive(Debug)]
struct LicenseEventMessage {
    operation_generation: u64,
    event: LicenseEvent,
}

#[derive(Debug)]
enum LicenseEvent {
    Checking,
    AwaitingApproval(DeviceAuthorization),
    Licensed {
        lease: VerifiedLease,
        encoded_lease: String,
        keys: JsonWebKeySet,
        checked_at: u64,
    },
    Offline {
        lease: VerifiedLease,
        message: String,
    },
    Evaluation {
        reason_code: String,
    },
    SignedOut {
        suppress_auto_refresh: bool,
    },
    ClearCachedLease,
    Failed(String),
    KeychainUnavailable,
}

async fn run_worker(
    mut settings: LicensingSettings,
    commands: Receiver<LicenseCommand>,
    events: Sender<LicenseEventMessage>,
    repaint: egui::Context,
) {
    let mut session_refresh_token = None;
    let mut retry_delay = None;
    let mut retry_operation_generation = 0;
    let mut pending_command = None;
    loop {
        let command = if let Some(command) = pending_command.take() {
            command
        } else {
            match retry_delay {
                Some(delay) => match commands.recv_timeout(delay) {
                    Ok(command) => command,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => LicenseCommand::Refresh {
                        operation_generation: retry_operation_generation,
                    },
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                },
                None => match commands.recv() {
                    Ok(command) => command,
                    Err(_) => break,
                },
            }
        };
        match command {
            LicenseCommand::SetDisplayName(display_name) => settings.display_name = display_name,
            LicenseCommand::SwitchServer {
                server,
                operation_generation,
            } => {
                revoke_and_delete_session(
                    settings.server.origin().ok().flatten(),
                    session_refresh_token.take(),
                    &events,
                    &repaint,
                    operation_generation,
                )
                .await;
                settings.server = server;
                settings.clear_account();
                settings.signed_out = true;
                retry_delay = None;
                send_event(
                    &events,
                    &repaint,
                    operation_generation,
                    LicenseEvent::SignedOut {
                        suppress_auto_refresh: true,
                    },
                );
            }
            LicenseCommand::SignOut {
                operation_generation,
            } => {
                revoke_and_delete_session(
                    settings.server.origin().ok().flatten(),
                    session_refresh_token.take(),
                    &events,
                    &repaint,
                    operation_generation,
                )
                .await;
                settings.clear_account();
                settings.signed_out = true;
                retry_delay = None;
                send_event(
                    &events,
                    &repaint,
                    operation_generation,
                    LicenseEvent::SignedOut {
                        suppress_auto_refresh: true,
                    },
                );
            }
            LicenseCommand::Refresh {
                operation_generation,
            } => {
                send_event(
                    &events,
                    &repaint,
                    operation_generation,
                    LicenseEvent::Checking,
                );
                match run_until_command(
                    refresh(
                        &mut settings,
                        &mut session_refresh_token,
                        &events,
                        &repaint,
                        operation_generation,
                    ),
                    &commands,
                )
                .await
                {
                    LicenseAttemptOutcome::Completed {
                        result,
                        display_name,
                    } => {
                        if let Some(display_name) = display_name {
                            settings.display_name = display_name;
                        }
                        if let Err(error) = &result {
                            send_event(
                                &events,
                                &repaint,
                                operation_generation,
                                refresh_failure_event(&settings, error, unix_now()),
                            );
                        }
                        retry_delay = refresh_retry_delay(&settings, &result, retry_delay);
                        retry_operation_generation = operation_generation;
                    }
                    LicenseAttemptOutcome::Interrupted {
                        command,
                        display_name,
                    } => {
                        if let Some(display_name) = display_name {
                            settings.display_name = display_name;
                        }
                        retry_delay = None;
                        pending_command = Some(command);
                    }
                    LicenseAttemptOutcome::Disconnected => break,
                }
            }
            LicenseCommand::StartSignIn {
                operation_generation,
            } => {
                settings.signed_out = false;
                send_event(
                    &events,
                    &repaint,
                    operation_generation,
                    LicenseEvent::Checking,
                );
                match run_until_command(
                    sign_in(
                        &mut settings,
                        &mut session_refresh_token,
                        &events,
                        &repaint,
                        operation_generation,
                    ),
                    &commands,
                )
                .await
                {
                    LicenseAttemptOutcome::Completed {
                        result,
                        display_name,
                    } => {
                        if let Some(display_name) = display_name {
                            settings.display_name = display_name;
                        }
                        retry_delay = match result {
                            Ok(()) => scheduled_refresh_delay(&settings),
                            Err(error) => {
                                send_event(
                                    &events,
                                    &repaint,
                                    operation_generation,
                                    refresh_failure_event(&settings, &error, unix_now()),
                                );
                                error
                                    .is::<SubscriptionRenewalPending>()
                                    .then(|| next_retry_delay(None))
                            }
                        };
                        retry_operation_generation = operation_generation;
                    }
                    LicenseAttemptOutcome::Interrupted {
                        command,
                        display_name,
                    } => {
                        if let Some(display_name) = display_name {
                            settings.display_name = display_name;
                        }
                        pending_command = Some(command);
                    }
                    LicenseAttemptOutcome::Disconnected => break,
                }
            }
        }
    }
}

async fn revoke_and_delete_session(
    origin: Option<Url>,
    session_refresh_token: Option<String>,
    events: &Sender<LicenseEventMessage>,
    repaint: &egui::Context,
    operation_generation: u64,
) {
    let Some(origin) = origin else {
        return;
    };
    let credentials = KeyringCredentialStore;
    let refresh_token = session_refresh_token.or_else(|| credentials.load(&origin).ok().flatten());
    if let Some(refresh_token) = refresh_token
        && let Ok(transport) = HttpLicenseTransport::new(origin.clone())
    {
        let _ = transport.revoke(&refresh_token).await;
    }
    if credentials.delete(&origin).is_err() {
        send_event(
            events,
            repaint,
            operation_generation,
            LicenseEvent::KeychainUnavailable,
        );
    }
}

enum LicenseAttemptOutcome {
    Completed {
        result: Result<()>,
        display_name: Option<String>,
    },
    Interrupted {
        command: LicenseCommand,
        display_name: Option<String>,
    },
    Disconnected,
}

async fn run_until_command(
    attempt: impl Future<Output = Result<()>>,
    commands: &Receiver<LicenseCommand>,
) -> LicenseAttemptOutcome {
    tokio::pin!(attempt);
    let mut display_name = None;
    loop {
        tokio::select! {
            result = &mut attempt => {
                return LicenseAttemptOutcome::Completed { result, display_name };
            }
            () = tokio::time::sleep(Duration::from_millis(100)) => {
                loop {
                    match commands.try_recv() {
                        Ok(LicenseCommand::SetDisplayName(value)) => display_name = Some(value),
                        Ok(command) => {
                            return LicenseAttemptOutcome::Interrupted {
                                command,
                                display_name,
                            };
                        }
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            return LicenseAttemptOutcome::Disconnected;
                        }
                    }
                }
            }
        }
    }
}

async fn refresh(
    settings: &mut LicensingSettings,
    session_refresh_token: &mut Option<String>,
    events: &Sender<LicenseEventMessage>,
    repaint: &egui::Context,
    operation_generation: u64,
) -> Result<()> {
    let origin = settings
        .server
        .origin()?
        .ok_or_else(|| anyhow!("the hosted licensing service is not configured in this build"))?;
    let credentials = KeyringCredentialStore;
    let transport = HttpLicenseTransport::new(origin.clone())?;
    let clock = SystemUnixClock;
    let runtime = LicenseRuntime {
        origin,
        transport: &transport,
        credentials: &credentials,
        clock: &clock,
        events: EventTarget {
            events,
            repaint,
            operation_generation,
        },
    };
    refresh_with(settings, session_refresh_token, &runtime).await
}

async fn refresh_with<T: LicenseTransport, C: CredentialStore>(
    settings: &mut LicensingSettings,
    session_refresh_token: &mut Option<String>,
    runtime: &LicenseRuntime<'_, T, C, impl UnixClock>,
) -> Result<()> {
    let refresh_token = match session_refresh_token.clone() {
        Some(token) => token,
        None => match runtime.credentials.load(&runtime.origin) {
            Ok(Some(token)) => token,
            Ok(None) => {
                if cached_lease_at(settings, runtime.clock.now()).is_some() {
                    bail!("no refresh credential is available for the cached offline lease");
                } else {
                    runtime.events.send(LicenseEvent::SignedOut {
                        suppress_auto_refresh: false,
                    });
                    return Ok(());
                }
            }
            Err(_) => {
                runtime.events.send(LicenseEvent::KeychainUnavailable);
                if cached_lease_at(settings, runtime.clock.now()).is_some() {
                    bail!("the refresh credential could not be read for the cached offline lease");
                } else {
                    runtime.events.send(LicenseEvent::SignedOut {
                        suppress_auto_refresh: false,
                    });
                    return Ok(());
                }
            }
        },
    };
    match runtime.transport.refresh(&refresh_token).await? {
        TokenExchange::Granted(tokens) => {
            persist_and_request_lease(settings, tokens, session_refresh_token, runtime).await
        }
        TokenExchange::Pending(_) => bail!("the refresh token response was incomplete"),
        TokenExchange::Denied(error) if error == "invalid_grant" => {
            if runtime.credentials.delete(&runtime.origin).is_err() {
                runtime.events.send(LicenseEvent::KeychainUnavailable);
            }
            *session_refresh_token = None;
            settings.clear_account();
            runtime.events.send(LicenseEvent::SignedOut {
                suppress_auto_refresh: false,
            });
            Ok(())
        }
        TokenExchange::RenewalPending => Err(SubscriptionRenewalPending.into()),
        TokenExchange::Denied(error) => bail!("license refresh was denied: {error}"),
    }
}

async fn sign_in(
    settings: &mut LicensingSettings,
    session_refresh_token: &mut Option<String>,
    events: &Sender<LicenseEventMessage>,
    repaint: &egui::Context,
    operation_generation: u64,
) -> Result<()> {
    let origin = settings
        .server
        .origin()?
        .ok_or_else(|| anyhow!("the hosted licensing service is not configured in this build"))?;
    let transport = HttpLicenseTransport::new(origin.clone())?;
    let clock = SystemUnixClock;
    let runtime = LicenseRuntime {
        origin,
        transport: &transport,
        credentials: &KeyringCredentialStore,
        clock: &clock,
        events: EventTarget {
            events,
            repaint,
            operation_generation,
        },
    };
    sign_in_with(settings, session_refresh_token, &runtime).await
}

async fn sign_in_with<T: LicenseTransport, C: CredentialStore>(
    settings: &mut LicensingSettings,
    session_refresh_token: &mut Option<String>,
    runtime: &LicenseRuntime<'_, T, C, impl UnixClock>,
) -> Result<()> {
    let authorization = runtime.transport.start_authorization(settings).await?;
    let verification_uri =
        Url::parse(&authorization.verification_uri).context("the verification URL is invalid")?;
    let verification_uri_complete = Url::parse(&authorization.verification_uri_complete)
        .context("the complete verification URL is invalid")?;
    if !same_origin(&verification_uri, &runtime.origin)
        || !same_origin(&verification_uri_complete, &runtime.origin)
    {
        bail!("the licensing server returned a cross-origin verification URL");
    }
    runtime
        .events
        .send(LicenseEvent::AwaitingApproval(DeviceAuthorization {
            user_code: authorization.user_code.clone(),
            verification_uri: authorization.verification_uri.clone(),
            verification_uri_complete: authorization.verification_uri_complete.clone(),
            expires_in: authorization.expires_in,
        }));
    let started = std::time::Instant::now();
    let mut interval = Duration::from_secs(authorization.interval.max(1));
    loop {
        if started.elapsed() >= Duration::from_secs(authorization.expires_in) {
            bail!("the device authorization expired");
        }
        tokio::time::sleep(interval).await;
        match runtime.transport.poll(&authorization.device_code).await? {
            TokenExchange::Granted(tokens) => {
                if tokens.refresh_token.is_none() {
                    bail!("the device authorization did not issue a refresh credential");
                }
                return persist_and_request_lease(settings, tokens, session_refresh_token, runtime)
                    .await;
            }
            TokenExchange::Pending(error) if error == "slow_down" => {
                interval += Duration::from_secs(5);
            }
            TokenExchange::Pending(_) => {}
            TokenExchange::RenewalPending => interval = next_retry_delay(Some(interval)),
            TokenExchange::Denied(error) => bail!("device authorization was denied: {error}"),
        }
    }
}

async fn persist_and_request_lease<T, C>(
    settings: &mut LicensingSettings,
    tokens: TokenResponse,
    session_refresh_token: &mut Option<String>,
    runtime: &LicenseRuntime<'_, T, C, impl UnixClock>,
) -> Result<()>
where
    T: LicenseTransport,
    C: CredentialStore,
{
    if let Some(refresh_token) = tokens.refresh_token {
        match runtime.credentials.save(&runtime.origin, &refresh_token) {
            Ok(()) => *session_refresh_token = None,
            Err(_) => {
                let _ = runtime.credentials.delete(&runtime.origin);
                *session_refresh_token = Some(refresh_token);
                runtime.events.send(LicenseEvent::KeychainUnavailable);
            }
        }
    }
    match runtime.transport.entitlement(&tokens.access_token).await? {
        EntitlementExchange::Issued(response) => {
            let keys = runtime.transport.keys().await?;
            let checked_at = runtime.clock.now();
            let lease = validate_lease_at(
                &response.lease,
                &keys,
                &runtime.origin,
                settings.installation_id,
                checked_at,
            )
            .inspect_err(|_| {
                settings.clear_account();
                runtime.events.send(LicenseEvent::ClearCachedLease);
            })?;
            settings.lease = Some(response.lease.clone());
            settings.keys = Some(keys.clone());
            settings.last_check_unix = Some(checked_at);
            settings.signed_out = false;
            runtime.events.send(LicenseEvent::Licensed {
                lease,
                encoded_lease: response.lease,
                keys,
                checked_at,
            });
        }
        EntitlementExchange::Ineligible(reason_code) => {
            if runtime.credentials.delete(&runtime.origin).is_err() {
                runtime.events.send(LicenseEvent::KeychainUnavailable);
            }
            *session_refresh_token = None;
            settings.clear_account();
            runtime
                .events
                .send(LicenseEvent::Evaluation { reason_code });
        }
        EntitlementExchange::Unauthorized => {
            if runtime.credentials.delete(&runtime.origin).is_err() {
                runtime.events.send(LicenseEvent::KeychainUnavailable);
            }
            *session_refresh_token = None;
            settings.clear_account();
            runtime.events.send(LicenseEvent::SignedOut {
                suppress_auto_refresh: false,
            });
        }
        EntitlementExchange::RenewalPending => return Err(SubscriptionRenewalPending.into()),
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct EventTarget<'a> {
    events: &'a Sender<LicenseEventMessage>,
    repaint: &'a egui::Context,
    operation_generation: u64,
}

struct LicenseRuntime<'a, T, C, K> {
    origin: Url,
    transport: &'a T,
    credentials: &'a C,
    clock: &'a K,
    events: EventTarget<'a>,
}

impl EventTarget<'_> {
    fn send(self, event: LicenseEvent) {
        send_event(self.events, self.repaint, self.operation_generation, event);
    }
}

fn send_event(
    events: &Sender<LicenseEventMessage>,
    repaint: &egui::Context,
    operation_generation: u64,
    event: LicenseEvent,
) {
    let _ = events.send(LicenseEventMessage {
        operation_generation,
        event,
    });
    repaint.request_repaint();
}

fn initial_status(settings: &LicensingSettings) -> LicenseStatus {
    if settings.server.origin().ok().flatten().is_none() {
        return LicenseStatus::Unconfigured;
    }
    cached_lease(settings)
        .map(LicenseStatus::Offline)
        .unwrap_or(LicenseStatus::SignedOut)
}

fn cached_lease(settings: &LicensingSettings) -> Option<VerifiedLease> {
    cached_lease_at(settings, unix_now())
}

fn cached_lease_at(settings: &LicensingSettings, observed_at: u64) -> Option<VerifiedLease> {
    validate_lease_at(
        settings.lease.as_deref()?,
        settings.keys.as_ref()?,
        &settings.server.origin().ok().flatten()?,
        settings.installation_id,
        observed_at,
    )
    .ok()
}

trait UnixClock {
    fn now(&self) -> u64;
}

struct SystemUnixClock;

impl UnixClock for SystemUnixClock {
    fn now(&self) -> u64 {
        unix_now()
    }
}

#[async_trait]
trait LicenseTransport {
    async fn start_authorization(&self, settings: &LicensingSettings)
    -> Result<DeviceCodeResponse>;
    async fn poll(&self, device_code: &str) -> Result<TokenExchange>;
    async fn refresh(&self, refresh_token: &str) -> Result<TokenExchange>;
    async fn entitlement(&self, access_token: &str) -> Result<EntitlementExchange>;
    async fn keys(&self) -> Result<JsonWebKeySet>;
    async fn revoke(&self, refresh_token: &str) -> Result<()>;
}

trait CredentialStore {
    fn load(&self, origin: &Url) -> Result<Option<String>>;
    fn save(&self, origin: &Url, refresh_token: &str) -> Result<()>;
    fn delete(&self, origin: &Url) -> Result<()>;
}

struct KeyringCredentialStore;

impl CredentialStore for KeyringCredentialStore {
    fn load(&self, origin: &Url) -> Result<Option<String>> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, origin.as_str())?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn save(&self, origin: &Url, refresh_token: &str) -> Result<()> {
        keyring::Entry::new(KEYRING_SERVICE, origin.as_str())?.set_password(refresh_token)?;
        Ok(())
    }

    fn delete(&self, origin: &Url) -> Result<()> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, origin.as_str())?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

struct HttpLicenseTransport {
    origin: Url,
    client: Client,
}

impl HttpLicenseTransport {
    fn new(origin: Url) -> Result<Self> {
        let redirect_origin = origin.clone();
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .redirect(Policy::custom(move |attempt| {
                if attempt.previous().len() >= 5 || !same_origin(attempt.url(), &redirect_origin) {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .context("could not initialize secure licensing HTTP")?;
        Ok(Self { origin, client })
    }

    fn endpoint(&self, path: &str) -> Result<Url> {
        self.origin
            .join(path)
            .with_context(|| format!("invalid licensing endpoint {path}"))
    }

    async fn token_exchange(&self, form: &[(&str, &str)]) -> Result<TokenExchange> {
        let response = self
            .client
            .post(self.endpoint("desktop/v1/token")?)
            .form(form)
            .send()
            .await
            .context("could not reach the licensing token service")?;
        let status = response.status();
        let body = response
            .bytes()
            .await
            .context("could not read the licensing token response")?;
        if status.is_success() {
            return Ok(TokenExchange::Granted(
                serde_json::from_slice(&body).context("invalid licensing token response")?,
            ));
        }
        let error: OAuthError =
            serde_json::from_slice(&body).context("invalid licensing token error response")?;
        if status == StatusCode::SERVICE_UNAVAILABLE && error.error == "temporarily_unavailable" {
            Ok(TokenExchange::RenewalPending)
        } else if matches!(error.error.as_str(), "authorization_pending" | "slow_down") {
            Ok(TokenExchange::Pending(error.error))
        } else {
            Ok(TokenExchange::Denied(error.error))
        }
    }
}

#[async_trait]
impl LicenseTransport for HttpLicenseTransport {
    async fn start_authorization(
        &self,
        settings: &LicensingSettings,
    ) -> Result<DeviceCodeResponse> {
        let installation_id = settings.installation_id.to_string();
        let response = self
            .client
            .post(self.endpoint("desktop/v1/device/authorize")?)
            .form(&[
                ("client_id", CLIENT_ID),
                ("scope", SCOPE),
                ("installation_id", installation_id.as_str()),
                ("display_name", settings.display_name.as_str()),
                ("platform", std::env::consts::OS),
                ("architecture", std::env::consts::ARCH),
                ("styrhous_version", env!("CARGO_PKG_VERSION")),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .context("invalid device authorization response")?;
        Ok(response)
    }

    async fn poll(&self, device_code: &str) -> Result<TokenExchange> {
        self.token_exchange(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", CLIENT_ID),
            ("device_code", device_code),
        ])
        .await
    }

    async fn refresh(&self, refresh_token: &str) -> Result<TokenExchange> {
        self.token_exchange(&[
            ("grant_type", "refresh_token"),
            ("client_id", CLIENT_ID),
            ("refresh_token", refresh_token),
        ])
        .await
    }

    async fn entitlement(&self, access_token: &str) -> Result<EntitlementExchange> {
        let response = self
            .client
            .post(self.endpoint("desktop/v1/entitlement")?)
            .bearer_auth(access_token)
            .send()
            .await
            .context("could not refresh the entitlement lease")?;
        let status = response.status();
        if status.is_success() {
            return Ok(EntitlementExchange::Issued(
                response
                    .json()
                    .await
                    .context("invalid entitlement lease response")?,
            ));
        }
        if status == StatusCode::SERVICE_UNAVAILABLE {
            let error: DesktopError = response
                .json()
                .await
                .context("invalid entitlement error response")?;
            if error.reason_code == SUBSCRIPTION_RENEWAL_PENDING {
                return Ok(EntitlementExchange::RenewalPending);
            }
            bail!("the entitlement service is temporarily unavailable");
        }
        if matches!(status, StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED) {
            let error: DesktopError = response
                .json()
                .await
                .context("invalid entitlement error response")?;
            return Ok(if status == StatusCode::UNAUTHORIZED {
                EntitlementExchange::Unauthorized
            } else {
                EntitlementExchange::Ineligible(error.reason_code)
            });
        }
        Err(response
            .error_for_status()
            .expect_err("status is not successful")
            .into())
    }

    async fn keys(&self) -> Result<JsonWebKeySet> {
        self.client
            .get(self.endpoint("desktop/v1/keys")?)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .context("invalid licensing key set")
    }

    async fn revoke(&self, refresh_token: &str) -> Result<()> {
        self.client
            .post(self.endpoint("desktop/v1/token/revoke")?)
            .form(&[
                ("client_id", CLIENT_ID),
                ("token", refresh_token),
                ("token_type_hint", "refresh_token"),
            ])
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}

#[derive(Clone, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
}

enum TokenExchange {
    Granted(TokenResponse),
    Pending(String),
    Denied(String),
    RenewalPending,
}

#[derive(Deserialize)]
struct OAuthError {
    error: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EntitlementResponse {
    lease: String,
}

enum EntitlementExchange {
    Issued(EntitlementResponse),
    Ineligible(String),
    Unauthorized,
    RenewalPending,
}

const SUBSCRIPTION_RENEWAL_PENDING: &str = "subscription_renewal_pending";
const OFFLINE_LEASE_EXPIRED: &str = "offline_lease_expired";

#[derive(Debug)]
struct SubscriptionRenewalPending;

impl std::fmt::Display for SubscriptionRenewalPending {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("subscription renewal is awaiting confirmation")
    }
}

impl std::error::Error for SubscriptionRenewalPending {}

fn refresh_failure_event(
    settings: &LicensingSettings,
    error: &anyhow::Error,
    now: u64,
) -> LicenseEvent {
    if let Some(lease) = cached_lease_at(settings, now) {
        LicenseEvent::Offline {
            lease,
            message: error.to_string(),
        }
    } else if error.is::<SubscriptionRenewalPending>() {
        LicenseEvent::Evaluation {
            reason_code: SUBSCRIPTION_RENEWAL_PENDING.into(),
        }
    } else {
        LicenseEvent::Failed(error.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopError {
    reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct JsonWebKeySet {
    keys: Vec<JsonWebKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct JsonWebKey {
    kty: String,
    #[serde(rename = "use")]
    usage: String,
    alg: String,
    kid: String,
    n: String,
    e: String,
}

#[derive(Deserialize)]
struct LeaseClaims {
    schema_version: u8,
    iss: String,
    aud: String,
    sub: Uuid,
    seat_id: Uuid,
    billing_account_id: Uuid,
    installation_id: Uuid,
    activation_id: Uuid,
    state: String,
    reason_code: String,
    iat: u64,
    nbf: u64,
    exp: u64,
    refresh_after: u64,
    jti: String,
    signing_key_id: String,
}

fn validate_lease_at(
    token: &str,
    key_set: &JsonWebKeySet,
    origin: &Url,
    installation_id: Uuid,
    observed_at: u64,
) -> Result<VerifiedLease> {
    let header = decode_header(token).context("the cached lease header is invalid")?;
    if header.alg != Algorithm::RS256 {
        bail!("the lease uses an unsupported signature algorithm");
    }
    let key_id = header
        .kid
        .context("the lease has no signing key identifier")?;
    let key = key_set
        .keys
        .iter()
        .find(|key| key.kid == key_id)
        .ok_or_else(|| anyhow!("the lease signing key is not trusted"))?;
    if key.kty != "RSA" || key.usage != "sig" || key.alg != "RS256" {
        bail!("the lease signing key has incompatible metadata");
    }
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[LEASE_AUDIENCE]);
    validation.set_issuer(&[origin.as_str()]);
    validation.validate_exp = false;
    validation.validate_nbf = false;
    validation.leeway = 0;
    validation.set_required_spec_claims(&["exp", "iat", "nbf", "jti", "aud", "iss", "sub"]);
    let data = decode::<LeaseClaims>(
        token,
        &DecodingKey::from_rsa_components(&key.n, &key.e)?,
        &validation,
    )
    .context("the lease signature or registered claims are invalid")?;
    let claims = data.claims;
    let lease_lifetime = claims.exp.checked_sub(claims.iat);
    let valid_jwt_id = Uuid::parse_str(&claims.jti).is_ok();
    if claims.schema_version != 1
        || claims.iss != origin.as_str()
        || claims.aud != LEASE_AUDIENCE
        || claims.installation_id != installation_id
        || claims.signing_key_id != key_id
        || !matches!(claims.state.as_str(), "trial" | "commercial" | "grace")
        || claims.iat > claims.nbf
        || claims.nbf > observed_at
        || claims.exp <= observed_at
        || lease_lifetime.is_none_or(|seconds| seconds > MAX_LEASE_LIFETIME.as_secs())
        || claims.refresh_after < claims.iat
        || claims.refresh_after > claims.exp
        || !valid_jwt_id
    {
        bail!("the lease claims do not match this installation");
    }
    Ok(VerifiedLease {
        user_id: claims.sub,
        seat_id: claims.seat_id,
        billing_account_id: claims.billing_account_id,
        installation_id: claims.installation_id,
        activation_id: claims.activation_id,
        state: claims.state,
        reason_code: claims.reason_code,
        expires_at: claims.exp,
        refresh_after: claims.refresh_after,
    })
}

fn validate_origin(value: &str) -> Result<Url> {
    let mut origin =
        Url::parse(value.trim()).context("licensing server must be an absolute URL")?;
    if !origin.username().is_empty()
        || origin.password().is_some()
        || origin.query().is_some()
        || origin.fragment().is_some()
        || origin.path() != "/"
    {
        bail!("licensing server must be an origin without credentials, path, query, or fragment");
    }
    let secure = origin.scheme() == "https";
    let local_development = !cfg!(styrhous_release_build)
        && origin.scheme() == "http"
        && origin.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
    if !secure && !local_development {
        bail!("licensing server must use HTTPS (HTTP is limited to local development)");
    }
    origin.set_path("/");
    Ok(origin)
}

fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn refresh_retry_delay(
    settings: &LicensingSettings,
    result: &Result<()>,
    previous: Option<Duration>,
) -> Option<Duration> {
    if result.is_err() {
        Some(next_retry_delay(previous))
    } else {
        scheduled_refresh_delay(settings)
    }
}

fn next_retry_delay(previous: Option<Duration>) -> Duration {
    previous
        .map(|delay| (delay * 2).min(MAX_RETRY_DELAY))
        .unwrap_or(Duration::from_secs(30))
}

fn scheduled_refresh_delay(settings: &LicensingSettings) -> Option<Duration> {
    let lease = cached_lease(settings)?;
    Some(Duration::from_secs(
        lease.refresh_after.saturating_sub(unix_now()).max(1),
    ))
}

fn reason_label(reason_code: &str) -> String {
    reason_code.replace('_', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct GoldenLease {
        issuer: String,
        audience: String,
        installation_id: Uuid,
        keys: JsonWebKeySet,
        lease: String,
        expected: GoldenExpected,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct GoldenExpected {
        state: String,
        reason_code: String,
        issued_at: u64,
        expires_at: u64,
        refresh_after: u64,
    }

    struct FixedUnixClock(u64);

    impl UnixClock for FixedUnixClock {
        fn now(&self) -> u64 {
            self.0
        }
    }

    struct FakeTransport {
        lease: String,
        keys: JsonWebKeySet,
        deny_refresh: bool,
        ineligible_reason: Option<String>,
        unauthorized_entitlement: bool,
        refresh_tokens: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl LicenseTransport for FakeTransport {
        async fn start_authorization(
            &self,
            _settings: &LicensingSettings,
        ) -> Result<DeviceCodeResponse> {
            bail!("device authorization is not used by this scenario")
        }

        async fn poll(&self, _device_code: &str) -> Result<TokenExchange> {
            bail!("device polling is not used by this scenario")
        }

        async fn refresh(&self, refresh_token: &str) -> Result<TokenExchange> {
            self.refresh_tokens
                .lock()
                .unwrap()
                .push(refresh_token.into());
            if self.deny_refresh {
                Ok(TokenExchange::Denied("invalid_grant".into()))
            } else {
                Ok(TokenExchange::Granted(TokenResponse {
                    access_token: "short-lived-access-token".into(),
                    refresh_token: Some("rotated-refresh-token".into()),
                }))
            }
        }

        async fn entitlement(&self, access_token: &str) -> Result<EntitlementExchange> {
            assert_eq!(access_token, "short-lived-access-token");
            Ok(if self.unauthorized_entitlement {
                EntitlementExchange::Unauthorized
            } else {
                match &self.ineligible_reason {
                    Some(reason_code) => EntitlementExchange::Ineligible(reason_code.clone()),
                    None => EntitlementExchange::Issued(EntitlementResponse {
                        lease: self.lease.clone(),
                    }),
                }
            })
        }

        async fn keys(&self) -> Result<JsonWebKeySet> {
            assert!(
                self.ineligible_reason.is_none() && !self.unauthorized_entitlement,
                "signing keys must not be requested after entitlement rejection"
            );
            Ok(self.keys.clone())
        }

        async fn revoke(&self, _refresh_token: &str) -> Result<()> {
            Ok(())
        }
    }

    struct RenewalTransport {
        inner: FakeTransport,
        pending: Mutex<bool>,
        at_token: bool,
    }

    #[async_trait]
    impl LicenseTransport for RenewalTransport {
        async fn start_authorization(
            &self,
            settings: &LicensingSettings,
        ) -> Result<DeviceCodeResponse> {
            self.inner.start_authorization(settings).await
        }

        async fn poll(&self, device_code: &str) -> Result<TokenExchange> {
            self.inner.poll(device_code).await
        }

        async fn refresh(&self, refresh_token: &str) -> Result<TokenExchange> {
            if *self.pending.lock().unwrap() && self.at_token {
                self.inner
                    .refresh_tokens
                    .lock()
                    .unwrap()
                    .push(refresh_token.into());
                Ok(TokenExchange::RenewalPending)
            } else {
                self.inner.refresh(refresh_token).await
            }
        }

        async fn entitlement(&self, access_token: &str) -> Result<EntitlementExchange> {
            if *self.pending.lock().unwrap() && !self.at_token {
                Ok(EntitlementExchange::RenewalPending)
            } else {
                self.inner.entitlement(access_token).await
            }
        }

        async fn keys(&self) -> Result<JsonWebKeySet> {
            self.inner.keys().await
        }

        async fn revoke(&self, refresh_token: &str) -> Result<()> {
            self.inner.revoke(refresh_token).await
        }
    }

    struct DeviceFlowTransport {
        authorization: DeviceCodeResponse,
        tokens: Mutex<Option<TokenResponse>>,
        lease: String,
        keys: JsonWebKeySet,
        polls: Mutex<usize>,
    }

    #[async_trait]
    impl LicenseTransport for DeviceFlowTransport {
        async fn start_authorization(
            &self,
            _settings: &LicensingSettings,
        ) -> Result<DeviceCodeResponse> {
            Ok(self.authorization.clone())
        }

        async fn poll(&self, device_code: &str) -> Result<TokenExchange> {
            assert_eq!(device_code, self.authorization.device_code);
            *self.polls.lock().unwrap() += 1;
            self.tokens
                .lock()
                .unwrap()
                .take()
                .map(TokenExchange::Granted)
                .ok_or_else(|| anyhow!("the test token was already consumed"))
        }

        async fn refresh(&self, _refresh_token: &str) -> Result<TokenExchange> {
            bail!("refresh is not used by this scenario")
        }

        async fn entitlement(&self, access_token: &str) -> Result<EntitlementExchange> {
            assert_eq!(access_token, "short-lived-access-token");
            Ok(EntitlementExchange::Issued(EntitlementResponse {
                lease: self.lease.clone(),
            }))
        }

        async fn keys(&self) -> Result<JsonWebKeySet> {
            Ok(self.keys.clone())
        }

        async fn revoke(&self, _refresh_token: &str) -> Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeCredentialStore {
        token: Mutex<Option<String>>,
        deleted: Mutex<bool>,
        load_fails: bool,
        save_fails: bool,
        delete_fails: bool,
    }

    impl CredentialStore for FakeCredentialStore {
        fn load(&self, _origin: &Url) -> Result<Option<String>> {
            if self.load_fails {
                bail!("the test keychain is unavailable");
            }
            Ok(self.token.lock().unwrap().clone())
        }

        fn save(&self, _origin: &Url, refresh_token: &str) -> Result<()> {
            if self.save_fails {
                bail!("the test keychain is unavailable");
            }
            *self.token.lock().unwrap() = Some(refresh_token.into());
            Ok(())
        }

        fn delete(&self, _origin: &Url) -> Result<()> {
            if self.delete_fails {
                bail!("the test keychain is unavailable");
            }
            *self.token.lock().unwrap() = None;
            *self.deleted.lock().unwrap() = true;
            Ok(())
        }
    }

    #[test]
    fn custom_server_requires_a_safe_origin() {
        assert!(validate_origin("https://licenses.example.com").is_ok());
        assert!(validate_origin("https://licenses.example.com/api").is_err());
        assert!(validate_origin("https://user@licenses.example.com").is_err());
        assert!(validate_origin("http://example.com").is_err());
        assert!(validate_origin("http://127.0.0.1:5050").is_ok());
    }

    #[test]
    fn token_response_debug_cannot_expose_credentials() {
        fn assert_not_debug<T>() {
            assert!(!std::any::type_name::<T>().is_empty());
        }
        assert_not_debug::<TokenResponse>();
    }

    #[test]
    fn settings_never_serialize_refresh_tokens() {
        let serialized = serde_json::to_string(&LicensingSettings::default()).unwrap();
        assert!(!serialized.contains("refresh_token"));
        assert!(!serialized.contains("access_token"));
    }

    #[test]
    fn commercial_online_status_is_the_only_status_without_a_warning() {
        let lease = VerifiedLease {
            user_id: Uuid::now_v7(),
            seat_id: Uuid::now_v7(),
            billing_account_id: Uuid::now_v7(),
            installation_id: Uuid::now_v7(),
            activation_id: Uuid::now_v7(),
            state: "commercial".into(),
            reason_code: "active_subscription".into(),
            expires_at: unix_now() + 300,
            refresh_after: unix_now() + 100,
        };
        assert!(!LicenseStatus::Licensed(lease.clone()).shows_warning());
        assert!(LicenseStatus::Offline(lease).shows_warning());
        assert!(LicenseStatus::SignedOut.shows_warning());
    }

    #[test]
    fn retry_delay_is_bounded_to_fifteen_minutes() {
        assert_eq!(MAX_RETRY_DELAY, Duration::from_secs(900));
    }

    #[tokio::test]
    async fn in_flight_license_attempt_yields_promptly_to_a_new_user_command() {
        let (commands, received) = std::sync::mpsc::channel();
        commands
            .send(LicenseCommand::SetDisplayName("Renamed device".into()))
            .unwrap();
        commands
            .send(LicenseCommand::SignOut {
                operation_generation: 2,
            })
            .unwrap();

        let outcome = tokio::time::timeout(
            Duration::from_secs(1),
            run_until_command(std::future::pending::<Result<()>>(), &received),
        )
        .await
        .expect("the in-flight request should be interrupted");

        assert!(matches!(
            outcome,
            LicenseAttemptOutcome::Interrupted {
                command: LicenseCommand::SignOut {
                    operation_generation: 2
                },
                display_name: Some(display_name),
            } if display_name == "Renamed device"
        ));
    }

    #[test]
    fn explicit_sign_out_ignores_a_late_result_from_the_previous_operation() {
        let mut service = LicensingService::default();
        let (events, received) = std::sync::mpsc::channel();
        service.results = Some(received);
        service.status = LicenseStatus::Checking;

        service.sign_out();
        events
            .send(LicenseEventMessage {
                operation_generation: service.operation_generation - 1,
                event: LicenseEvent::Licensed {
                    lease: VerifiedLease {
                        user_id: Uuid::now_v7(),
                        seat_id: Uuid::now_v7(),
                        billing_account_id: Uuid::now_v7(),
                        installation_id: service.settings.installation_id,
                        activation_id: Uuid::now_v7(),
                        state: "commercial".into(),
                        reason_code: "active_subscription".into(),
                        expires_at: unix_now() + 300,
                        refresh_after: unix_now() + 100,
                    },
                    encoded_lease: "stale-lease".into(),
                    keys: JsonWebKeySet { keys: Vec::new() },
                    checked_at: unix_now(),
                },
            })
            .unwrap();

        service.poll(&egui::Context::default());

        assert_eq!(service.status, LicenseStatus::SignedOut);
        assert!(service.settings.signed_out);
        assert!(service.settings.lease.is_none());
        assert!(service.settings.keys.is_none());
    }

    #[test]
    fn validates_the_shared_backend_offline_lease_contract() {
        let golden: GoldenLease = serde_json::from_str(include_str!(
            "../../../licensing/protocol-fixtures/offline-lease-v1.json"
        ))
        .unwrap();
        assert_eq!(golden.audience, LEASE_AUDIENCE);
        let lease = validate_lease_at(
            &golden.lease,
            &golden.keys,
            &Url::parse(&golden.issuer).unwrap(),
            golden.installation_id,
            golden.expected.issued_at,
        )
        .unwrap();
        assert_eq!(lease.state, golden.expected.state);
        assert_eq!(lease.reason_code, golden.expected.reason_code);
        assert_eq!(lease.expires_at, golden.expected.expires_at);
        assert_eq!(lease.refresh_after, golden.expected.refresh_after);
        assert_eq!(
            lease.expires_at - golden.expected.issued_at,
            MAX_LEASE_LIFETIME.as_secs()
        );
        assert!(
            validate_lease_at(
                &golden.lease,
                &golden.keys,
                &Url::parse(&golden.issuer).unwrap(),
                golden.installation_id,
                golden.expected.issued_at - 1,
            )
            .is_err()
        );
        assert!(
            validate_lease_at(
                &golden.lease,
                &golden.keys,
                &Url::parse(&golden.issuer).unwrap(),
                golden.installation_id,
                golden.expected.expires_at,
            )
            .is_err()
        );
        assert!(
            validate_lease_at(
                &golden.lease,
                &golden.keys,
                &Url::parse(&golden.issuer).unwrap(),
                Uuid::now_v7(),
                golden.expected.issued_at,
            )
            .is_err()
        );
        assert!(
            validate_lease_at(
                &golden.lease,
                &golden.keys,
                &Url::parse("https://other.example.com/").unwrap(),
                golden.installation_id,
                golden.expected.issued_at,
            )
            .is_err()
        );
        let mut incompatible_keys = golden.keys.clone();
        incompatible_keys.keys[0].alg = "RS512".into();
        assert!(
            validate_lease_at(
                &golden.lease,
                &incompatible_keys,
                &Url::parse(&golden.issuer).unwrap(),
                golden.installation_id,
                golden.expected.issued_at,
            )
            .is_err()
        );
        let mut tampered_lease = golden.lease.clone();
        let signature_start = tampered_lease
            .rfind('.')
            .expect("the fixture contains a JWT signature")
            + 1;
        let replacement = if &tampered_lease[signature_start..=signature_start] == "A" {
            "B"
        } else {
            "A"
        };
        tampered_lease.replace_range(signature_start..=signature_start, replacement);
        assert!(
            validate_lease_at(
                &tampered_lease,
                &golden.keys,
                &Url::parse(&golden.issuer).unwrap(),
                golden.installation_id,
                golden.expected.issued_at,
            )
            .is_err()
        );
    }

    #[test]
    fn signed_lease_identifiers_require_uuid_shape_without_a_version_restriction() {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct IdentifierFixture {
            #[serde(flatten)]
            golden: GoldenLease,
            malformed_identifier_lease: String,
        }

        let fixture: IdentifierFixture = serde_json::from_str(include_str!(
            "../../../licensing/protocol-fixtures/offline-lease-identifier-shape.json"
        ))
        .unwrap();
        let golden = fixture.golden;
        let origin = Url::parse(&golden.issuer).unwrap();
        let verified = validate_lease_at(
            &golden.lease,
            &golden.keys,
            &origin,
            golden.installation_id,
            golden.expected.issued_at,
        )
        .expect("a signed lease with a UUIDv4 identifier remains valid");
        assert_eq!(verified.installation_id, golden.installation_id);
        assert!(
            validate_lease_at(
                &fixture.malformed_identifier_lease,
                &golden.keys,
                &origin,
                golden.installation_id,
                golden.expected.issued_at,
            )
            .is_err(),
            "a valid signature does not excuse a malformed identifier"
        );
    }

    #[tokio::test]
    async fn device_sign_in_persists_refresh_credentials_and_a_verified_lease() {
        let golden: GoldenLease = serde_json::from_str(include_str!(
            "../../../licensing/protocol-fixtures/offline-lease-v1.json"
        ))
        .unwrap();
        let origin = Url::parse(&golden.issuer).unwrap();
        let transport = DeviceFlowTransport {
            authorization: device_authorization(&origin),
            tokens: Mutex::new(Some(TokenResponse {
                access_token: "short-lived-access-token".into(),
                refresh_token: Some("device-refresh-token".into()),
            })),
            lease: golden.lease,
            keys: golden.keys,
            polls: Mutex::new(0),
        };
        let credentials = FakeCredentialStore::default();
        let mut settings = LicensingSettings {
            server: LicenseServer::Custom(origin.to_string()),
            installation_id: golden.installation_id,
            ..Default::default()
        };
        let mut session_token = None;
        let (events, received) = std::sync::mpsc::channel();
        let clock = FixedUnixClock(golden.expected.issued_at);
        let repaint = egui::Context::default();
        let runtime = LicenseRuntime {
            origin,
            transport: &transport,
            credentials: &credentials,
            clock: &clock,
            events: EventTarget {
                events: &events,
                repaint: &repaint,
                operation_generation: 1,
            },
        };

        sign_in_with(&mut settings, &mut session_token, &runtime)
            .await
            .unwrap();

        assert_eq!(*transport.polls.lock().unwrap(), 1);
        assert_eq!(
            credentials.token.lock().unwrap().as_deref(),
            Some("device-refresh-token")
        );
        assert!(session_token.is_none());
        assert!(settings.lease.is_some());
        assert!(matches!(
            received.recv().unwrap().event,
            LicenseEvent::AwaitingApproval(_)
        ));
        assert!(matches!(
            received.recv().unwrap().event,
            LicenseEvent::Licensed { .. }
        ));
    }

    #[tokio::test]
    async fn device_sign_in_rejects_cross_origin_verification_urls_before_polling() {
        let golden: GoldenLease = serde_json::from_str(include_str!(
            "../../../licensing/protocol-fixtures/offline-lease-v1.json"
        ))
        .unwrap();
        let origin = Url::parse(&golden.issuer).unwrap();
        let mut authorization = device_authorization(&origin);
        authorization.verification_uri_complete =
            "https://attacker.example/authorize?user_code=ABCD-EFGH".into();
        let transport = DeviceFlowTransport {
            authorization,
            tokens: Mutex::new(Some(TokenResponse {
                access_token: "short-lived-access-token".into(),
                refresh_token: Some("device-refresh-token".into()),
            })),
            lease: golden.lease,
            keys: golden.keys,
            polls: Mutex::new(0),
        };
        let credentials = FakeCredentialStore::default();
        let mut settings = LicensingSettings {
            server: LicenseServer::Custom(origin.to_string()),
            installation_id: golden.installation_id,
            ..Default::default()
        };
        let mut session_token = None;
        let (events, received) = std::sync::mpsc::channel();
        let clock = FixedUnixClock(golden.expected.issued_at);
        let repaint = egui::Context::default();
        let runtime = LicenseRuntime {
            origin,
            transport: &transport,
            credentials: &credentials,
            clock: &clock,
            events: EventTarget {
                events: &events,
                repaint: &repaint,
                operation_generation: 1,
            },
        };

        let error = sign_in_with(&mut settings, &mut session_token, &runtime)
            .await
            .unwrap_err();

        assert!(error.to_string().contains("cross-origin verification URL"));
        assert_eq!(*transport.polls.lock().unwrap(), 0);
        assert!(received.try_recv().is_err());
        assert!(credentials.token.lock().unwrap().is_none());
        assert!(settings.lease.is_none());
    }

    #[tokio::test]
    async fn device_sign_in_rejects_a_response_without_refresh_credentials() {
        let golden: GoldenLease = serde_json::from_str(include_str!(
            "../../../licensing/protocol-fixtures/offline-lease-v1.json"
        ))
        .unwrap();
        let origin = Url::parse(&golden.issuer).unwrap();
        let transport = DeviceFlowTransport {
            authorization: device_authorization(&origin),
            tokens: Mutex::new(Some(TokenResponse {
                access_token: "short-lived-access-token".into(),
                refresh_token: None,
            })),
            lease: golden.lease,
            keys: golden.keys,
            polls: Mutex::new(0),
        };
        let credentials = FakeCredentialStore::default();
        let mut settings = LicensingSettings {
            server: LicenseServer::Custom(origin.to_string()),
            installation_id: golden.installation_id,
            ..Default::default()
        };
        let mut session_token = None;
        let (events, received) = std::sync::mpsc::channel();
        let clock = FixedUnixClock(golden.expected.issued_at);
        let repaint = egui::Context::default();
        let runtime = LicenseRuntime {
            origin,
            transport: &transport,
            credentials: &credentials,
            clock: &clock,
            events: EventTarget {
                events: &events,
                repaint: &repaint,
                operation_generation: 1,
            },
        };

        let error = sign_in_with(&mut settings, &mut session_token, &runtime)
            .await
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("did not issue a refresh credential")
        );
        assert_eq!(*transport.polls.lock().unwrap(), 1);
        assert!(credentials.token.lock().unwrap().is_none());
        assert!(settings.lease.is_none());
        assert!(matches!(
            received.recv().unwrap().event,
            LicenseEvent::AwaitingApproval(_)
        ));
        assert!(received.try_recv().is_err());
    }

    fn device_authorization(origin: &Url) -> DeviceCodeResponse {
        DeviceCodeResponse {
            device_code: "device-code".into(),
            user_code: "ABCD-EFGH".into(),
            verification_uri: origin.join("devices/authorize").unwrap().to_string(),
            verification_uri_complete: origin
                .join("devices/authorize?user_code=ABCD-EFGH")
                .unwrap()
                .to_string(),
            expires_in: 60,
            interval: 1,
        }
    }

    #[tokio::test]
    async fn pending_renewal_retains_credentials_and_recovers_without_signing_in() {
        for at_token in [true, false] {
            let golden: GoldenLease = serde_json::from_str(include_str!(
                "../../../licensing/protocol-fixtures/offline-lease-v1.json"
            ))
            .unwrap();
            let origin = Url::parse(&golden.issuer).unwrap();
            let transport = RenewalTransport {
                inner: FakeTransport {
                    lease: golden.lease.clone(),
                    keys: golden.keys.clone(),
                    deny_refresh: false,
                    ineligible_reason: None,
                    unauthorized_entitlement: false,
                    refresh_tokens: Mutex::new(Vec::new()),
                },
                pending: Mutex::new(true),
                at_token,
            };
            let credentials = FakeCredentialStore {
                token: Mutex::new(Some("initial-refresh-token".into())),
                ..Default::default()
            };
            let mut settings = LicensingSettings {
                server: LicenseServer::Custom(origin.to_string()),
                installation_id: golden.installation_id,
                lease: Some(golden.lease.clone()),
                keys: Some(golden.keys),
                ..Default::default()
            };
            let mut session_token = None;
            let (events, received) = std::sync::mpsc::channel();
            let clock = FixedUnixClock(golden.expected.issued_at);
            let repaint = egui::Context::default();
            let runtime = LicenseRuntime {
                origin,
                transport: &transport,
                credentials: &credentials,
                clock: &clock,
                events: EventTarget {
                    events: &events,
                    repaint: &repaint,
                    operation_generation: 1,
                },
            };
            let retained_token = if at_token {
                "initial-refresh-token"
            } else {
                "rotated-refresh-token"
            };
            for _ in 0..2 {
                let error = refresh_with(&mut settings, &mut session_token, &runtime)
                    .await
                    .unwrap_err();
                assert!(error.is::<SubscriptionRenewalPending>());
                assert_eq!(
                    credentials.token.lock().unwrap().as_deref(),
                    Some(retained_token)
                );
                assert!(!*credentials.deleted.lock().unwrap());
                assert_eq!(settings.lease.as_deref(), Some(golden.lease.as_str()));
                assert!(received.try_recv().is_err());
            }

            *transport.pending.lock().unwrap() = false;
            refresh_with(&mut settings, &mut session_token, &runtime)
                .await
                .unwrap();
            assert_eq!(
                transport
                    .inner
                    .refresh_tokens
                    .lock()
                    .unwrap()
                    .last()
                    .unwrap(),
                retained_token
            );
            assert!(matches!(
                received.recv().unwrap().event,
                LicenseEvent::Licensed { .. }
            ));
            assert!(!*credentials.deleted.lock().unwrap());
        }
    }

    #[test]
    fn offline_status_expires_without_waiting_for_a_network_response() {
        let golden: GoldenLease = serde_json::from_str(include_str!(
            "../../../licensing/protocol-fixtures/offline-lease-v1.json"
        ))
        .unwrap();
        let settings = LicensingSettings {
            server: LicenseServer::Custom(golden.issuer),
            installation_id: golden.installation_id,
            lease: Some(golden.lease),
            keys: Some(golden.keys),
            ..Default::default()
        };
        let (events, received) = std::sync::mpsc::channel();
        let mut service = LicensingService {
            settings,
            results: Some(received),
            ..Default::default()
        };
        let error: anyhow::Error = SubscriptionRenewalPending.into();
        events
            .send(LicenseEventMessage {
                operation_generation: 0,
                event: refresh_failure_event(
                    &service.settings,
                    &error,
                    golden.expected.expires_at - 1,
                ),
            })
            .unwrap();
        service.poll_at(golden.expected.expires_at - 1);
        assert!(service.status().lease().is_some());
        events
            .send(LicenseEventMessage {
                operation_generation: 0,
                event: LicenseEvent::Checking,
            })
            .unwrap();
        service.poll_at(golden.expected.expires_at);
        assert!(service.status().lease().is_none());
        assert!(service.status().is_waiting_for_refresh());
        assert!(!service.settings.signed_out);
        assert!(service.settings.lease.is_none());
        assert_eq!(
            refresh_retry_delay(&service.settings, &Err(error), Some(MAX_RETRY_DELAY)),
            Some(MAX_RETRY_DELAY)
        );
        service.sign_out();
        assert!(service.settings.signed_out);
        assert_eq!(service.status(), &LicenseStatus::SignedOut);
    }

    async fn license_http_response(
        status: u16,
        body: &'static str,
    ) -> (HttpLicenseTransport, tokio::task::JoinHandle<()>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let server = tokio::spawn(async move {
            tokio::time::timeout(Duration::from_secs(5), async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut headers = Vec::new();
                while !headers.ends_with(b"\r\n\r\n") {
                    headers.push(stream.read_u8().await.unwrap());
                    assert!(headers.len() < 16_384);
                }
                let headers = String::from_utf8(headers).unwrap();
                let length = headers.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<usize>().unwrap())
                }).unwrap_or(0);
                let mut request_body = vec![0; length];
                stream.read_exact(&mut request_body).await.unwrap();
                let response = format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                stream.write_all(response.as_bytes()).await.unwrap();
            }).await.unwrap();
        });
        (HttpLicenseTransport::new(origin).unwrap(), server)
    }

    #[tokio::test]
    async fn http_transport_distinguishes_retryable_renewal_from_permanent_rejection() {
        let (transport, server) =
            license_http_response(503, r#"{"error":"temporarily_unavailable"}"#).await;
        assert!(matches!(
            transport.refresh("refresh-credential").await.unwrap(),
            TokenExchange::RenewalPending
        ));
        server.await.unwrap();
        let (transport, server) = license_http_response(400, r#"{"error":"invalid_grant"}"#).await;
        assert!(
            matches!(transport.refresh("refresh-credential").await.unwrap(), TokenExchange::Denied(reason) if reason == "invalid_grant")
        );
        server.await.unwrap();
        let (transport, server) =
            license_http_response(503, r#"{"reasonCode":"subscription_renewal_pending"}"#).await;
        assert!(matches!(
            transport.entitlement("access-credential").await.unwrap(),
            EntitlementExchange::RenewalPending
        ));
        server.await.unwrap();
        let (transport, server) =
            license_http_response(503, r#"{"reasonCode":"unavailable"}"#).await;
        assert!(transport.entitlement("access-credential").await.is_err());
        server.await.unwrap();
    }

    #[test]
    fn pending_renewal_never_extends_the_signed_lease_expiry() {
        let golden: GoldenLease = serde_json::from_str(include_str!(
            "../../../licensing/protocol-fixtures/offline-lease-v1.json"
        ))
        .unwrap();
        let settings = LicensingSettings {
            server: LicenseServer::Custom(golden.issuer),
            installation_id: golden.installation_id,
            lease: Some(golden.lease),
            keys: Some(golden.keys),
            ..Default::default()
        };
        let error = SubscriptionRenewalPending.into();
        assert!(
            matches!(refresh_failure_event(&settings, &error, golden.expected.expires_at - 1),
            LicenseEvent::Offline { lease, .. } if lease.expires_at == golden.expected.expires_at)
        );
        assert!(
            matches!(refresh_failure_event(&settings, &error, golden.expected.expires_at),
            LicenseEvent::Evaluation { reason_code } if reason_code == SUBSCRIPTION_RENEWAL_PENDING)
        );
        let mut delay = next_retry_delay(None);
        assert!(!delay.is_zero());
        for _ in 0..32 {
            delay = next_retry_delay(Some(delay));
            assert!(delay <= MAX_RETRY_DELAY);
        }
        assert_eq!(delay, MAX_RETRY_DELAY);
    }

    #[tokio::test]
    async fn refresh_rotates_the_keychain_token_and_persists_a_verified_lease() {
        let golden: GoldenLease = serde_json::from_str(include_str!(
            "../../../licensing/protocol-fixtures/offline-lease-v1.json"
        ))
        .unwrap();
        let origin = Url::parse(&golden.issuer).unwrap();
        let transport = FakeTransport {
            lease: golden.lease,
            keys: golden.keys,
            deny_refresh: false,
            ineligible_reason: None,
            unauthorized_entitlement: false,
            refresh_tokens: Mutex::new(Vec::new()),
        };
        let credentials = FakeCredentialStore {
            token: Mutex::new(Some("initial-refresh-token".into())),
            ..Default::default()
        };
        let mut settings = LicensingSettings {
            server: LicenseServer::Custom(origin.to_string()),
            installation_id: golden.installation_id,
            ..Default::default()
        };
        let mut session_token = None;
        let (events, received) = std::sync::mpsc::channel();
        let clock = FixedUnixClock(golden.expected.issued_at);
        let repaint = egui::Context::default();
        let runtime = LicenseRuntime {
            origin,
            transport: &transport,
            credentials: &credentials,
            clock: &clock,
            events: EventTarget {
                events: &events,
                repaint: &repaint,
                operation_generation: 1,
            },
        };

        refresh_with(&mut settings, &mut session_token, &runtime)
            .await
            .unwrap();

        assert_eq!(
            *transport.refresh_tokens.lock().unwrap(),
            vec!["initial-refresh-token"]
        );
        assert_eq!(
            credentials.token.lock().unwrap().as_deref(),
            Some("rotated-refresh-token")
        );
        assert!(settings.lease.is_some());
        assert!(settings.keys.is_some());
        assert!(matches!(
            received.recv().unwrap().event,
            LicenseEvent::Licensed { .. }
        ));
    }

    #[tokio::test]
    async fn unavailable_keychain_preserves_a_valid_cached_offline_lease() {
        let golden: GoldenLease = serde_json::from_str(include_str!(
            "../../../licensing/protocol-fixtures/offline-lease-v1.json"
        ))
        .unwrap();
        let origin = Url::parse(&golden.issuer).unwrap();
        let transport = FakeTransport {
            lease: golden.lease.clone(),
            keys: golden.keys.clone(),
            deny_refresh: false,
            ineligible_reason: None,
            unauthorized_entitlement: false,
            refresh_tokens: Mutex::new(Vec::new()),
        };
        let credentials = FakeCredentialStore {
            load_fails: true,
            ..Default::default()
        };
        let mut settings = LicensingSettings {
            server: LicenseServer::Custom(origin.to_string()),
            installation_id: golden.installation_id,
            lease: Some(golden.lease),
            keys: Some(golden.keys),
            ..Default::default()
        };
        let mut session_token = None;
        let (events, received) = std::sync::mpsc::channel();
        let clock = FixedUnixClock(golden.expected.issued_at);
        let repaint = egui::Context::default();
        let runtime = LicenseRuntime {
            origin,
            transport: &transport,
            credentials: &credentials,
            clock: &clock,
            events: EventTarget {
                events: &events,
                repaint: &repaint,
                operation_generation: 1,
            },
        };

        let result = refresh_with(&mut settings, &mut session_token, &runtime).await;

        assert!(result.is_err());
        assert!(settings.lease.is_some());
        assert!(settings.keys.is_some());
        assert!(matches!(
            received.recv().unwrap().event,
            LicenseEvent::KeychainUnavailable
        ));
        assert!(received.try_recv().is_err());
    }

    #[tokio::test]
    async fn failed_keychain_rotation_uses_only_the_new_in_memory_token() {
        let golden: GoldenLease = serde_json::from_str(include_str!(
            "../../../licensing/protocol-fixtures/offline-lease-v1.json"
        ))
        .unwrap();
        let origin = Url::parse(&golden.issuer).unwrap();
        let transport = FakeTransport {
            lease: golden.lease,
            keys: golden.keys,
            deny_refresh: false,
            ineligible_reason: None,
            unauthorized_entitlement: false,
            refresh_tokens: Mutex::new(Vec::new()),
        };
        let credentials = FakeCredentialStore {
            token: Mutex::new(Some("initial-refresh-token".into())),
            save_fails: true,
            ..Default::default()
        };
        let mut settings = LicensingSettings {
            server: LicenseServer::Custom(origin.to_string()),
            installation_id: golden.installation_id,
            ..Default::default()
        };
        let mut session_token = None;
        let (events, received) = std::sync::mpsc::channel();
        let clock = FixedUnixClock(golden.expected.issued_at);
        let repaint = egui::Context::default();
        let runtime = LicenseRuntime {
            origin,
            transport: &transport,
            credentials: &credentials,
            clock: &clock,
            events: EventTarget {
                events: &events,
                repaint: &repaint,
                operation_generation: 1,
            },
        };

        refresh_with(&mut settings, &mut session_token, &runtime)
            .await
            .unwrap();

        assert_eq!(session_token.as_deref(), Some("rotated-refresh-token"));
        assert!(credentials.token.lock().unwrap().is_none());
        assert!(matches!(
            received.recv().unwrap().event,
            LicenseEvent::KeychainUnavailable
        ));
        assert!(matches!(
            received.recv().unwrap().event,
            LicenseEvent::Licensed { .. }
        ));
    }

    #[tokio::test]
    async fn ineligible_entitlement_enters_evaluation_without_fetching_signing_keys() {
        let golden: GoldenLease = serde_json::from_str(include_str!(
            "../../../licensing/protocol-fixtures/offline-lease-v1.json"
        ))
        .unwrap();
        let origin = Url::parse(&golden.issuer).unwrap();
        let transport = FakeTransport {
            lease: golden.lease.clone(),
            keys: golden.keys.clone(),
            deny_refresh: false,
            ineligible_reason: Some("no_valid_entitlement".into()),
            unauthorized_entitlement: false,
            refresh_tokens: Mutex::new(Vec::new()),
        };
        let credentials = FakeCredentialStore {
            token: Mutex::new(Some("initial-refresh-token".into())),
            ..Default::default()
        };
        let mut settings = LicensingSettings {
            server: LicenseServer::Custom(origin.to_string()),
            installation_id: golden.installation_id,
            lease: Some(golden.lease),
            keys: Some(golden.keys),
            ..Default::default()
        };
        let mut session_token = None;
        let (events, received) = std::sync::mpsc::channel();
        let clock = FixedUnixClock(golden.expected.issued_at);
        let repaint = egui::Context::default();
        let runtime = LicenseRuntime {
            origin,
            transport: &transport,
            credentials: &credentials,
            clock: &clock,
            events: EventTarget {
                events: &events,
                repaint: &repaint,
                operation_generation: 1,
            },
        };

        refresh_with(&mut settings, &mut session_token, &runtime)
            .await
            .unwrap();

        assert!(*credentials.deleted.lock().unwrap());
        assert!(session_token.is_none());
        assert!(settings.lease.is_none());
        assert!(settings.keys.is_none());
        assert!(matches!(
            received.recv().unwrap().event,
            LicenseEvent::Evaluation { reason_code }
                if reason_code == "no_valid_entitlement"
        ));
        assert!(received.try_recv().is_err());
    }

    #[tokio::test]
    async fn unauthorized_entitlement_signs_out_instead_of_reporting_evaluation_mode() {
        let golden: GoldenLease = serde_json::from_str(include_str!(
            "../../../licensing/protocol-fixtures/offline-lease-v1.json"
        ))
        .unwrap();
        let origin = Url::parse(&golden.issuer).unwrap();
        let transport = FakeTransport {
            lease: golden.lease.clone(),
            keys: golden.keys.clone(),
            deny_refresh: false,
            ineligible_reason: None,
            unauthorized_entitlement: true,
            refresh_tokens: Mutex::new(Vec::new()),
        };
        let credentials = FakeCredentialStore {
            token: Mutex::new(Some("initial-refresh-token".into())),
            ..Default::default()
        };
        let mut settings = LicensingSettings {
            server: LicenseServer::Custom(origin.to_string()),
            installation_id: golden.installation_id,
            lease: Some(golden.lease),
            keys: Some(golden.keys),
            ..Default::default()
        };
        let mut session_token = None;
        let (events, received) = std::sync::mpsc::channel();
        let clock = FixedUnixClock(golden.expected.issued_at);
        let repaint = egui::Context::default();
        let runtime = LicenseRuntime {
            origin,
            transport: &transport,
            credentials: &credentials,
            clock: &clock,
            events: EventTarget {
                events: &events,
                repaint: &repaint,
                operation_generation: 1,
            },
        };

        refresh_with(&mut settings, &mut session_token, &runtime)
            .await
            .unwrap();

        assert!(*credentials.deleted.lock().unwrap());
        assert!(session_token.is_none());
        assert!(settings.lease.is_none());
        assert!(settings.keys.is_none());
        assert!(matches!(
            received.recv().unwrap().event,
            LicenseEvent::SignedOut {
                suppress_auto_refresh: false
            }
        ));
        assert!(received.try_recv().is_err());
    }

    #[tokio::test]
    async fn invalid_grant_clears_the_lease_even_when_credential_deletion_fails() {
        let golden: GoldenLease = serde_json::from_str(include_str!(
            "../../../licensing/protocol-fixtures/offline-lease-v1.json"
        ))
        .unwrap();
        let origin = Url::parse(&golden.issuer).unwrap();
        let transport = FakeTransport {
            lease: golden.lease.clone(),
            keys: golden.keys.clone(),
            deny_refresh: true,
            ineligible_reason: None,
            unauthorized_entitlement: false,
            refresh_tokens: Mutex::new(Vec::new()),
        };
        let credentials = FakeCredentialStore {
            token: Mutex::new(Some("expired-refresh-token".into())),
            delete_fails: true,
            ..Default::default()
        };
        let mut settings = LicensingSettings {
            server: LicenseServer::Custom(origin.to_string()),
            installation_id: golden.installation_id,
            lease: Some(golden.lease),
            keys: Some(golden.keys),
            last_check_unix: Some(unix_now()),
            ..Default::default()
        };
        let mut session_token = None;
        let (events, received) = std::sync::mpsc::channel();
        let clock = FixedUnixClock(golden.expected.issued_at);
        let repaint = egui::Context::default();
        let runtime = LicenseRuntime {
            origin,
            transport: &transport,
            credentials: &credentials,
            clock: &clock,
            events: EventTarget {
                events: &events,
                repaint: &repaint,
                operation_generation: 1,
            },
        };

        refresh_with(&mut settings, &mut session_token, &runtime)
            .await
            .unwrap();

        assert!(!*credentials.deleted.lock().unwrap());
        assert!(settings.lease.is_none());
        assert!(settings.keys.is_none());
        assert!(matches!(
            received.recv().unwrap().event,
            LicenseEvent::KeychainUnavailable
        ));
        assert!(matches!(
            received.recv().unwrap().event,
            LicenseEvent::SignedOut {
                suppress_auto_refresh: false
            }
        ));
    }
}
