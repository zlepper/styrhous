use super::discovery::ManagedDiscoveryAction;
use super::*;
use crate::ui::blade_text::gap;
use components::design::theme::Tone;
use components::tooltip::elided_label;
use components::{Pill, Tile, TileSize};
use egui::{Align, Layout, Rect, Sense, pos2, vec2};
use std::collections::BTreeMap;

/// A provider card's header (`.card-h`) and the padding of its body.
const CARD_HEADER_HEIGHT: f32 = 44.0;
const CARD_BODY_PADDING: f32 = space::S16;
/// A table row's height and the gap under it (`.tr`: 40 with a 4 point
/// transparent border), the header's height and a cell's side padding.
const TABLE_ROW_HEIGHT: f32 = 36.0;
const TABLE_ROW_GAP: f32 = space::S4;
const TABLE_HEADER_HEIGHT: f32 = 36.0;
const CELL_PADDING: f32 = space::S12;
const NAME_COLUMN: f32 = 180.0;
const SECOND_COLUMN: f32 = 112.0;
/// Room at the end of a row for its action.
const ACTION_COLUMN: f32 = 168.0;

/// Tags as `key=value` pairs, `·` between them.
pub(super) fn discovery_tag_summary(tags: &BTreeMap<String, String>) -> Option<String> {
    if tags.is_empty() {
        return None;
    }
    Some(
        tags.iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(" · "),
    )
}

pub(super) fn discovery_count(
    loading: bool,
    tool_available: bool,
    error: Option<&String>,
    count: usize,
) -> Option<usize> {
    (!loading && tool_available && error.is_none()).then_some(count)
}

/// The words of a provider's count chip: "2 clusters".
pub(super) fn cluster_count_label(count: usize) -> String {
    format!("{count} cluster{}", if count == 1 { "" } else { "s" })
}

/// A provider card (`.card` with `.card-h`): its icon tile, name and a status
/// chip, a divider, then `body` padded [`CARD_BODY_PADDING`].
pub(super) fn provider_card(
    ui: &mut egui::Ui,
    icon: icons::Icon,
    tile_tone: Tone,
    title: &str,
    status: Option<(&str, Tone)>,
    body: impl FnOnce(&mut egui::Ui),
) {
    let theme = ui.tokens();
    Card::new().padding(0).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let (header, _) = ui.allocate_exact_size(
            vec2(ui.available_width(), CARD_HEADER_HEIGHT),
            Sense::hover(),
        );
        let tile = Rect::from_min_size(
            pos2(
                header.left() + space::S16,
                header.center().y - TileSize::Sm.side() / 2.0,
            ),
            vec2(TileSize::Sm.side(), TileSize::Sm.side()),
        );
        Tile::new(icon)
            .tone(tile_tone)
            .size(TileSize::Sm)
            .paint(ui, tile);
        let mut row = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(header.shrink2(vec2(space::S16, 0.0)))
                .layout(Layout::right_to_left(Align::Center)),
        );
        if let Some((text, tone)) = status {
            Pill::new(text, tone).without_dot().show(&mut row);
        }
        let title_rect = Rect::from_min_max(
            pos2(tile.right() + space::S8, header.top()),
            pos2(row.min_rect().left() - space::S8, header.bottom()),
        );
        let mut title_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(title_rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        elided_label(
            &mut title_ui,
            type_scale::UI_STRONG,
            title,
            theme.text.primary,
        );
        let divider_y = header.bottom();
        ui.painter()
            .hline(header.x_range(), divider_y + 0.5, stroke::divider(&theme));
        ui.add_space(components::design::size::HAIRLINE);
        egui::Frame::new()
            .inner_margin(CARD_BODY_PADDING)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = ui.ctx().global_style().spacing.item_spacing.y;
                body(ui);
            });
    });
}

/// Body text of a provider card with `code` set in mono between `before`
/// and `after`: "Install the Azure CLI and run `az login` to discover …".
pub(super) fn show_install_hint(ui: &mut egui::Ui, before: &str, code: &str, after: &str) {
    let theme = ui.tokens();
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), type_scale::UI.line_height() + 2.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = space::S4;
            type_scale::UI.label(ui, before, theme.text.secondary);
            components::Chip::new(code).show(ui);
            type_scale::UI.label(ui, after, theme.text.secondary);
        },
    );
}

