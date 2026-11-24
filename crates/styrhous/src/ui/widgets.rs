use crate::minimal_resource::format_age;
use crate::resource_table::{
    CONTAINERS_COLUMN, CPU_COLUMN, CellValue, ContainerIndicator, MEMORY_COLUMN, PODS_COLUMN,
    ROLES_COLUMN,
};
use components::design::theme::ThemeExt;
use components::design::{space, type_scale};
use components::icons::Icon;
use components::tooltip::TooltipContent;
use components::{Button, EmptyState, ProgressChecklist, TableRowBuilder};

const STATE_MARGIN: i8 = space::S32 as i8;

/// Why a CPU or memory cell has no value (S08).
pub(super) const METRICS_UNAVAILABLE: &str = "Metrics server not available";
/// Why a Node's Pods cell has no value.
const NODE_POD_COUNT_UNAVAILABLE: &str = "Pods across all namespaces could not be listed";

pub(super) fn container_indicators(ui: &mut egui::Ui, indicators: &[ContainerIndicator]) {
    if indicators.is_empty() {
        TableRowBuilder::missing(ui, "No containers");
        return;
    }
    let labels = indicators
        .iter()
        .map(|indicator| format!("{}: {}", indicator.kind.label(), indicator.name))
        .collect::<Vec<_>>();
    let statuses = indicators
        .iter()
        .map(container_status_rows)
        .collect::<Vec<_>>();
    let rows = statuses
        .iter()
        .map(|rows| {
            rows.iter()
                .map(|(key, value)| (*key, value.as_str()))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    TableRowBuilder::containers(
        ui,
        indicators
            .iter()
            .zip(&labels)
            .zip(&rows)
            .map(|((indicator, label), rows)| {
                (
                    indicator.tone,
                    label.as_str(),
                    TooltipContent::KeyValues(rows),
                )
            }),
    );
}

/// Render a type-specific resource-table value using the shared table visual
/// vocabulary. Inspector tables use this too, so a Pod or Job looks the same
/// wherever it is listed.
pub(super) fn show_resource_cell(ui: &mut egui::Ui, column_id: &str, cell: Option<&CellValue>) {
    match cell.unwrap_or(&CellValue::Empty) {
        CellValue::Text(value) => TableRowBuilder::text(ui, value, false),
        CellValue::Number(value) => TableRowBuilder::number(ui, &value.to_string()),
        CellValue::Usage {
            label, permille, ..
        } => TableRowBuilder::usage(
            ui,
            label,
            permille.map(|permille| f32::from(permille) / 1000.0),
        ),
        CellValue::Timestamp(value) => TableRowBuilder::number(
            ui,
            &format_age(Some(*value), crate::ui::clock::now(ui.ctx())),
        ),
        CellValue::Status { label, tone } => {
            TableRowBuilder::problem_status(ui, label, *tone);
        }
        CellValue::ContainerIndicators(indicators) => container_indicators(ui, indicators),
        CellValue::List(values) if values.is_empty() => {
            TableRowBuilder::missing(ui, empty_reason(column_id))
        }
        CellValue::List(values) => {
            // A Node's roles read as detail beside its name.
            let theme = ui.tokens();
            let color = if column_id == ROLES_COLUMN {
                theme.text.secondary
            } else {
                theme.text.primary
            };
            TableRowBuilder::text_in(ui, type_scale::UI, &values.join(", "), color)
        }
        CellValue::Unavailable => TableRowBuilder::missing(ui, unavailable_reason(column_id)),
        CellValue::Empty => TableRowBuilder::missing(ui, empty_reason(column_id)),
    }
}

/// Why the column `column_id` could not be read.
fn unavailable_reason(column_id: &str) -> &'static str {
    match column_id {
        PODS_COLUMN => NODE_POD_COUNT_UNAVAILABLE,
        _ => METRICS_UNAVAILABLE,
    }
}

/// Why the column `column_id` has no value.
fn empty_reason(column_id: &str) -> &'static str {
    match column_id {
        CPU_COLUMN | MEMORY_COLUMN => "No metrics reported for this resource",
        CONTAINERS_COLUMN => "No container statuses reported yet",
        PODS_COLUMN => "Counting the Pods on this Node",
        ROLES_COLUMN => "No roles assigned",
        _ => "Not set",
    }
}

