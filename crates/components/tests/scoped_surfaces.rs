//! The same controls on the canvas and inside every re-pointing scope, so a
//! snapshot shows the design's context rules (board C00b, layer 3) at work:
//! rows step up to `surface-raised` in cards, blades and dialogs; fields and
//! wells step down to `surface-panel` in popovers, floating toolbars and
//! cards nested in overlays; fields step up in the sidebar.

use components::design::theme::{self, ThemeExt, ThemeScope};
use components::design::{radius, space, type_scale};
use components::test_support::UiHarnessSnapshot;
use components::{
    TableRowBuilder, TailwindCombobox, TailwindTable, TailwindTextInput, WorkspaceCard,
    overlay_frame, popover_frame, raised_frame,
};
use egui::{Frame, Margin, Rect, Ui, UiBuilder, pos2, vec2};
use egui_kittest::{Harness, kittest::Queryable};

const COLUMN_WIDTH: f32 = 176.0;
const COLUMN_GAP: f32 = 12.0;
const TOP: f32 = 24.0;

/// Where each column's controls sit.
#[derive(Clone, Copy)]
enum Context {
    Canvas,
    Card,
    Blade,
    BladeCard,
    Dialog,
    Popover,
    Sidebar,
    FloatingToolbar,
}

impl Context {
    const ALL: [Self; 8] = [
        Self::Canvas,
        Self::Card,
        Self::Blade,
        Self::BladeCard,
        Self::Dialog,
        Self::Popover,
        Self::Sidebar,
        Self::FloatingToolbar,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Canvas => "Canvas",
            Self::Card => "Card",
            Self::Blade => "Blade",
            Self::BladeCard => "Blade card",
            Self::Dialog => "Dialog",
            Self::Popover => "Popover",
            Self::Sidebar => "Sidebar",
            Self::FloatingToolbar => "Floating toolbar",
        }
    }
}

/// The controls every column shows, in its scope.
fn controls(ui: &mut Ui, context: Context, value: &mut String) {
    let name = context.label();
    let theme = ui.tokens();
    ui.spacing_mut().item_spacing.y = space::S12;
    Frame::new()
        .fill(theme.components.well_bg)
        .corner_radius(radius::FULL)
        .inner_margin(Margin::symmetric(9, 3))
        .show(ui, |ui| {
            ui.label(type_scale::META.text(ui.ctx(), "well chip", theme.text.secondary));
        });
    TailwindTextInput::new(value)
        .id_salt(("field", name))
        .accessibility_label(format!("{name} field"))
        .show(ui);
    TailwindTable::new(("rows", name))
        .column("name", "Name", |column| {
            column.initial_width(COLUMN_WIDTH - 96.0)
        })
        .show(ui, &["row"], |ui, row, _| {
            TableRowBuilder::text(ui, row, true);
        });
    WorkspaceCard::new().show(ui, |ui| {
        ui.label(type_scale::META.text(ui.ctx(), "nested card", theme.text.secondary));
    });
    TailwindCombobox::from_label(format!("{name} select"))
        .placeholder("Select…")
        .width(ui.available_width())
        .filter_by(|item: &&str| *item)
        .show_items(ui, &["kind", "prod"], |combobox, item| {
            combobox.item(*item, false);
        });
}

fn column(ui: &mut Ui, context: Context, value: &mut String) {
    let theme = ui.tokens();
    let inset = Margin::same(space::S12 as i8);
    ui.label(type_scale::CAPTION.text(ui.ctx(), context.label(), theme.text.muted));
    ui.add_space(space::S8);
    let mut show = |ui: &mut Ui| controls(ui, context, value);
    match context {
        Context::Canvas => {
            show(ui);
        }
        Context::Card => {
            WorkspaceCard::new().margin(inset).show(ui, show);
        }
        Context::Blade => {
            overlay_frame(&theme)
                .inner_margin(inset)
                .show(ui, |ui| theme::scope(ui, ThemeScope::Blade, show));
        }
        Context::BladeCard => {
            overlay_frame(&theme).inner_margin(inset).show(ui, |ui| {
                theme::scope(ui, ThemeScope::Blade, |ui| {
                    WorkspaceCard::new().margin(inset).show(ui, show)
                })
            });
        }
        Context::Dialog => {
            overlay_frame(&theme)
                .inner_margin(inset)
                .show(ui, |ui| theme::scope(ui, ThemeScope::Dialog, show));
        }
        Context::Popover => {
            popover_frame(&theme)
                .inner_margin(inset)
                .show(ui, |ui| theme::scope(ui, ThemeScope::Popover, show));
        }
        Context::Sidebar => {
            Frame::new()
                .fill(theme.nav.surface)
                .corner_radius(radius::CARD)
                .inner_margin(inset)
                .show(ui, |ui| theme::scope(ui, ThemeScope::Sidebar, show));
        }
        Context::FloatingToolbar => {
            raised_frame(&theme)
                .corner_radius(radius::CARD)
                .inner_margin(inset)
                .show(ui, |ui| theme::scope(ui, ThemeScope::FloatingToolbar, show));
        }
    }
}

fn gallery(ui: &mut Ui, values: &mut [String; 8]) {
    let theme = ui.tokens();
    ui.painter()
        .rect_filled(ui.ctx().content_rect(), 0, theme.surface.canvas);
    for (index, (context, value)) in Context::ALL.into_iter().zip(values).enumerate() {
        let left = space::S24 + index as f32 * (COLUMN_WIDTH + COLUMN_GAP);
        let rect = Rect::from_min_size(pos2(left, TOP), vec2(COLUMN_WIDTH, 900.0));
        let mut column_ui = ui.new_child(
            UiBuilder::new()
                .id_salt(context.label())
                .max_rect(rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        column_ui.set_width(COLUMN_WIDTH);
        column(&mut column_ui, context, value);
    }
}

#[test]
fn scoped_surfaces_gallery() {
    let mut values: [String; 8] = std::array::from_fn(|_| "value".to_owned());
    let mut harness = Harness::new_ui(move |ui| gallery(ui, &mut values));
    components::test_support::setup_egui(&mut harness);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Blade card select")
        .click();
    harness.run();
    harness.ui_harness("scoped_surfaces/scoped_surfaces_gallery/gallery");
}
