use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use reqwest::{Client, StatusCode, Url, redirect::Policy};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const LEASE_AUDIENCE: &str = "styrhous-desktop";
const KEYRING_SERVICE: &str = "io.github.zlepper.styrhous.licensing";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(15 * 60);
const MAX_LEASE_LIFETIME: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// How often a signed-in installation without entitlement checks for a new license.
const EVALUATION_RECHECK_DELAY: Duration = Duration::from_secs(60 * 60);
const LEASE_SCHEMA_VERSION: u8 = 2;
/// How far ahead of the local clock a freshly issued lease may start, absorbing clock drift.
const CLOCK_SKEW_LEEWAY: Duration = Duration::from_secs(5 * 60);
const OFFLINE_LEASE_EXPIRED: &str = "offline_lease_expired";
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
                    // Keep the lease deadline and signed-in controls visible
                    // while a background request is in flight.
                    if self.status.lease().is_none()
                        && !matches!(self.status, LicenseStatus::Evaluation { .. })
                    {
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

    #[cfg(test)]
    pub(crate) fn set_keychain_warning_for_test(&mut self, keychain_warning: bool) {
        self.keychain_warning = keychain_warning;
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
    let mut session_token = None;
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
                    session_token.take(),
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
                    session_token.take(),
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
                        &mut session_token,
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
                        retry_delay = next_check_delay(&settings, &result, retry_delay, unix_now());
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
                        &mut session_token,
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
                        // A failed sign-in waits for the user to try again.
                        retry_delay = result
                            .is_ok()
                            .then(|| next_check_delay(&settings, &result, None, unix_now()))
                            .flatten();
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
    session_token: Option<String>,
    events: &Sender<LicenseEventMessage>,
    repaint: &egui::Context,
    operation_generation: u64,
) {
    let Some(origin) = origin else {
        return;
    };
    let events = EventTarget {
        events,
        repaint,
        operation_generation,
    };
    match HttpLicenseTransport::new(origin.clone()) {
        Ok(transport) => {
            let runtime = LicenseRuntime {
                origin,
                transport: &transport,
                credentials: &KeyringCredentialStore,
                clock: &SystemUnixClock,
                events,
            };
            revoke_and_delete_session_with(session_token, &runtime).await;
        }
        Err(_) => forget_credential(&KeyringCredentialStore, &origin, events),
    }
}

/// Revokes the installation on the server (best effort) and forgets its local credential.
async fn revoke_and_delete_session_with<T: LicenseTransport, C: CredentialStore>(
    session_token: Option<String>,
    runtime: &LicenseRuntime<'_, T, C, impl UnixClock>,
) {
    let token = session_token.or_else(|| runtime.credentials.load(&runtime.origin).ok().flatten());
    if let Some(token) = token {
        let _ = runtime.transport.sign_out(&token).await;
    }
    forget_credential(runtime.credentials, &runtime.origin, runtime.events);
}

fn forget_credential(credentials: &impl CredentialStore, origin: &Url, events: EventTarget<'_>) {
    if credentials.delete(origin).is_err() {
        events.send(LicenseEvent::KeychainUnavailable);
    }
}

/// Where a successful license check left this installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionState {
    Licensed,
    /// Signed in without entitlement; the app periodically checks again.
    Evaluation,
    SignedOut,
}

enum LicenseAttemptOutcome {
    Completed {
        result: Result<SessionState>,
        display_name: Option<String>,
    },
    Interrupted {
        command: LicenseCommand,
        display_name: Option<String>,
    },
    Disconnected,
}

async fn run_until_command(
    attempt: impl Future<Output = Result<SessionState>>,
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
    session_token: &mut Option<String>,
    events: &Sender<LicenseEventMessage>,
    repaint: &egui::Context,
    operation_generation: u64,
) -> Result<SessionState> {
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
    refresh_with(settings, session_token, &runtime).await
}

async fn refresh_with<T: LicenseTransport, C: CredentialStore>(
    settings: &mut LicensingSettings,
    session_token: &mut Option<String>,
    runtime: &LicenseRuntime<'_, T, C, impl UnixClock>,
) -> Result<SessionState> {
    let token = session_token.clone().or_else(|| {
        runtime
            .credentials
            .load(&runtime.origin)
            .inspect_err(|_| runtime.events.send(LicenseEvent::KeychainUnavailable))
            .ok()
            .flatten()
    });
    let Some(token) = token else {
        // Keep a still-valid offline lease rather than signing out over a missing credential.
        if cached_lease_at(settings, runtime.clock.now()).is_some() {
            bail!("no installation credential is available for the cached offline lease");
        }
        runtime.events.send(LicenseEvent::SignedOut {
            suppress_auto_refresh: false,
        });
        return Ok(SessionState::SignedOut);
    };
    request_lease(settings, &token, session_token, runtime).await
}

async fn sign_in(
    settings: &mut LicensingSettings,
    session_token: &mut Option<String>,
    events: &Sender<LicenseEventMessage>,
    repaint: &egui::Context,
    operation_generation: u64,
) -> Result<SessionState> {
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
    sign_in_with(settings, session_token, &runtime).await
}

async fn sign_in_with<T: LicenseTransport, C: CredentialStore>(
    settings: &mut LicensingSettings,
    session_token: &mut Option<String>,
    runtime: &LicenseRuntime<'_, T, C, impl UnixClock>,
) -> Result<SessionState> {
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
    let deadline = tokio::time::Instant::now() + Duration::from_secs(authorization.expires_in);
    let mut interval = Duration::from_secs(authorization.interval.max(1));
    loop {
        if tokio::time::Instant::now() >= deadline {
            bail!("the device authorization expired");
        }
        tokio::time::sleep(interval).await;
        let exchange = match runtime.transport.poll(&authorization.device_code).await {
            Ok(exchange) => exchange,
            Err(error) => {
                // A transient failure must not discard an approval the user may be giving now.
                tracing::warn!(error = %error, "device authorization poll failed; retrying");
                continue;
            }
        };
        match exchange {
            TokenExchange::Granted(token) => {
                store_installation_token(&token.installation_token, session_token, runtime);
                return request_lease(settings, &token.installation_token, session_token, runtime)
                    .await;
            }
            TokenExchange::Pending(error) if error == "slow_down" => {
                interval += Duration::from_secs(5);
            }
            TokenExchange::Pending(_) => {}
            TokenExchange::Denied(error) => bail!("device authorization was denied: {error}"),
        }
    }
}