/// A container's status as tooltip rows: its kind and name, state (with
/// the reason), readiness, restarts and message.
fn container_status_rows(indicator: &ContainerIndicator) -> Vec<(&'static str, String)> {
    let state = match &indicator.reason {
        Some(reason) => format!("{} · {reason}", indicator.state),
        None => indicator.state.clone(),
    };
    let mut rows = vec![
        (indicator.kind.label(), indicator.name.clone()),
        ("State", state),
        (
            "Ready",
            if indicator.ready { "Yes" } else { "No" }.to_owned(),
        ),
        ("Restarts", indicator.restart_count.to_string()),
    ];
    if let Some(message) = &indicator.message {
        rows.push(("Message", message.clone()));
    }
    rows
}

/// What is missing and what to do next, led by a neutral `icon` tile.
pub(super) fn workspace_empty_state(ui: &mut egui::Ui, icon: Icon, title: &str, message: &str) {
    let _ = EmptyState::new(icon, title).description(message).show(ui);
}

/// A failure that recovers by itself (a watcher that keeps retrying): a danger
/// tile and the server's message, with nothing to retry.
pub(super) fn workspace_unavailable_state(ui: &mut egui::Ui, title: &str, message: &str) {
    let _ = EmptyState::error(title).description(message).show(ui);
}

/// Everything is hidden by the search: says so and offers Clear filter.
/// Returns whether it was clicked.
pub(super) fn workspace_filtered_empty_state(
    ui: &mut egui::Ui,
    title: &str,
    message: &str,
) -> bool {
    EmptyState::filtered(title)
        .description(message)
        .action("Clear filter")
        .show(ui)
        .is_some_and(|response| response.clicked())
}

/// The search is not a valid regular expression; the parser's message keeps
/// its monospaced caret under the offending character.
pub(super) fn workspace_search_error_state(ui: &mut egui::Ui, message: &str) {
    workspace_error_details(ui, "Invalid regular expression", message, 0.0, |_| ());
}

pub(super) fn workspace_loading_state(ui: &mut egui::Ui, title: &str, message: &str) {
    let _ = EmptyState::loading(title).description(message).show(ui);
}

/// A failure the user can retry: its title, the technical message in a code
/// block and Retry, laid out like a failed connection. Returns whether Retry
/// was clicked.
pub(super) fn workspace_error_state(ui: &mut egui::Ui, title: &str, message: &str) -> bool {
    workspace_error_details(ui, title, message, retry_footer_height(ui), retry_footer)
}

/// `title` over "Error details" and `message` in a code block, then
/// `show_footer`, in a column as wide as a failed connection's.
fn workspace_error_details<R>(
    ui: &mut egui::Ui,
    title: &str,
    message: &str,
    footer_height: f32,
    show_footer: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let width = ui.available_width().min(FAILED_CONNECTION_COLUMN_WIDTH);
    let estimate = type_scale::TITLE.line_height()
        + space::S16
        + error_details_height(ui, message, width)
        + footer_height;
    show_measured_state_area(ui, "error-details", estimate, |ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(width, 0.0),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing.y = 0.0;
                type_scale::TITLE.label(ui, title, ui.tokens().text.primary);
                ui.add_space(space::S16);
                show_error_details(ui, message);
                show_footer(ui)
            },
        )
        .inner
    })
}

/// The column of a failed connection: its steps, the error and the retry share it.
const FAILED_CONNECTION_COLUMN_WIDTH: f32 = 600.0;

/// The progress of connecting to a cluster, and why it failed with a way to retry.
/// Returns whether retry was clicked.
pub(super) fn workspace_connection_progress(
    ui: &mut egui::Ui,
    checklist: ProgressChecklist<'_>,
    error: Option<&str>,
) -> bool {
    let width = ui.available_width().min(FAILED_CONNECTION_COLUMN_WIDTH);
    let error_height = error.map_or(0.0, |message| {
        space::S24 + error_details_height(ui, message, width) + retry_footer_height(ui)
    });
    let estimate = checklist.height(ui) + error_height;
    show_measured_state_area(ui, "connection-progress", estimate, |ui| {
        let Some(message) = error else {
            checklist.show(ui);
            return false;
        };
        let width = ui.available_width().min(FAILED_CONNECTION_COLUMN_WIDTH);
        checklist.width(width).show(ui);
        ui.add_space(space::S24);
        ui.allocate_ui_with_layout(
            egui::vec2(width, 0.0),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                ui.set_width(width);
                show_error_details(ui, message);
                retry_footer(ui)
            },
        )
        .inner
    })
}

