use super::global_blade::{GlobalBladeContent, GlobalBladeRenderContext, GlobalBladeRenderResult};
use super::table_preferences::{
    PersistedResourceTablePreferences, ResourceTableKey, TableColumnDefinition,
};
use components::{ReorderHandle, ReorderableTable};

#[derive(Debug, Clone)]
pub(super) struct ResourceTableSettingsTarget {
    key: ResourceTableKey,
    columns: Vec<EditableColumn>,
    resource_detail_owner: Option<u64>,
}

#[derive(Debug, Clone)]
struct EditableColumn {
    definition: TableColumnDefinition,
    visible: bool,
}

pub(super) fn target(
    preferences: &mut PersistedResourceTablePreferences,
    key: ResourceTableKey,
    definitions: &[TableColumnDefinition],
) -> ResourceTableSettingsTarget {
    let columns = preferences
        .all_columns(&key, definitions)
        .into_iter()
        .map(|(definition, visible)| EditableColumn {
            definition,
            visible,
        })
        .collect();
    ResourceTableSettingsTarget {
        key,
        columns,
        resource_detail_owner: None,
    }
}

impl ResourceTableSettingsTarget {
    pub(super) fn set_resource_detail_owner(&mut self, history_entry_id: u64) {
        self.resource_detail_owner = Some(history_entry_id);
    }
}

pub(super) fn show_target(
    ui: &mut egui::Ui,
    target: &mut ResourceTableSettingsTarget,
    preferences: &mut PersistedResourceTablePreferences,
) {
    ui.label(
        egui::RichText::new(
            "Choose which columns are visible and drag rows to change their order.",
        )
        .font(components::design::typography::body())
        .color(components::colors::gray::_600),
    );
    ui.add_space(components::design::spacing::XL);
    let width = ui.available_width();
    let moved = ReorderableTable::new(("resource-table-settings-rows", &target.key), 44.0).show(
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
}

impl GlobalBladeContent for ResourceTableSettingsTarget {
    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        _context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        ui.label(
            egui::RichText::new("Configure columns")
                .font(components::design::typography::page_title())
                .color(components::colors::gray::_900),
        );
        GlobalBladeRenderResult::default()
    }

    fn render_body(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        show_target(ui, self, context.table_preferences());
        GlobalBladeRenderResult::default()
    }

    fn is_owned_by_resource_detail(&self, history_entry_id: u64) -> bool {
        self.resource_detail_owner == Some(history_entry_id)
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