/// Keeps the installation token in the OS keychain, or only in memory when it is unavailable.
fn store_installation_token<T, C>(
    token: &str,
    session_token: &mut Option<String>,
    runtime: &LicenseRuntime<'_, T, C, impl UnixClock>,
) where
    C: CredentialStore,
{
    match runtime.credentials.save(&runtime.origin, token) {
        Ok(()) => *session_token = None,
        Err(_) => {
            let _ = runtime.credentials.delete(&runtime.origin);
            *session_token = Some(token.into());
            runtime.events.send(LicenseEvent::KeychainUnavailable);
        }
    }
}

async fn request_lease<T, C>(
    settings: &mut LicensingSettings,
    token: &str,
    session_token: &mut Option<String>,
    runtime: &LicenseRuntime<'_, T, C, impl UnixClock>,
) -> Result<SessionState>
where
    T: LicenseTransport,
    C: CredentialStore,
{
    match runtime.transport.entitlement(token).await? {
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
            Ok(SessionState::Licensed)
        }
        EntitlementExchange::Ineligible(reason_code) => {
            // The installation stays signed in so a later purchase is picked up automatically.
            settings.clear_account();
            runtime
                .events
                .send(LicenseEvent::Evaluation { reason_code });
            Ok(SessionState::Evaluation)
        }
        EntitlementExchange::Unauthorized => {
            forget_credential(runtime.credentials, &runtime.origin, runtime.events);
            *session_token = None;
            settings.clear_account();
            runtime.events.send(LicenseEvent::SignedOut {
                suppress_auto_refresh: false,
            });
            Ok(SessionState::SignedOut)
        }
    }
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
    async fn entitlement(&self, installation_token: &str) -> Result<EntitlementExchange>;
    async fn keys(&self) -> Result<JsonWebKeySet>;
    async fn sign_out(&self, installation_token: &str) -> Result<()>;
}

trait CredentialStore {
    fn load(&self, origin: &Url) -> Result<Option<String>>;
    fn save(&self, origin: &Url, installation_token: &str) -> Result<()>;
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