/// The Retry button's height.
const RETRY_BUTTON_HEIGHT: f32 = components::ButtonSize::Md.height();

fn retry_footer_height(ui: &egui::Ui) -> f32 {
    space::S12 + ui.spacing().item_spacing.y + RETRY_BUTTON_HEIGHT
}

fn retry_footer(ui: &mut egui::Ui) -> bool {
    ui.add_space(space::S12);
    ui.vertical_centered(|ui| Button::primary("Retry").show(ui).clicked())
        .inner
}

/// Padding of a code block ([`components::CodeBlock`], `padding: 10px 14px`).
const CODE_BLOCK_PADDING: egui::Vec2 = egui::vec2(14.0, 10.0);

/// The height `show_error_details` needs for `message` at `width`.
fn error_details_height(ui: &egui::Ui, message: &str, width: f32) -> f32 {
    let code = type_scale::MONO_SMALL
        .galley(
            ui.ctx(),
            message,
            ui.tokens().code.text,
            width - 2.0 * space::S12,
        )
        .rect
        .height();
    type_scale::BODY.line_height() + space::S4 + code + 2.0 * CODE_BLOCK_PADDING.y
}

/// "Error details" above the message in a wrapping, selectable code block.
fn show_error_details(ui: &mut egui::Ui, message: &str) {
    let theme = ui.tokens();
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        type_scale::BODY.label(ui, "Error details", theme.text.muted);
        ui.add_space(space::S4);
        let _ = components::CodeBlock::new(message).wrap().show(ui);
    });
}

/// `show_state_area`, centred on the content's height as measured in the previous frame,
/// so `estimate` only has to be close for the first one. `id_salt` keeps the
/// measurements of different pages apart.
fn show_measured_state_area<R>(
    ui: &mut egui::Ui,
    id_salt: &str,
    estimate: f32,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let id = ui.id().with(id_salt);
    let measured = ui.ctx().data(|data| data.get_temp::<f32>(id));
    let (result, height) = show_state_area(ui, measured.unwrap_or(estimate), |ui| {
        let result = add_contents(ui);
        (result, ui.min_rect().height())
    });
    if measured != Some(height) {
        ui.ctx().data_mut(|data| data.insert_temp(id, height));
        ui.ctx().request_repaint();
    }
    result
}

fn show_state_area<R>(
    ui: &mut egui::Ui,
    content_height: f32,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    egui::Frame::new()
        .inner_margin(egui::Margin::same(STATE_MARGIN))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), content_height),
                    egui::Layout::top_down(egui::Align::Center),
                    add_contents,
                )
                .inner
            })
            .inner
        })
        .inner
}

/// "1 line", "3 lines": a count with its noun, pluralised with an "s".
pub(super) fn count_noun(count: usize, noun: &str) -> String {
    format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
}

/// A captioned group of `key=value` chips (labels, annotations); clicking a
/// chip copies `key=value`. Nothing is shown for an empty group.
pub(super) fn key_value_chips<'a>(
    ui: &mut egui::Ui,
    caption: &str,
    entries: impl IntoIterator<Item = (&'a String, &'a String)>,
) {
    let mut entries = entries.into_iter().peekable();
    if entries.peek().is_none() {
        return;
    }
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = space::S6;
        type_scale::CAPTION.label(ui, caption, ui.tokens().text.muted);
        components::chip_flow(ui, |ui| {
            for (key, value) in entries {
                let key = format!("{key}=");
                let copy = format!("{key}{value}");
                components::Chip::new(value)
                    .key(&key)
                    .copyable(caption)
                    .copy_as(&copy)
                    .show(ui);
            }
        });
    });
}

/// A resource's age in a right-aligned Age cell, or an em dash when
/// Kubernetes reported no creation time.
pub(super) fn age_cell(ui: &mut egui::Ui, creation_timestamp: Option<time::OffsetDateTime>) {
    match creation_timestamp {
        Some(_) => TableRowBuilder::number(
            ui,
            &format_age(creation_timestamp, crate::ui::clock::now(ui.ctx())),
        ),
        None => TableRowBuilder::missing(ui, "No creation time reported"),
    }
}

/// A resource's namespace, or an em dash for a cluster-scoped resource.
pub(super) fn namespace_cell(ui: &mut egui::Ui, namespace: Option<&str>) {
    TableRowBuilder::text_or_missing(ui, namespace, "Not namespaced");
}
