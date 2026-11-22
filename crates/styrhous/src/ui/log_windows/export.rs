//! Exporting a log window's spooled records to a file (GitHub issue #90).

use super::*;
use crate::log_store::{LogExportFilter, LogExportFormat, LogExportOptions, LogStoreResult};
use crate::ui::save_file_dialog::{SaveFileAnswer, SaveFileChoice, SaveFileRequest};
use crate::ui::state::OperationOutcome;
use crate::ui::widgets::count_noun;
use components::{ButtonSize, TailwindButton, checkbox_field, menu_frame};
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

fn status_text(status: &LogExportStatus) -> Option<(String, egui::Color32)> {
    match status {
        LogExportStatus::ChoosingFile { .. } => None,
        LogExportStatus::Writing { path, .. } => {
            Some((format!("Exporting to {}…", file_name(path)), gray::_600))
        }
        LogExportStatus::Done { path, lines } => Some((
            format!(
                "Exported {} to {}",
                count_noun(*lines, "line"),
                file_name(path)
            ),
            gray::_600,
        )),
        LogExportStatus::Failed(error) => Some((format!("Export failed: {error}"), status::DANGER)),
    }
}

/// The "Export…" button, its options popover and the latest export's status,
/// laid out right to left at the end of the header's second row.
pub(super) fn show_export_controls(
    ui: &mut egui::Ui,
    window: &mut PodLogWindowState,
    display_options: &LogDisplayOptions,
) {
    // Asking again while a dialog is open is allowed (a dialog that never
    // returns must not lock exporting); only one file is written at a time.
    let writing = window.export.writing_export_id().is_some();
    // The same control style as the "Current"/"Previous" buttons in this row.
    let trigger = ui.add_enabled(!writing, egui::Button::new("Export…"));
    trigger.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, !writing, "Export logs")
    });
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
        .gap(spacing::XS)
        .width(POPOVER_WIDTH)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(menu_frame().inner_margin(egui::Margin::same(spacing::LG as i8)))
        .show(|ui| {
            ui.label(
                egui::RichText::new("Export options")
                    .font(typography::semibold(typography::BODY_SIZE))
                    .color(gray::_900),
            );
            ui.add_space(spacing::XS);
            ui.label(
                egui::RichText::new("Saves the loaded lines from disk.")
                    .font(typography::metadata())
                    .color(gray::_500),
            );
            ui.add_space(spacing::MD);
            let filter_available = window.export_filter().is_some();
            let choices = &mut window.export.choices;
            ui.add_enabled_ui(filter_available, |ui| {
                checkbox_field(
                    ui,
                    &mut choices.only_matching,
                    "Only lines matching the filter",
                );
            });
            let format = &mut choices.format;
            checkbox_field(ui, &mut format.include_sources, "Include source labels");
            checkbox_field(ui, &mut format.include_timestamps, "Include timestamps");
            checkbox_field(ui, &mut format.keep_ansi, "Keep ANSI colour codes");
            ui.add_space(spacing::MD);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                save = TailwindButton::primary("Save…")
                    .size(ButtonSize::Xs)
                    .show(ui)
                    .clicked();
            });
        });
    if save {
        egui::Popup::close_id(ui.ctx(), popup_id);
        window.start_export();
    }
    if let Some((text, color)) = window.export.status.as_ref().and_then(status_text) {
        ui.add_space(spacing::SM);
        ui.add(
            egui::Label::new(
                egui::RichText::new(text)
                    .font(typography::body())
                    .color(color),
            )
            .truncate(),
        );
    }
}
