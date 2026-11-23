//! The contents of table cells (`.td`): text, numbers, missing values,
//! status, container squares and navigable text.

use egui::{Button, Color32, Ui, WidgetText};

use crate::PointingHand;
use crate::design::size;
use crate::design::theme::{ThemeExt, Tone};
use crate::design::type_scale::{self, TextRole};
use crate::tooltip::{TooltipContent, TooltipExt, elided_label};

/// The em dash of a missing value.
const MISSING: &str = "—";
// C17
/// A row's name (`.tbl .nm`): UI 13/20 at 600.
const NAME_ROLE: TextRole = type_scale::UI_STRONG;

/// The text colour of a cell in a row whose problem is `tone`.
fn problem_color(ui: &Ui, tone: Option<Tone>) -> Color32 {
    let theme = ui.tokens();
    match tone {
        Some(Tone::Warning) => theme.warning.text,
        Some(Tone::Danger) => theme.danger.text,
        _ => theme.text.primary,
    }
}

/// Builder passed to the row render closure for rendering cells
pub struct TableRowBuilder<'a> {
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl TableRowBuilder<'_> {
    /// Render text in a cell, elided (with the full text in a tooltip) when
    /// it does not fit.
    ///
    /// The first column (the name) is set in weight 500.
    pub fn text(ui: &mut Ui, text: &str, is_first_column: bool) {
        let role = if is_first_column {
            type_scale::UI_CONTROL
        } else {
            type_scale::UI
        };
        Self::text_in(ui, role, text, ui.tokens().text.primary);
    }

    /// Render `text` in `role` and `color`, elided when it does not fit.
    pub fn text_in(ui: &mut Ui, role: TextRole, text: &str, color: Color32) {
        elided_label(ui, role, text, color);
    }

    /// Render a number, for a [`super::TableColumnBuilder::numeric`]
    /// column: in tabular figures when the column is
    /// [`super::TableColumnBuilder::tabular`], otherwise proportional.
    pub fn number(ui: &mut Ui, text: &str) {
        let role = if super::layout::in_tabular_cell(ui) {
            type_scale::UI.tabular()
        } else {
            type_scale::UI
        };
        Self::text_in(ui, role, text, ui.tokens().text.primary);
    }

    // C17
    /// Render a row's name in weight 600, so the eye lands on it first
    /// (`.tbl .nm`, S17).
    pub fn name(ui: &mut Ui, text: &str) {
        Self::text_in(ui, NAME_ROLE, text, ui.tokens().text.primary);
    }

    /// Render a status cell coloured by its problem (S17): the words in
    /// `warning-text` or `danger-text` for a warning or danger `tone`,
    /// otherwise `text-primary`; the lamp always carries the tone.
    pub fn problem_status(ui: &mut Ui, text: &str, tone: Tone) -> egui::Response {
        let color = problem_color(ui, Some(tone));
        crate::StatusLabel::new(text, tone).color(color).show(ui)
    }

    /// Render a number coloured by the row's problem `tone` (a restart
    /// count: `warning-text` or `danger-text` while there is one).
    pub fn problem_number(ui: &mut Ui, text: &str, tone: Option<Tone>) {
        let color = problem_color(ui, tone);
        let role = if super::layout::in_tabular_cell(ui) {
            type_scale::UI.tabular()
        } else {
            type_scale::UI
        };
        Self::text_in(ui, role, text, color);
    }

    /// Render monospaced text (image references, hashes).
    pub fn mono(ui: &mut Ui, text: &str) {
        Self::text_in(ui, type_scale::MONO, text, ui.tokens().text.primary);
    }

    /// Render a missing value: an em dash in `text-disabled` whose tooltip
    /// says why it is missing ("Metrics server not available"). Never a
    /// truncated word like "Unavail…".
    pub fn missing(ui: &mut Ui, reason: &str) {
        let color = ui.tokens().text.disabled;
        let galley = type_scale::UI.galley(ui.ctx(), MISSING, color, f32::INFINITY);
        let (rect, response) = type_scale::allocate_text(ui, galley.rect.size(), reason);
        type_scale::paint_text(ui, &response, rect.min, galley, color);
        response.tooltip(reason);
    }

    /// Render `value` as [`Self::text`], or as [`Self::missing`] explained by
    /// `reason` when there is none or it is empty.
    pub fn text_or_missing(ui: &mut Ui, value: Option<&str>, reason: &str) {
        match value.filter(|value| !value.is_empty()) {
            Some(value) => Self::text(ui, value, false),
            None => Self::missing(ui, reason),
        }
    }

    /// Render a status cell: the status words with their lamp (`.st`).
    pub fn status(ui: &mut Ui, text: &str, tone: Tone) -> egui::Response {
        crate::StatusLabel::new(text, tone).show(ui)
    }

    /// Render one container state square per `(tone, label, tooltip)`
    /// (`.cs`), 3 points apart, each read as its label and described by its
    /// tooltip (a container's status as key/value rows).
    pub fn containers<'s>(
        ui: &mut Ui,
        containers: impl IntoIterator<Item = (Tone, &'s str, TooltipContent<'s>)>,
    ) {
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = size::CONTAINER_SQUARE_GAP;
            for (tone, label, tooltip) in containers {
                crate::container_square(ui, tone, label).tooltip_content(tooltip);
            }
        });
    }

    /// Render a text-styled button for a navigable table cell.
    ///
    /// The button deliberately has no chrome so it retains the table's visual
    /// language, while still letting egui route pointer and keyboard input to
    /// the cell that owns it.
    pub fn clickable_text(
        ui: &mut Ui,
        text: &str,
        color: Color32,
        accessibility_label: impl Into<WidgetText>,
    ) -> egui::Response {
        Self::clickable_in(ui, type_scale::UI, text, color, accessibility_label)
    }

    // C17
    /// Render a row's name as [`Self::name`], opening something when
    /// clicked as [`Self::clickable_text`] does.
    pub fn clickable_name(
        ui: &mut Ui,
        text: &str,
        accessibility_label: impl Into<WidgetText>,
    ) -> egui::Response {
        let color = ui.tokens().text.primary;
        Self::clickable_in(ui, NAME_ROLE, text, color, accessibility_label)
    }

    fn clickable_in(
        ui: &mut Ui,
        role: TextRole,
        text: &str,
        color: Color32,
        accessibility_label: impl Into<WidgetText>,
    ) -> egui::Response {
        let accessibility_label = accessibility_label.into();
        let response = ui
            .add(
                Button::new(role.rich_text(ui.ctx(), text).color(color))
                    .small()
                    .frame(false)
                    .truncate(),
            )
            .with_pointing_hand();
        let is_enabled = ui.is_enabled();
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                is_enabled,
                accessibility_label.text(),
            )
        });
        response
    }
}