/// The column headings of a discovery table.
pub(super) fn show_discovery_headers(ui: &mut egui::Ui, columns: [&str; 3]) {
    let theme = ui.tokens();
    // Header and rows butt against each other; the rows keep their own gaps.
    ui.spacing_mut().item_spacing.y = 0.0;
    let (rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), TABLE_HEADER_HEIGHT),
        Sense::hover(),
    );
    for (index, text) in columns.into_iter().enumerate() {
        let (left, right) = column_span(rect, index);
        let cell = Rect::from_min_max(
            pos2(left + CELL_PADDING, rect.top()),
            pos2(right, rect.bottom()),
        );
        let mut ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(cell)
                .layout(Layout::left_to_right(Align::Center)),
        );
        type_scale::CAPTION.label(&mut ui, text, theme.text.muted);
        // A hairline between columns, short of the row's edges.
        if index < 3 {
            let x = right.min(rect.right());
            ui.painter().vline(
                x,
                (rect.top() + space::S12)..=(rect.bottom() - space::S12),
                stroke::divider(&theme),
            );
        }
    }
}

/// The horizontal extent of column `index` of a row in `rect`.
fn column_span(rect: Rect, index: usize) -> (f32, f32) {
    let second = rect.left() + NAME_COLUMN;
    let third = second + SECOND_COLUMN;
    match index {
        0 => (rect.left(), second),
        1 => (second, third),
        _ => (third, (rect.right() - ACTION_COLUMN).max(third)),
    }
}

#[derive(Clone, Copy)]
pub(super) enum DiscoveryImportState {
    Configured,
    Ready,
    Importing,
    Disabled,
}

/// One discovered cluster: three cells and the action to add it.
pub(super) struct DiscoveryRow<'a> {
    pub(super) cells: [&'a str; 3],
    /// Colour of the second cell, for a status word such as "Online".
    pub(super) second_tone: Option<Tone>,
    pub(super) import_state: DiscoveryImportState,
}

impl DiscoveryRow<'_> {
    pub(super) fn show(self, ui: &mut egui::Ui, on_click: impl FnOnce()) {
        let theme = ui.tokens();
        let (rect, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), TABLE_ROW_HEIGHT), Sense::hover());
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(radius::ROW),
            theme.surface.panel,
        );
        let colours = [
            theme.text.primary,
            self.second_tone
                .map_or(theme.text.primary, |tone| theme.tone(tone).text),
            theme.text.secondary,
        ];
        for (index, text) in self.cells.into_iter().enumerate() {
            let (left, right) = column_span(rect, index);
            let cell = Rect::from_min_max(
                pos2(left + CELL_PADDING, rect.top()),
                pos2(
                    (right - CELL_PADDING).max(left + CELL_PADDING),
                    rect.bottom(),
                ),
            );
            let mut cell_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(cell)
                    .layout(Layout::left_to_right(Align::Center)),
            );
            elided_label(&mut cell_ui, type_scale::UI, text, colours[index]);
        }
        let action = Rect::from_min_max(
            pos2(rect.right() - ACTION_COLUMN, rect.top()),
            pos2(rect.right() - CELL_PADDING, rect.bottom()),
        );
        let mut action_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(action)
                .layout(Layout::right_to_left(Align::Center)),
        );
        show_import_button(&mut action_ui, self.import_state, on_click);
        gap(ui, TABLE_ROW_GAP);
    }
}

pub(super) fn discovery_import_state(
    configured: bool,
    importing: bool,
    import_in_progress: bool,
) -> DiscoveryImportState {
    if configured {
        DiscoveryImportState::Configured
    } else if importing {
        DiscoveryImportState::Importing
    } else if import_in_progress {
        DiscoveryImportState::Disabled
    } else {
        DiscoveryImportState::Ready
    }
}

pub(super) fn show_discovery_loading(ui: &mut egui::Ui, message: &str) {
    let theme = ui.tokens();
    ui.horizontal(|ui| {
        components::Spinner::new().show(ui);
        type_scale::UI.label(ui, message, theme.text.secondary);
    });
}

