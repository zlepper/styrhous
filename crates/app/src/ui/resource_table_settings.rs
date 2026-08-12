use super::table_preferences::{
    PersistedResourceTablePreferences, ResourceTableKey, TableColumnDefinition,
};
use components::{BladeNavigator, BladeStack, ReorderHandle, ReorderableTable};

#[derive(Default)]
pub(super) struct ResourceTableSettingsState {
    navigator: Option<BladeNavigator<ResourceTableSettingsTarget>>,
}

#[derive(Clone)]
struct ResourceTableSettingsTarget {
    key: ResourceTableKey,
    columns: Vec<EditableColumn>,
}

#[derive(Clone)]
struct EditableColumn {
    definition: TableColumnDefinition,
    visible: bool,
}

impl ResourceTableSettingsState {
    pub(super) fn open(
        &mut self,
        preferences: &mut PersistedResourceTablePreferences,
        key: ResourceTableKey,
        definitions: &[TableColumnDefinition],
    ) {
        let columns = preferences
            .all_columns(&key, definitions)
            .into_iter()
            .map(|(definition, visible)| EditableColumn {
                definition,
                visible,
            })
            .collect();
        self.navigator = Some(BladeNavigator::new(ResourceTableSettingsTarget {
            key,
            columns,
        }));
    }
}

pub(super) fn show(
    ctx: &egui::Context,
    state: &mut ResourceTableSettingsState,
    preferences: &mut PersistedResourceTablePreferences,
) {
    let Some(navigator) = state.navigator.as_mut() else {
        return;
    };
    let stack = BladeStack::new("resource-table-settings");
    let response = stack.show(
        ctx,
        navigator,
        |ui, _, _| {
            ui.label(
                egui::RichText::new("Configure columns")
                    .font(components::design::typography::page_title())
                    .color(components::colors::gray::_900),
            );
        },
        |ui, target, _| {
            ui.label(
                egui::RichText::new(
                    "Choose which columns are visible and drag rows to change their order.",
                )
                .font(components::design::typography::body())
                .color(components::colors::gray::_600),
            );
            ui.add_space(components::design::spacing::XL);
            let width = ui.available_width();
            let moved = ReorderableTable::new(("resource-table-settings-rows", &target.key), 44.0)
                .show(
                    ui,
                    &mut target.columns,
                    width,
                    |ui, column, _index, handle| show_column_row(ui, column, handle),
                    show_column_preview,
                );
            let definitions = target
                .columns
                .iter()
                .map(|column| column.definition.clone())
                .collect::<Vec<_>>();
            for column in &target.columns {
                preferences.set_visible(
                    &target.key,
                    &definitions,
                    &column.definition.id,
                    column.visible,
                );
            }
            let persisted_columns = preferences.all_columns(&target.key, &definitions);
            for column in &mut target.columns {
                if let Some((_, visible)) = persisted_columns
                    .iter()
                    .find(|(definition, _)| definition.id == column.definition.id)
                {
                    column.visible = *visible;
                }
            }
            if moved {
                preferences.set_order(
                    &target.key,
                    &definitions,
                    &target
                        .columns
                        .iter()
                        .map(|column| column.definition.id.clone())
                        .collect::<Vec<_>>(),
                );
            }
        },
    );
    if response.dismissed || response.close_finished {
        state.navigator = None;
    }
}

fn show_column_row(ui: &mut egui::Ui, column: &mut EditableColumn, handle: &ReorderHandle) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, components::colors::WHITE);
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(1.0, components::colors::TABLE_BORDER),
    );
    let mut row = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    row.add_space(components::design::spacing::LG);
    let (handle_rect, _) = row.allocate_exact_size(egui::Vec2::splat(18.0), egui::Sense::hover());
    let response = row.interact(
        handle_rect,
        row.id().with(("reorder", &column.definition.id)),
        egui::Sense::drag(),
    );
    handle.register(&response);
    components::icons::bars_3(
        &mut row.new_child(egui::UiBuilder::new().max_rect(handle_rect)),
        16.0,
        components::colors::gray::_500,
    );
    row.add_space(components::design::spacing::MD);
    if components::tailwind_checkbox(&mut row, column.visible, &column.definition.label).clicked() {
        column.visible = !column.visible;
    }
    row.add_space(components::design::spacing::MD);
    row.label(
        egui::RichText::new(&column.definition.label)
            .font(components::design::typography::body())
            .color(components::colors::gray::_900),
    );
}

fn show_column_preview(ui: &mut egui::Ui, column: &EditableColumn) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, components::colors::WHITE);
    ui.painter().text(
        rect.left_center() + egui::vec2(48.0, 0.0),
        egui::Align2::LEFT_CENTER,
        &column.definition.label,
        components::design::typography::body(),
        components::colors::gray::_900,
    );
}
