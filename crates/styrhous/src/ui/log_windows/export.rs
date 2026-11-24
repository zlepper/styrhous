//! Exporting a log window's spooled records to a file (GitHub issue #90).

use super::*;
use crate::log_store::{LogExportFilter, LogExportFormat, LogExportOptions, LogStoreResult};
use crate::ui::save_file_dialog::{SaveFileAnswer, SaveFileChoice, SaveFileRequest};
use crate::ui::state::OperationOutcome;
use crate::ui::widgets::count_noun;
use components::frames::show_elevated;
use components::tooltip::truncated_label;
use components::{Button, ButtonSize, Checkbox, popover_frame};
use std::path::{Path, PathBuf};

const POPOVER_WIDTH: f32 = 300.0;

#[derive(Debug, Clone, Default)]
pub(in crate::ui) struct LogExportState {
    choices: LogExportChoices,
    next_export_id: u64,
    pub(in crate::ui) status: Option<LogExportStatus>,
    /// A file choice the application has yet to ask the platform for.
    pending_request: Option<SaveFileRequest>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct LogExportChoices {
    only_matching: bool,
    format: LogExportFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ui) enum LogExportStatus {
    ChoosingFile {
        export_id: u64,
        options: LogExportOptions,
    },
    Writing {
        export_id: u64,
        path: PathBuf,
    },
    Done {
        path: PathBuf,
        lines: usize,
    },
    Failed(String),
}

impl LogExportState {
    /// An export is waiting for a file choice or being written.
    pub(in crate::ui) fn is_busy(&self) -> bool {
        matches!(
            self.status,
            Some(LogExportStatus::ChoosingFile { .. } | LogExportStatus::Writing { .. })
        )
    }

    /// The export being written, so a result can still be reported after the
    /// window closes.
    pub(in crate::ui) fn writing_export_id(&self) -> Option<u64> {
        match self.status {
            Some(LogExportStatus::Writing { export_id, .. }) => Some(export_id),
            _ => None,
        }
    }
}

/// An export outcome for the notification history.
pub(in crate::ui) struct LogExportNotification {
    pub(in crate::ui) cluster_key: i32,
    pub(in crate::ui) outcome: OperationOutcome,
    pub(in crate::ui) title: &'static str,
    pub(in crate::ui) target: String,
    pub(in crate::ui) details: String,
}

impl LogExportNotification {
    fn failure(cluster_key: i32, details: String) -> Self {
        Self {
            cluster_key,
            outcome: OperationOutcome::Failure,
            title: "Couldn’t export logs",
            target: "Log export".to_owned(),
            details,
        }
    }