    fn save(&self, origin: &Url, installation_token: &str) -> Result<()> {
        keyring::Entry::new(KEYRING_SERVICE, origin.as_str())?.set_password(installation_token)?;
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
            .post(self.endpoint("desktop/v2/device/authorize")?)
            .form(&[
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
        let response = self
            .client
            .post(self.endpoint("desktop/v2/token")?)
            .form(&[("device_code", device_code)])
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
        if matches!(error.error.as_str(), "authorization_pending" | "slow_down") {
            Ok(TokenExchange::Pending(error.error))
        } else {
            Ok(TokenExchange::Denied(error.error))
        }
    }

    async fn entitlement(&self, installation_token: &str) -> Result<EntitlementExchange> {
        let response = self
            .client
            .post(self.endpoint("desktop/v2/entitlement")?)
            .bearer_auth(installation_token)
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
        if status == StatusCode::UNAUTHORIZED {
            return Ok(EntitlementExchange::Unauthorized);
        }
        if status == StatusCode::FORBIDDEN {
            let error: DesktopError = response
                .json()
                .await
                .context("invalid entitlement error response")?;
            return Ok(EntitlementExchange::Ineligible(error.reason_code));
        }
        Err(response
            .error_for_status()
            .expect_err("status is not successful")
            .into())
    }

    async fn keys(&self) -> Result<JsonWebKeySet> {
        self.client
            .get(self.endpoint("desktop/v2/keys")?)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .context("invalid licensing key set")
    }

    async fn sign_out(&self, installation_token: &str) -> Result<()> {
        self.client
            .post(self.endpoint("desktop/v2/sign-out")?)
            .bearer_auth(installation_token)
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

/// Intentionally not `Debug`: it carries the installation credential.
#[derive(Deserialize)]
struct TokenResponse {
    installation_token: String,
}

enum TokenExchange {
    Granted(TokenResponse),
    Pending(String),
    Denied(String),
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
}

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

/// An Ed25519 lease verification key (RFC 8037).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct JsonWebKey {
    kty: String,
    crv: String,
    #[serde(rename = "use")]
    usage: String,
    alg: String,
    kid: String,
    x: String,
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
    if header.alg != Algorithm::EdDSA {
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
    if key.kty != "OKP" || key.crv != "Ed25519" || key.usage != "sig" || key.alg != "EdDSA" {
        bail!("the lease signing key has incompatible metadata");
    }
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.set_audience(&[LEASE_AUDIENCE]);
    validation.set_issuer(&[origin.as_str()]);
    validation.validate_exp = false;
    validation.validate_nbf = false;
    validation.leeway = 0;
    validation.set_required_spec_claims(&["exp", "iat", "nbf", "jti", "aud", "iss", "sub"]);
    let data = decode::<LeaseClaims>(
        token,
        &DecodingKey::from_ed_components(&key.x)?,
        &validation,
    )
    .context("the lease signature or registered claims are invalid")?;
    let claims = data.claims;
    let lease_lifetime = claims.exp.checked_sub(claims.iat);
    let valid_jwt_id = Uuid::parse_str(&claims.jti).is_ok();
    if claims.schema_version != LEASE_SCHEMA_VERSION
        || claims.iss != origin.as_str()
        || claims.aud != LEASE_AUDIENCE
        || claims.installation_id != installation_id
        || claims.signing_key_id != key_id
        || !matches!(claims.state.as_str(), "trial" | "commercial" | "grace")
        || claims.iat > claims.nbf
        || claims.nbf > observed_at.saturating_add(CLOCK_SKEW_LEEWAY.as_secs())
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

/// When to check the license next: back off after failures, follow the lease's refresh
/// time, recheck evaluation periodically, and stay idle while signed out.
fn next_check_delay(
    settings: &LicensingSettings,
    result: &Result<SessionState>,
    previous: Option<Duration>,
    now: u64,
) -> Option<Duration> {
    match result {
        Err(_) => Some(next_retry_delay(previous)),
        Ok(SessionState::Licensed) => scheduled_refresh_delay(settings, now),
        Ok(SessionState::Evaluation) => Some(EVALUATION_RECHECK_DELAY),
        Ok(SessionState::SignedOut) => None,
    }
}

fn next_retry_delay(previous: Option<Duration>) -> Duration {
    previous
        .map(|delay| (delay * 2).min(MAX_RETRY_DELAY))
        .unwrap_or(Duration::from_secs(30))
}

fn scheduled_refresh_delay(settings: &LicensingSettings, now: u64) -> Option<Duration> {
    let lease = cached_lease_at(settings, now)?;
    Some(Duration::from_secs(
        lease.refresh_after.saturating_sub(now).max(1),
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

    fn golden() -> GoldenLease {
        serde_json::from_str(include_str!(
            "../../../licensing/protocol-fixtures/offline-lease-v2.json"
        ))
        .unwrap()
    }

    fn settings_for(golden: &GoldenLease, cached_lease: bool) -> LicensingSettings {
        LicensingSettings {
            server: LicenseServer::Custom(golden.issuer.clone()),
            installation_id: golden.installation_id,
            lease: cached_lease.then(|| golden.lease.clone()),
            keys: cached_lease.then(|| golden.keys.clone()),
            ..Default::default()
        }
    }

    struct FixedUnixClock(u64);

    impl UnixClock for FixedUnixClock {
        fn now(&self) -> u64 {
            self.0
        }
    }

    /// The wiring shared by every worker scenario; events are collected in `received`.
    struct Harness<T> {
        origin: Url,
        transport: T,
        credentials: FakeCredentialStore,
        clock: FixedUnixClock,
        events: Sender<LicenseEventMessage>,
        received: Receiver<LicenseEventMessage>,
        repaint: egui::Context,
    }

    impl<T: LicenseTransport> Harness<T> {
        fn new(golden: &GoldenLease, transport: T, credentials: FakeCredentialStore) -> Self {
            let (events, received) = std::sync::mpsc::channel();
            Self {
                origin: Url::parse(&golden.issuer).unwrap(),
                transport,
                credentials,
                clock: FixedUnixClock(golden.expected.issued_at),
                events,
                received,
                repaint: egui::Context::default(),
            }
        }

        fn runtime(&self) -> LicenseRuntime<'_, T, FakeCredentialStore, FixedUnixClock> {
            LicenseRuntime {
                origin: self.origin.clone(),
                transport: &self.transport,
                credentials: &self.credentials,
                clock: &self.clock,
                events: EventTarget {
                    events: &self.events,
                    repaint: &self.repaint,
                    operation_generation: 1,
                },
            }
        }

        fn next_event(&self) -> LicenseEvent {
            self.received.recv().unwrap().event
        }

        fn assert_no_more_events(&self) {
            assert!(self.received.try_recv().is_err());
        }
    }

    enum EntitlementOutcome {
        Issued,
        Ineligible(&'static str),
        Unauthorized,
    }

    struct FakeTransport {
        lease: String,
        keys: JsonWebKeySet,
        entitlement: EntitlementOutcome,
        authorization: Option<DeviceCodeResponse>,
        poll_results: Mutex<Vec<Result<TokenExchange>>>,
        polls: Mutex<usize>,
        entitlement_tokens: Mutex<Vec<String>>,
        signed_out_tokens: Mutex<Vec<String>>,
        sign_out_fails: bool,
    }

    impl FakeTransport {
        fn new(golden: &GoldenLease, entitlement: EntitlementOutcome) -> Self {
            Self {
                lease: golden.lease.clone(),
                keys: golden.keys.clone(),
                entitlement,
                authorization: None,
                poll_results: Mutex::new(Vec::new()),
                polls: Mutex::new(0),
                entitlement_tokens: Mutex::new(Vec::new()),
                signed_out_tokens: Mutex::new(Vec::new()),
                sign_out_fails: false,
            }
        }

        /// A device flow that is approved on the first poll.
        fn approving(golden: &GoldenLease) -> Self {
            Self::approving_with(golden, EntitlementOutcome::Issued)
        }

        fn approving_with(golden: &GoldenLease, entitlement: EntitlementOutcome) -> Self {
            let origin = Url::parse(&golden.issuer).unwrap();
            Self {
                authorization: Some(device_authorization(&origin)),
                poll_results: Mutex::new(vec![Ok(granted())]),
                ..Self::new(golden, entitlement)
            }
        }

        fn polls(&self) -> usize {
            *self.polls.lock().unwrap()
        }
    }

    #[async_trait]
    impl LicenseTransport for FakeTransport {
        async fn start_authorization(
            &self,
            _settings: &LicensingSettings,
        ) -> Result<DeviceCodeResponse> {
            self.authorization
                .clone()
                .ok_or_else(|| anyhow!("device authorization is not used by this scenario"))
        }

        async fn poll(&self, device_code: &str) -> Result<TokenExchange> {
            assert_eq!(device_code, "device-code");
            *self.polls.lock().unwrap() += 1;
            let mut results = self.poll_results.lock().unwrap();
            if results.is_empty() {
                bail!("the test device flow has no more poll results");
            }
            results.remove(0)
        }

        async fn entitlement(&self, installation_token: &str) -> Result<EntitlementExchange> {
            self.entitlement_tokens
                .lock()
                .unwrap()
                .push(installation_token.into());
            Ok(match self.entitlement {
                EntitlementOutcome::Issued => EntitlementExchange::Issued(EntitlementResponse {
                    lease: self.lease.clone(),
                }),
                EntitlementOutcome::Ineligible(reason_code) => {
                    EntitlementExchange::Ineligible(reason_code.into())
                }
                EntitlementOutcome::Unauthorized => EntitlementExchange::Unauthorized,
            })
        }

        async fn keys(&self) -> Result<JsonWebKeySet> {
            assert!(
                matches!(self.entitlement, EntitlementOutcome::Issued),
                "signing keys must not be requested after entitlement rejection"
            );
            Ok(self.keys.clone())
        }

        async fn sign_out(&self, installation_token: &str) -> Result<()> {
            self.signed_out_tokens
                .lock()
                .unwrap()
                .push(installation_token.into());
            if self.sign_out_fails {
                bail!("the test licensing service is unreachable");
            }
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

    impl FakeCredentialStore {
        fn holding(token: &str) -> Self {
            Self {
                token: Mutex::new(Some(token.into())),
                ..Default::default()
            }
        }
    }

    impl CredentialStore for FakeCredentialStore {
        fn load(&self, _origin: &Url) -> Result<Option<String>> {
            if self.load_fails {
                bail!("the test keychain is unavailable");
            }
            Ok(self.token.lock().unwrap().clone())
        }

        fn save(&self, _origin: &Url, installation_token: &str) -> Result<()> {
            if self.save_fails {
                bail!("the test keychain is unavailable");
            }
            *self.token.lock().unwrap() = Some(installation_token.into());
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

    fn granted() -> TokenExchange {
        TokenExchange::Granted(TokenResponse {
            installation_token: "installation-token".into(),
        })
    }

    fn device_authorization(origin: &Url) -> DeviceCodeResponse {
        DeviceCodeResponse {
            device_code: "device-code".into(),
            user_code: "BCDF-GHJK".into(),
            verification_uri: origin.join("devices/authorize").unwrap().to_string(),
            verification_uri_complete: origin
                .join("devices/authorize?user_code=BCDF-GHJK")
                .unwrap()
                .to_string(),
            expires_in: 60,
            interval: 1,
        }
    }

    fn test_lease(installation_id: Uuid) -> VerifiedLease {
        VerifiedLease {
            user_id: Uuid::now_v7(),
            seat_id: Uuid::now_v7(),
            billing_account_id: Uuid::now_v7(),
            installation_id,
            activation_id: Uuid::now_v7(),
            state: "commercial".into(),
            reason_code: "active_subscription".into(),
            expires_at: unix_now() + 300,
            refresh_after: unix_now() + 100,
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
    fn settings_never_serialize_installation_credentials() {
        let serialized = serde_json::to_string(&LicensingSettings::default()).unwrap();
        assert!(!serialized.contains("token"));
    }

    #[test]
    fn commercial_online_status_is_the_only_status_without_a_warning() {
        let lease = test_lease(Uuid::now_v7());
        assert!(!LicenseStatus::Licensed(lease.clone()).shows_warning());
        assert!(LicenseStatus::Offline(lease).shows_warning());
        assert!(LicenseStatus::SignedOut.shows_warning());
    }

    #[test]
    fn check_scheduling_follows_the_session_state() {
        let golden = golden();
        let settings = settings_for(&golden, false);
        assert_eq!(
            next_check_delay(&settings, &Ok(SessionState::Evaluation), None, 0),
            Some(EVALUATION_RECHECK_DELAY)
        );
        assert_eq!(
            next_check_delay(&settings, &Ok(SessionState::SignedOut), None, 0),
            None
        );
        let mut delay = None;
        for _ in 0..32 {
            delay = next_check_delay(&settings, &Err(anyhow!("offline")), delay, 0);
            assert!(delay.unwrap() <= MAX_RETRY_DELAY);
        }
        assert_eq!(delay, Some(MAX_RETRY_DELAY));
    }

    #[test]
    fn licensed_checks_follow_the_lease_refresh_time() {
        let golden = golden();
        let licensed = Ok(SessionState::Licensed);
        let issued_at = golden.expected.issued_at;
        assert_eq!(
            next_check_delay(&settings_for(&golden, true), &licensed, None, issued_at),
            Some(Duration::from_secs(
                golden.expected.refresh_after - issued_at
            ))
        );
        assert_eq!(
            next_check_delay(
                &settings_for(&golden, true),
                &licensed,
                None,
                golden.expected.refresh_after + 10
            ),
            Some(Duration::from_secs(1)),
            "an overdue refresh is scheduled promptly"
        );
        assert_eq!(
            next_check_delay(&settings_for(&golden, false), &licensed, None, issued_at),
            None
        );
        assert_eq!(
            next_check_delay(
                &settings_for(&golden, true),
                &licensed,
                None,
                golden.expected.expires_at
            ),
            None,
            "an expired lease schedules nothing"
        );
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
            run_until_command(std::future::pending::<Result<SessionState>>(), &received),
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
                    lease: test_lease(service.settings.installation_id),
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
        let golden = golden();
        assert_eq!(golden.audience, LEASE_AUDIENCE);
        let origin = Url::parse(&golden.issuer).unwrap();
        let validate = |lease: &str, keys: &JsonWebKeySet, origin: &Url, installation_id, at| {
            validate_lease_at(lease, keys, origin, installation_id, at)
        };
        let lease = validate(
            &golden.lease,
            &golden.keys,
            &origin,
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

        let issued_at = golden.expected.issued_at;
        let leeway = CLOCK_SKEW_LEEWAY.as_secs();
        for (local_clock, accepted) in [
            (issued_at - 60, true),
            (issued_at - leeway, true),
            (issued_at - leeway - 1, false),
        ] {
            assert_eq!(
                validate(
                    &golden.lease,
                    &golden.keys,
                    &origin,
                    golden.installation_id,
                    local_clock
                )
                .is_ok(),
                accepted,
                "a lease issued {}s ahead of the local clock",
                issued_at - local_clock
            );
        }
        assert!(
            validate(
                &golden.lease,
                &golden.keys,
                &origin,
                golden.installation_id,
                golden.expected.expires_at
            )
            .is_err()
        );
        assert!(
            validate(
                &golden.lease,
                &golden.keys,
                &origin,
                Uuid::now_v7(),
                issued_at
            )
            .is_err()
        );
        let other_origin = Url::parse("https://other.example.com/").unwrap();
        assert!(
            validate(
                &golden.lease,
                &golden.keys,
                &other_origin,
                golden.installation_id,
                issued_at
            )
            .is_err()
        );

        let mut incompatible_keys = golden.keys.clone();
        incompatible_keys.keys[0].alg = "RS256".into();
        assert!(
            validate(
                &golden.lease,
                &incompatible_keys,
                &origin,
                golden.installation_id,
                issued_at
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
            validate(
                &tampered_lease,
                &golden.keys,
                &origin,
                golden.installation_id,
                issued_at
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
    async fn device_sign_in_persists_the_installation_token_and_a_verified_lease() {
        let golden = golden();
        let harness = Harness::new(
            &golden,
            FakeTransport::approving(&golden),
            FakeCredentialStore::default(),
        );
        let mut settings = settings_for(&golden, false);
        let mut session_token = None;

        let state = sign_in_with(&mut settings, &mut session_token, &harness.runtime())
            .await
            .unwrap();

        assert_eq!(state, SessionState::Licensed);
        assert_eq!(*harness.transport.polls.lock().unwrap(), 1);
        assert_eq!(
            harness.credentials.token.lock().unwrap().as_deref(),
            Some("installation-token")
        );
        assert_eq!(
            *harness.transport.entitlement_tokens.lock().unwrap(),
            vec!["installation-token"]
        );
        assert!(session_token.is_none());
        assert!(settings.lease.is_some());
        assert!(matches!(
            harness.next_event(),
            LicenseEvent::AwaitingApproval(_)
        ));
        assert!(matches!(
            harness.next_event(),
            LicenseEvent::Licensed { .. }
        ));
    }

    #[tokio::test]
    async fn device_sign_in_keeps_polling_while_approval_is_pending() {
        let golden = golden();
        let transport = FakeTransport::approving(&golden);
        transport.poll_results.lock().unwrap().insert(
            0,
            Ok(TokenExchange::Pending("authorization_pending".into())),
        );
        let harness = Harness::new(&golden, transport, FakeCredentialStore::default());
        let mut settings = settings_for(&golden, false);

        sign_in_with(&mut settings, &mut None, &harness.runtime())
            .await
            .unwrap();

        assert_eq!(*harness.transport.polls.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn device_sign_in_reports_a_denied_authorization() {
        let golden = golden();
        let transport = FakeTransport::approving(&golden);
        *transport.poll_results.lock().unwrap() =
            vec![Ok(TokenExchange::Denied("access_denied".into()))];
        let harness = Harness::new(&golden, transport, FakeCredentialStore::default());

        let error = sign_in_with(
            &mut settings_for(&golden, false),
            &mut None,
            &harness.runtime(),
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("access_denied"));
        assert!(harness.credentials.token.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn device_sign_in_rejects_cross_origin_verification_urls_before_polling() {
        let golden = golden();
        let mut transport = FakeTransport::approving(&golden);
        transport
            .authorization
            .as_mut()
            .unwrap()
            .verification_uri_complete =
            "https://attacker.example/authorize?user_code=BCDF-GHJK".into();
        let harness = Harness::new(&golden, transport, FakeCredentialStore::default());
        let mut settings = settings_for(&golden, false);

        let error = sign_in_with(&mut settings, &mut None, &harness.runtime())
            .await
            .unwrap_err();

        assert!(error.to_string().contains("cross-origin verification URL"));
        assert_eq!(*harness.transport.polls.lock().unwrap(), 0);
        harness.assert_no_more_events();
        assert!(harness.credentials.token.lock().unwrap().is_none());
        assert!(settings.lease.is_none());
    }

    #[tokio::test]
    async fn unavailable_keychain_keeps_the_new_installation_token_in_memory() {
        let golden = golden();
        let harness = Harness::new(
            &golden,
            FakeTransport::approving(&golden),
            FakeCredentialStore {
                save_fails: true,
                ..Default::default()
            },
        );
        let mut settings = settings_for(&golden, false);
        let mut session_token = None;

        sign_in_with(&mut settings, &mut session_token, &harness.runtime())
            .await
            .unwrap();
        assert_eq!(session_token.as_deref(), Some("installation-token"));
        assert!(matches!(
            harness.next_event(),
            LicenseEvent::AwaitingApproval(_)
        ));
        assert!(matches!(
            harness.next_event(),
            LicenseEvent::KeychainUnavailable
        ));
        assert!(matches!(
            harness.next_event(),
            LicenseEvent::Licensed { .. }
        ));

        refresh_with(&mut settings, &mut session_token, &harness.runtime())
            .await
            .unwrap();
        assert_eq!(
            harness
                .transport
                .entitlement_tokens
                .lock()
                .unwrap()
                .last()
                .unwrap(),
            "installation-token"
        );
    }

    #[tokio::test]
    async fn refresh_requests_a_lease_with_the_keychain_token() {
        let golden = golden();
        let harness = Harness::new(
            &golden,
            FakeTransport::new(&golden, EntitlementOutcome::Issued),
            FakeCredentialStore::holding("stored-token"),
        );
        let mut settings = settings_for(&golden, false);

        let state = refresh_with(&mut settings, &mut None, &harness.runtime())
            .await
            .unwrap();

        assert_eq!(state, SessionState::Licensed);
        assert_eq!(
            *harness.transport.entitlement_tokens.lock().unwrap(),
            vec!["stored-token"]
        );
        assert!(settings.lease.is_some());
        assert!(settings.keys.is_some());
        assert!(matches!(
            harness.next_event(),
            LicenseEvent::Licensed { .. }
        ));
    }

    #[tokio::test]
    async fn refresh_without_a_credential_signs_out() {
        let golden = golden();
        let harness = Harness::new(
            &golden,
            FakeTransport::new(&golden, EntitlementOutcome::Issued),
            FakeCredentialStore::default(),
        );

        let state = refresh_with(
            &mut settings_for(&golden, false),
            &mut None,
            &harness.runtime(),
        )
        .await
        .unwrap();

        assert_eq!(state, SessionState::SignedOut);
        assert!(matches!(
            harness.next_event(),
            LicenseEvent::SignedOut {
                suppress_auto_refresh: false
            }
        ));
        assert!(
            harness
                .transport
                .entitlement_tokens
                .lock()
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn unavailable_keychain_preserves_a_valid_cached_offline_lease() {
        let golden = golden();
        let harness = Harness::new(
            &golden,
            FakeTransport::new(&golden, EntitlementOutcome::Issued),
            FakeCredentialStore {
                load_fails: true,
                ..Default::default()
            },
        );
        let mut settings = settings_for(&golden, true);

        let result = refresh_with(&mut settings, &mut None, &harness.runtime()).await;

        assert!(result.is_err());
        assert!(settings.lease.is_some());
        assert!(settings.keys.is_some());
        assert!(matches!(
            harness.next_event(),
            LicenseEvent::KeychainUnavailable
        ));
        harness.assert_no_more_events();
    }

    #[tokio::test]
    async fn ineligible_entitlement_enters_evaluation_but_stays_signed_in() {
        let golden = golden();
        let harness = Harness::new(
            &golden,
            FakeTransport::new(&golden, EntitlementOutcome::Ineligible("trial_expired")),
            FakeCredentialStore::holding("stored-token"),
        );
        let mut settings = settings_for(&golden, true);

        let state = refresh_with(&mut settings, &mut None, &harness.runtime())
            .await
            .unwrap();

        assert_eq!(state, SessionState::Evaluation);
        assert!(!*harness.credentials.deleted.lock().unwrap());
        assert_eq!(
            harness.credentials.token.lock().unwrap().as_deref(),
            Some("stored-token")
        );
        assert!(settings.lease.is_none());
        assert!(settings.keys.is_none());
        assert!(matches!(
            harness.next_event(),
            LicenseEvent::Evaluation { reason_code } if reason_code == "trial_expired"
        ));
        harness.assert_no_more_events();
    }

    #[tokio::test]
    async fn revoked_installation_token_signs_out_and_forgets_the_credential() {
        for delete_fails in [false, true] {
            let golden = golden();
            let harness = Harness::new(
                &golden,
                FakeTransport::new(&golden, EntitlementOutcome::Unauthorized),
                FakeCredentialStore {
                    delete_fails,
                    ..FakeCredentialStore::holding("revoked-token")
                },
            );
            let mut settings = settings_for(&golden, true);
            let mut session_token = Some("revoked-token".to_string());

            let state = refresh_with(&mut settings, &mut session_token, &harness.runtime())
                .await
                .unwrap();

            assert_eq!(state, SessionState::SignedOut);
            assert!(session_token.is_none());
            assert_eq!(*harness.credentials.deleted.lock().unwrap(), !delete_fails);
            assert!(settings.lease.is_none());
            assert!(settings.keys.is_none());
            if delete_fails {
                assert!(matches!(
                    harness.next_event(),
                    LicenseEvent::KeychainUnavailable
                ));
            }
            assert!(matches!(
                harness.next_event(),
                LicenseEvent::SignedOut {
                    suppress_auto_refresh: false
                }
            ));
        }
    }

    #[test]
    fn offline_status_expires_without_waiting_for_a_network_response() {
        let golden = golden();
        let (events, received) = std::sync::mpsc::channel();
        let mut service = LicensingService {
            settings: settings_for(&golden, true),
            results: Some(received),
            ..Default::default()
        };
        let error = anyhow!("the licensing service is unreachable");
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
        assert!(
            matches!(service.status(), LicenseStatus::Offline(lease) if lease.expires_at == golden.expected.expires_at)
        );

        events
            .send(LicenseEventMessage {
                operation_generation: 0,
                event: LicenseEvent::Checking,
            })
            .unwrap();
        service.poll_at(golden.expected.expires_at);
        assert_eq!(
            service.status(),
            &LicenseStatus::Evaluation {
                reason_code: OFFLINE_LEASE_EXPIRED.into()
            }
        );
        assert!(!service.settings.signed_out);
        assert!(service.settings.lease.is_none());
        assert!(matches!(
            refresh_failure_event(&service.settings, &error, golden.expected.expires_at),
            LicenseEvent::Failed(_)
        ));
        service.sign_out();
        assert!(service.settings.signed_out);
        assert_eq!(service.status(), &LicenseStatus::SignedOut);
    }

    async fn license_http_response(
        status: u16,
        body: &'static str,
    ) -> (HttpLicenseTransport, tokio::task::JoinHandle<String>) {
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
                headers + &String::from_utf8(request_body).unwrap()
            }).await.unwrap()
        });
        (HttpLicenseTransport::new(origin).unwrap(), server)
    }

    #[tokio::test]
    async fn http_transport_maps_protocol_responses() {
        let (transport, server) = license_http_response(400, r#"{"error":"slow_down"}"#).await;
        assert!(
            matches!(transport.poll("device-code").await.unwrap(), TokenExchange::Pending(error) if error == "slow_down")
        );
        server.await.unwrap();

        let (transport, server) = license_http_response(400, r#"{"error":"expired_token"}"#).await;
        assert!(
            matches!(transport.poll("device-code").await.unwrap(), TokenExchange::Denied(error) if error == "expired_token")
        );
        server.await.unwrap();

        let (transport, server) =
            license_http_response(200, r#"{"installation_token":"token"}"#).await;
        assert!(
            matches!(transport.poll("device-code").await.unwrap(), TokenExchange::Granted(token) if token.installation_token == "token")
        );
        server.await.unwrap();

        let (transport, server) =
            license_http_response(401, r#"{"reasonCode":"invalid_installation_token"}"#).await;
        assert!(matches!(
            transport.entitlement("token").await.unwrap(),
            EntitlementExchange::Unauthorized
        ));
        server.await.unwrap();

        let (transport, server) =
            license_http_response(403, r#"{"reasonCode":"trial_expired"}"#).await;
        assert!(
            matches!(transport.entitlement("token").await.unwrap(), EntitlementExchange::Ineligible(reason) if reason == "trial_expired")
        );
        server.await.unwrap();

        let (transport, server) =
            license_http_response(503, r#"{"reasonCode":"unavailable"}"#).await;
        assert!(transport.entitlement("token").await.is_err());
        server.await.unwrap();

        let (transport, server) = license_http_response(403, "not json").await;
        assert!(transport.entitlement("token").await.is_err());
        server.await.unwrap();

        let (transport, server) = license_http_response(200, "{").await;
        assert!(transport.entitlement("token").await.is_err());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn http_transport_sends_installation_metadata_and_bearer_credentials() {
        let (transport, server) = license_http_response(
            200,
            r#"{"device_code":"d","user_code":"BCDF-GHJK","verification_uri":"http://127.0.0.1/devices/authorize","verification_uri_complete":"http://127.0.0.1/devices/authorize?user_code=BCDF-GHJK","expires_in":600,"interval":5}"#,
        )
        .await;
        let settings = LicensingSettings {
            display_name: "Work laptop".into(),
            ..Default::default()
        };
        transport.start_authorization(&settings).await.unwrap();
        let request = server.await.unwrap();
        assert!(request.starts_with("POST /desktop/v2/device/authorize "));
        assert_eq!(
            request.split("\r\n\r\n").nth(1).unwrap(),
            format!(
                "installation_id={}&display_name=Work+laptop&platform={}&architecture={}&styrhous_version={}",
                settings.installation_id,
                std::env::consts::OS,
                std::env::consts::ARCH,
                env!("CARGO_PKG_VERSION"),
            )
        );

        let (transport, server) = license_http_response(204, "").await;
        transport.sign_out("installation-token").await.unwrap();
        let request = server.await.unwrap();
        assert!(request.starts_with("POST /desktop/v2/sign-out "));
        assert!(
            request
                .lines()
                .any(|line| line.eq_ignore_ascii_case("authorization: Bearer installation-token"))
        );
    }

    #[tokio::test]
    async fn device_sign_in_without_entitlement_stays_signed_in_for_evaluation() {
        let golden = golden();
        let harness = Harness::new(
            &golden,
            FakeTransport::approving_with(&golden, EntitlementOutcome::Ineligible("trial_expired")),
            FakeCredentialStore::default(),
        );
        let mut settings = settings_for(&golden, false);

        let state = sign_in_with(&mut settings, &mut None, &harness.runtime())
            .await
            .unwrap();

        assert_eq!(state, SessionState::Evaluation);
        assert_eq!(
            harness.credentials.token.lock().unwrap().as_deref(),
            Some("installation-token")
        );
        assert!(matches!(
            harness.next_event(),
            LicenseEvent::AwaitingApproval(_)
        ));
        assert!(matches!(
            harness.next_event(),
            LicenseEvent::Evaluation { reason_code } if reason_code == "trial_expired"
        ));
        harness.assert_no_more_events();
    }

    #[tokio::test]
    async fn device_sign_in_with_a_revoked_token_signs_out_and_forgets_it() {
        let golden = golden();
        let harness = Harness::new(
            &golden,
            FakeTransport::approving_with(&golden, EntitlementOutcome::Unauthorized),
            FakeCredentialStore::default(),
        );
        let mut session_token = None;

        let state = sign_in_with(
            &mut settings_for(&golden, false),
            &mut session_token,
            &harness.runtime(),
        )
        .await
        .unwrap();

        assert_eq!(state, SessionState::SignedOut);
        assert!(session_token.is_none());
        assert!(*harness.credentials.deleted.lock().unwrap());
        assert!(harness.credentials.token.lock().unwrap().is_none());
        assert!(matches!(
            harness.next_event(),
            LicenseEvent::AwaitingApproval(_)
        ));
        assert!(matches!(
            harness.next_event(),
            LicenseEvent::SignedOut {
                suppress_auto_refresh: false
            }
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn device_sign_in_retries_transient_poll_failures_until_approval() {
        let golden = golden();
        let transport = FakeTransport::approving(&golden);
        transport
            .poll_results
            .lock()
            .unwrap()
            .insert(0, Err(anyhow!("connection reset")));
        let harness = Harness::new(&golden, transport, FakeCredentialStore::default());

        let state = sign_in_with(
            &mut settings_for(&golden, false),
            &mut None,
            &harness.runtime(),
        )
        .await
        .unwrap();

        assert_eq!(state, SessionState::Licensed);
        assert_eq!(harness.transport.polls(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn device_sign_in_gives_up_when_the_authorization_expires() {
        let golden = golden();
        let transport = FakeTransport::approving(&golden);
        // Every poll fails; the loop must stop at the authorization's expiry, not spin forever.
        transport.poll_results.lock().unwrap().clear();
        let harness = Harness::new(&golden, transport, FakeCredentialStore::default());

        let error = sign_in_with(
            &mut settings_for(&golden, false),
            &mut None,
            &harness.runtime(),
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("expired"));
        assert_eq!(harness.transport.polls(), 60);
    }

    #[tokio::test(start_paused = true)]
    async fn device_sign_in_rejects_an_already_expired_authorization_without_polling() {
        let golden = golden();
        let mut transport = FakeTransport::approving(&golden);
        transport.authorization.as_mut().unwrap().expires_in = 0;
        let harness = Harness::new(&golden, transport, FakeCredentialStore::default());

        let error = sign_in_with(
            &mut settings_for(&golden, false),
            &mut None,
            &harness.runtime(),
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("expired"));
        assert_eq!(harness.transport.polls(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn device_sign_in_slows_down_when_asked() {
        let golden = golden();
        let transport = FakeTransport::approving(&golden);
        transport
            .poll_results
            .lock()
            .unwrap()
            .insert(0, Ok(TokenExchange::Pending("slow_down".into())));
        let harness = Harness::new(&golden, transport, FakeCredentialStore::default());
        let started = tokio::time::Instant::now();

        sign_in_with(
            &mut settings_for(&golden, false),
            &mut None,
            &harness.runtime(),
        )
        .await
        .unwrap();

        // One 1s interval, then the interval grows by 5s per RFC 8628.
        assert_eq!(started.elapsed(), Duration::from_secs(1 + 6));
        assert_eq!(harness.transport.polls(), 2);
    }

    #[tokio::test]
    async fn a_lease_for_another_installation_is_rejected_and_forgotten() {
        let golden = golden();
        let harness = Harness::new(
            &golden,
            FakeTransport::new(&golden, EntitlementOutcome::Issued),
            FakeCredentialStore::holding("stored-token"),
        );
        let mut settings = LicensingSettings {
            installation_id: Uuid::now_v7(),
            ..settings_for(&golden, true)
        };

        let result = refresh_with(&mut settings, &mut None, &harness.runtime()).await;

        assert!(result.is_err());
        assert!(settings.lease.is_none());
        assert!(settings.keys.is_none());
        assert!(matches!(
            harness.next_event(),
            LicenseEvent::ClearCachedLease
        ));
        harness.assert_no_more_events();
    }

    #[test]
    fn leases_verify_against_any_listed_key_with_a_matching_identifier() {
        let golden = golden();
        let origin = Url::parse(&golden.issuer).unwrap();
        let current = golden.keys.keys[0].clone();
        let rotated = JsonWebKey {
            kid: "rotated-key".into(),
            ..current.clone()
        };
        let verify = |keys: Vec<JsonWebKey>| {
            validate_lease_at(
                &golden.lease,
                &JsonWebKeySet { keys },
                &origin,
                golden.installation_id,
                golden.expected.issued_at,
            )
        };

        assert!(verify(vec![rotated.clone(), current]).is_ok());
        let error = verify(vec![rotated]).unwrap_err();
        assert!(error.to_string().contains("not trusted"));
    }

    #[tokio::test]
    async fn signing_out_revokes_the_installation_and_forgets_its_credential() {
        let golden = golden();
        for (session_token, sign_out_fails, delete_fails) in [
            (None, false, false),
            (Some("memory-token"), false, false),
            (None, true, false),
            (None, false, true),
        ] {
            let harness = Harness::new(
                &golden,
                FakeTransport {
                    sign_out_fails,
                    ..FakeTransport::new(&golden, EntitlementOutcome::Issued)
                },
                FakeCredentialStore {
                    delete_fails,
                    ..FakeCredentialStore::holding("stored-token")
                },
            );

            revoke_and_delete_session_with(session_token.map(String::from), &harness.runtime())
                .await;

            assert_eq!(
                *harness.transport.signed_out_tokens.lock().unwrap(),
                vec![session_token.unwrap_or("stored-token")]
            );
            assert_eq!(*harness.credentials.deleted.lock().unwrap(), !delete_fails);
            if delete_fails {
                assert!(matches!(
                    harness.next_event(),
                    LicenseEvent::KeychainUnavailable
                ));
            }
            harness.assert_no_more_events();
        }
    }

    #[test]
    fn service_applies_worker_events_to_its_status() {
        let golden = golden();
        let lease = test_lease(golden.installation_id);
        let evaluation = LicenseStatus::Evaluation {
            reason_code: "trial_expired".into(),
        };
        let failed = LicenseStatus::Failed {
            message: "offline".into(),
        };
        let cases: Vec<(LicenseStatus, LicenseEvent, LicenseStatus)> = vec![
            // A check in flight keeps signed-in states and their controls.
            (
                LicenseStatus::Licensed(lease.clone()),
                LicenseEvent::Checking,
                LicenseStatus::Licensed(lease.clone()),
            ),
            (
                LicenseStatus::Offline(lease.clone()),
                LicenseEvent::Checking,
                LicenseStatus::Offline(lease.clone()),
            ),
            (
                evaluation.clone(),
                LicenseEvent::Checking,
                evaluation.clone(),
            ),
            (
                LicenseStatus::SignedOut,
                LicenseEvent::Checking,
                LicenseStatus::Checking,
            ),
            (
                failed.clone(),
                LicenseEvent::Checking,
                LicenseStatus::Checking,
            ),
            (
                LicenseStatus::Licensed(lease.clone()),
                LicenseEvent::Evaluation {
                    reason_code: "trial_expired".into(),
                },
                evaluation.clone(),
            ),
            (
                LicenseStatus::Checking,
                LicenseEvent::Failed("offline".into()),
                failed.clone(),
            ),
            (
                LicenseStatus::Licensed(lease.clone()),
                LicenseEvent::SignedOut {
                    suppress_auto_refresh: false,
                },
                LicenseStatus::SignedOut,
            ),
        ];
        for (initial, event, expected) in cases {
            let description = format!("{initial:?} + {event:?}");
            // Evaluation and sign-out forget the cached lease; other events keep it.
            let forgets_lease = matches!(
                event,
                LicenseEvent::Evaluation { .. } | LicenseEvent::SignedOut { .. }
            );
            let (events, received) = std::sync::mpsc::channel();
            let mut service = LicensingService {
                settings: settings_for(&golden, true),
                status: initial,
                results: Some(received),
                ..Default::default()
            };
            events
                .send(LicenseEventMessage {
                    operation_generation: 0,
                    event,
                })
                .unwrap();
            service.poll_events();
            assert_eq!(service.status, expected, "{description}");
            assert_eq!(
                service.settings.lease.is_none(),
                forgets_lease,
                "{description}"
            );
            assert!(!service.settings.signed_out, "{description}");
        }

        let (events, received) = std::sync::mpsc::channel();
        let mut service = LicensingService {
            settings: LicensingSettings {
                server: LicenseServer::Custom("not a server".into()),
                ..Default::default()
            },
            results: Some(received),
            ..Default::default()
        };
        for event in [
            LicenseEvent::KeychainUnavailable,
            LicenseEvent::SignedOut {
                suppress_auto_refresh: true,
            },
        ] {
            events
                .send(LicenseEventMessage {
                    operation_generation: 0,
                    event,
                })
                .unwrap();
        }
        service.poll_events();
        assert!(service.keychain_warning());
        assert!(service.settings.signed_out);
        assert_eq!(service.status, LicenseStatus::Unconfigured);

        drop(events);
        service.poll_events();
        assert!(matches!(
            &service.status,
            LicenseStatus::Failed { message } if message.contains("background service stopped")
        ));
    }
}