pub(super) fn show_empty_discovery(ui: &mut egui::Ui, message: &str) {
    type_scale::UI.label(ui, message, ui.tokens().text.secondary);
}

pub(super) fn show_discovery_error(ui: &mut egui::Ui, error: &str) {
    type_scale::UI.label(ui, error, ui.tokens().danger.text);
}

pub(super) fn show_discovery_warning(ui: &mut egui::Ui, warning: &str) {
    components::Callout::new(Tone::Warning, warning).show(ui);
}

pub(super) fn show_import_button(
    ui: &mut egui::Ui,
    state: DiscoveryImportState,
    on_click: impl FnOnce(),
) {
    if matches!(state, DiscoveryImportState::Configured) {
        let colour = ui.tokens().success.text;
        ui.spacing_mut().item_spacing.x = space::S4;
        let words = type_scale::META.galley(ui.ctx(), "In kubeconfig", colour, f32::INFINITY);
        let (rect, _) = ui.allocate_exact_size(
            vec2(
                components::design::size::ICON_XS + space::S4 + words.rect.width(),
                type_scale::META.line_height(),
            ),
            Sense::hover(),
        );
        let icon = Rect::from_center_size(
            pos2(
                rect.left() + components::design::size::ICON_XS / 2.0,
                rect.center().y,
            ),
            vec2(
                components::design::size::ICON_XS,
                components::design::size::ICON_XS,
            ),
        );
        icons::Icon::Check.paint(ui, icon, colour);
        let mut label = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(
                    pos2(icon.right() + space::S4, rect.top()),
                    rect.right_bottom(),
                ))
                .layout(Layout::left_to_right(Align::Center)),
        );
        type_scale::META.label(&mut label, "In kubeconfig", colour);
    } else if Button::soft(if matches!(state, DiscoveryImportState::Importing) {
        "Adding…"
    } else {
        "Add to kubeconfig"
    })
    .leading_icon(icons::Icon::Plus)
    .size(ButtonSize::Sm)
    .enabled(matches!(state, DiscoveryImportState::Ready))
    .show(ui)
    .clicked()
    {
        on_click();
    }
}

#[derive(Debug)]
pub(super) struct ManagedDiscoveryEffect(pub(super) ManagedDiscoveryAction);

impl GlobalBladeEffect for ManagedDiscoveryEffect {
    fn apply(
        self: Box<Self>,
        context: &mut GlobalBladeEffectContext<'_>,
        navigation: &mut GlobalBladeNavigation<'_>,
    ) {
        let discovery = &mut context.ui_state.managed_cluster_discovery;
        discovery.error = None;
        match self.0 {
            ManagedDiscoveryAction::Refresh => {
                discovery.loading = true;
                discovery.aws.account_loading = true;
                discovery.aws.clear_scan();
                navigation
                    .commands_to_send()
                    .push(Box::new(LoadManagedClusterDiscovery));
                navigation
                    .commands_to_send()
                    .push(Box::new(LoadAwsAccounts {
                        request_id: discovery.aws.request_id,
                    }));
            }
            ManagedDiscoveryAction::ScanEks {
                account_id,
                profile,
            } => {
                discovery
                    .aws
                    .begin_scan(account_id.clone(), profile.clone());
                navigation
                    .commands_to_send()
                    .push(Box::new(ScanEksClusters {
                        request_id: discovery.aws.request_id,
                        account_id,
                        profile,
                    }));
            }
            ManagedDiscoveryAction::AddAks {
                subscription_id,
                resource_group,
                cluster_name,
            } => context.ui_state.begin_managed_import(
                ManagedClusterImport::Aks {
                    subscription_id,
                    resource_group,
                    cluster_name,
                },
                navigation.commands_to_send(),
            ),
            ManagedDiscoveryAction::AddTailscale { host_name } => {
                context.ui_state.begin_managed_import(
                    ManagedClusterImport::Tailscale { host_name },
                    navigation.commands_to_send(),
                );
            }
            ManagedDiscoveryAction::AddEks {
                account_id,
                region,
                name,
                profile,
            } => context.ui_state.begin_managed_import(
                ManagedClusterImport::Eks {
                    account_id,
                    region,
                    name,
                    profile,
                },
                navigation.commands_to_send(),
            ),
        }
    }
}