    /// The notification for a store result, whichever window it belonged to.
    pub(in crate::ui) fn for_result(cluster_key: i32, result: &LogStoreResult) -> Option<Self> {
        match result {
            LogStoreResult::Exported { path, lines, .. } => Some(Self {
                cluster_key,
                outcome: OperationOutcome::Success,
                title: "Logs exported",
                target: file_name(path),
                details: format!("{} saved to {}", count_noun(*lines, "line"), path.display()),
            }),
            LogStoreResult::ExportFailed { error, .. } => {
                Some(Self::failure(cluster_key, error.clone()))
            }
            _ => None,
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

impl PodLogWindowState {
    /// A file name describing what the window shows, unique per second.
    fn suggested_export_name(&self) -> String {
        let source = match self.targets.as_slice() {
            [target] => format!("{}-{}", target.pod_name, target.container),
            targets => format!("interleaved-{}-sources", targets.len()),
        };
        let previous = if self.instance == PodLogInstance::Previous {
            "-previous"
        } else {
            ""
        };
        let now = time::OffsetDateTime::now_utc();
        format!(
            "{source}{previous}-{:04}{:02}{:02}-{:02}{:02}{:02}.log",
            now.year(),
            u8::from(now.month()),
            now.day(),
            now.hour(),
            now.minute(),
            now.second()
        )
    }

    /// The filter an export can follow: active, and a valid pattern.
    fn export_filter(&self) -> Option<LogExportFilter> {
        (self.filter_is_active() && self.search.error.is_none()).then(|| LogExportFilter {
            query: self.search.query.clone(),
            regex_mode: self.search.regex_mode,
        })
    }

    fn start_export(&mut self) {
        let options = LogExportOptions {
            filter: self
                .export
                .choices
                .only_matching
                .then(|| self.export_filter())
                .flatten(),
            format: self.export.choices.format,
        };
        self.export.next_export_id += 1;
        let export_id = self.export.next_export_id;
        self.export.status = Some(LogExportStatus::ChoosingFile { export_id, options });
        self.export.pending_request = Some(SaveFileRequest {
            window_id: self.id,
            export_id,
            suggested_name: self.suggested_export_name(),
        });
    }

    /// The save dialog the application should open for this window, if any.
    pub(in crate::ui) fn take_save_request(&mut self) -> Option<SaveFileRequest> {
        self.export.pending_request.take()
    }

    /// Starts writing once the user chose a file, or forgets a cancelled export.
    /// Returns a failure to record when the export could not be started.
    pub(in crate::ui) fn answer_save_request(
        &mut self,
        answer: SaveFileAnswer,
        log_store: &LogStoreService,
    ) -> Option<LogExportNotification> {
        let (export_id, options) = match self.export.status.take() {
            Some(LogExportStatus::ChoosingFile { export_id, options })
                if export_id == answer.export_id =>
            {
                (export_id, options)
            }
            other => {
                self.export.status = other;
                return None;
            }
        };
        let failure = match answer.choice {
            SaveFileChoice::Cancelled => return None,
            SaveFileChoice::Chosen(path) => {
                if log_store.export(self.id, export_id, path.clone(), options) {
                    self.export.status = Some(LogExportStatus::Writing { export_id, path });
                    return None;
                }
                "The log store is busy; try again.".to_owned()
            }
            SaveFileChoice::Unavailable(reason) => reason,
        };
        self.export.status = Some(LogExportStatus::Failed(failure.clone()));
        Some(LogExportNotification::failure(self.cluster_key, failure))
    }

    /// Applies an export result to the export that is being written.
    pub(in crate::ui) fn settle_export(
        &mut self,
        result: &LogStoreResult,
    ) -> Option<LogExportNotification> {
        let writing = self.export.writing_export_id()?;
        let status = match result {
            LogStoreResult::Exported {
                export_id,
                path,
                lines,
                ..
            } if *export_id == writing => LogExportStatus::Done {
                path: path.clone(),
                lines: *lines,
            },
            LogStoreResult::ExportFailed {
                export_id, error, ..
            } if *export_id == writing => LogExportStatus::Failed(error.clone()),
            _ => return None,
        };
        self.export.status = Some(status);
        LogExportNotification::for_result(self.cluster_key, result)
    }
}

fn status_text(status: &LogExportStatus, theme: &Theme) -> Option<(String, egui::Color32)> {
    match status {
        LogExportStatus::ChoosingFile { .. } => None,
        LogExportStatus::Writing { path, .. } => Some((
            format!("Exporting to {}…", file_name(path)),
            theme.text.secondary,
        )),
        LogExportStatus::Done { path, lines } => Some((
            format!(
                "Exported {} to {}",
                count_noun(*lines, "line"),
                file_name(path)
            ),
            theme.text.secondary,
        )),
        LogExportStatus::Failed(error) => {
            Some((format!("Export failed: {error}"), theme.danger.text))
        }
    }
}

/// The "Export…" button, its options popover and the latest export's status,
/// at the end of the window header.
/// Widest the latest export's status grows in the header before it is
/// truncated (its full text is the tooltip).
const EXPORT_STATUS_MAX_WIDTH: f32 = 240.0;

pub(super) fn show_export_controls(
    ui: &mut egui::Ui,
    window: &mut PodLogWindowState,
    display_options: &LogDisplayOptions,
    compact: bool,
) {
    // Asking again while a dialog is open is allowed (a dialog that never
    // returns must not lock exporting); only one file is written at a time.
    let theme = ui.tokens();
    let writing = window.export.writing_export_id().is_some();
    let trigger = if compact {
        Button::icon(icons::Icon::ArrowDownTray, "Export logs")
            .variant(components::ButtonVariant::Ghost)
    } else {
        Button::ghost("Export…").accessibility_label("Export logs")
    }
    .size(ButtonSize::Sm)
    .enabled(!writing)
    .show(ui);
    let popup_id = egui::Popup::default_response_id(&trigger);
    if trigger.clicked() && !egui::Popup::is_id_open(ui.ctx(), popup_id) {
        // Start from what the window currently shows.
        window.export.choices = LogExportChoices {
            only_matching: window.export_filter().is_some(),
            format: LogExportFormat {
                include_timestamps: display_options.show_timestamps,
                include_sources: window.show_source_labels,
                keep_ansi: false,
            },
        };
    }
    let mut save = false;
    egui::Popup::menu(&trigger)
        .align(egui::RectAlign::BOTTOM_END)
        .gap(space::S4)
        .width(POPOVER_WIDTH)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(egui::Frame::NONE)
        .show(|ui| {
            let frame = popover_frame(&theme).inner_margin(egui::Margin::same(space::S16 as i8));
            show_elevated(ui, frame, theme.shadow.raised, |ui| {
                theme::scope(ui, ThemeScope::Popover, |ui| {
                    ui.label(
                        egui::RichText::new("Export options")
                            .font(type_scale::BODY_STRONG.font_id(ui.ctx()))
                            .color(theme.text.primary),
                    );
                    ui.add_space(space::S4);
                    ui.label(
                        egui::RichText::new("Saves the loaded lines from disk.")
                            .font(type_scale::META.font_id(ui.ctx()))
                            .color(theme.text.muted),
                    );
                    ui.add_space(space::S12);
                    let filter_available = window.export_filter().is_some();
                    let choices = &mut window.export.choices;
                    ui.add_enabled_ui(filter_available, |ui| {
                        Checkbox::new(&mut choices.only_matching, "Only lines matching the filter")
                            .show(ui);
                    });
                    let format = &mut choices.format;
                    Checkbox::new(&mut format.include_sources, "Include source labels").show(ui);
                    Checkbox::new(&mut format.include_timestamps, "Include timestamps").show(ui);
                    Checkbox::new(&mut format.keep_ansi, "Keep ANSI colour codes").show(ui);
                    ui.add_space(space::S12);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        save = Button::primary("Save…")
                            .size(ButtonSize::Sm)
                            .show(ui)
                            .clicked();
                    });
                })
            });
        });
    if save {
        egui::Popup::close_id(ui.ctx(), popup_id);
        window.start_export();
    }
    if let Some((text, color)) = window
        .export
        .status
        .as_ref()
        .and_then(|status| status_text(status, &theme))
    {
        ui.add_space(space::S8);
        // A long error must not crowd out the window's title and state.
        let width = ui.available_width().min(EXPORT_STATUS_MAX_WIDTH);
        ui.allocate_ui(egui::vec2(width, ui.available_height()), |ui| {
            truncated_label(
                ui,
                egui::RichText::new(text)
                    .font(type_scale::META.font_id(ui.ctx()))
                    .color(color),
                |label| label,
            );
        });
    }
}
