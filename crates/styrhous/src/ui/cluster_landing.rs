//! The workspace with no cluster to show (A01): the contexts as tiles to
//! start from, and the ways to find more.

use super::cluster_switcher::{ChoiceTarget, ClusterChoice, KEYED, context_choices, key_label};
use super::state::{ClusterConnectionState, UiState};
use super::widgets::count_noun;
use crate::worker::WorkerCommandBox;
use components::design::theme::ThemeExt;
use components::design::{size, space, type_scale};
use components::icons::Icon;
use components::{Button, ClusterTile, Keycap, ScopePill, SearchPill, TopBar};
use egui::{Align, Key, Layout, Modifiers, Rect, Sense, Ui, UiBuilder, pos2, vec2};

/// Where the title starts, below the top of the page (A01 `top: 200px`).
const CONTENT_TOP: f32 = 200.0;
/// Between the title and the line under it.
const TITLE_GAP: f32 = space::S6;
/// Between the line under the title and the tiles.
const TILES_TOP: f32 = TITLE_GAP + space::S24;
/// The tiles' grid: two columns of tiles.
const GRID_WIDTH: f32 = 640.0;
const GRID_GAP: f32 = 10.0;
/// Between the tiles and the buttons.
const BUTTONS_TOP: f32 = TITLE_GAP + space::S20;
const BUTTONS_GAP: f32 = space::S8;
/// Between the buttons and the key hint.
const HINT_TOP: f32 = TITLE_GAP + 14.0;
const HINT_GAP: f32 = space::S6;

/// Whether the workspace has no cluster to show, so the landing page does.
pub(super) fn is_shown(ui_state: &UiState) -> bool {
    ui_state
        .selected_cluster
        .and_then(|key| ui_state.clusters.get(&key))
        .is_none_or(|cluster| matches!(cluster.connection, ClusterConnectionState::Disconnected))
}

/// ⌘1…9: the Nth tile of the landing page, or the Nth pinned cluster
/// anywhere else. Not while the switcher is open (it takes the keys).
pub(super) fn quick_switch(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands: &mut Vec<WorkerCommandBox>,
) {
    if ui_state.cluster_switcher.is_some()
        || ctx.memory(|memory| memory.top_modal_layer().is_some())
    {
        return;
    }
    const DIGITS: [Key; KEYED] = [
        Key::Num1,
        Key::Num2,
        Key::Num3,
        Key::Num4,
        Key::Num5,
        Key::Num6,
        Key::Num7,
        Key::Num8,
        Key::Num9,
    ];
    let Some(slot) = ctx.input_mut(|input| {
        DIGITS
            .iter()
            .position(|key| input.consume_key(Modifiers::COMMAND, *key))
    }) else {
        return;
    };
    let choices = context_choices(ui_state);
    let choice = if is_shown(ui_state) {
        choices.get(slot)
    } else {
        // The pinned clusters that still exist, as the switcher numbers them.
        ui_state
            .cluster_roster
            .pinned
            .iter()
            .filter_map(|wanted| {
                choices.iter().find(|choice| {
                    matches!(&choice.target, ChoiceTarget::Context { name, .. } if name == wanted)
                })
            })
            .nth(slot)
    };
    if let Some(choice) = choice {
        ui_state.choose_cluster(choice, commands);
    }
}

/// The disabled top bar the page keeps (A01): the controls of a list, with
/// no list to act on.
fn idle_top_bar(ui: &mut Ui) {
    let bar = TopBar::new(ui.ctx().content_rect().width());
    bar.show(
        ui,
        |ui| {
            ui.add_enabled_ui(false, |ui| {
                let mut query = String::new();
                SearchPill::new(&mut query)
                    .id_salt("landing-search")
                    .key_when_disabled()
                    .width(bar.search_width())
                    .show(ui);
                ScopePill::namespaces(Vec::<String>::new(), None)
                    .id_salt("landing-scope")
                    .width(bar.scope_width())
                    .combobox()
                    .placeholder("Select namespaces")
                    .filter_by(|name: &String| name.as_str())
                    .show_items(ui, &Vec::<String>::new(), |_, _| ());
            });
        },
        None,
    );
}

/// Show the landing page in the workspace's page.
pub(super) fn show(ui: &mut Ui, ui_state: &mut UiState, commands: &mut Vec<WorkerCommandBox>) {
    let page_top = ui.available_rect_before_wrap().top();
    idle_top_bar(ui);
    let mut choices = context_choices(ui_state);
    for (index, choice) in choices.iter_mut().enumerate() {
        choice.key = key_label(index);
    }
    let page = ui.available_rect_before_wrap();
    let left = page.center().x - GRID_WIDTH / 2.0;
    let top = page_top + CONTENT_TOP;
    let mut content = ui.new_child(
        UiBuilder::new()
            .max_rect(Rect::from_min_size(
                pos2(page.left(), top),
                vec2(page.width(), page.bottom() - top),
            ))
            .layout(Layout::top_down(Align::Center)),
    );
    let ui = &mut content;
    let tokens = ui.tokens();
    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
    type_scale::DISPLAY.label(ui, "Choose a cluster", tokens.text.primary);
    ui.add_space(TITLE_GAP);
    let source = if ui_state.kubeconfig_source.is_empty() {
        "your kubeconfig"
    } else {
        ui_state.kubeconfig_source.as_str()
    };
    let line = if choices.is_empty() {
        "No contexts in your kubeconfig. Find clusters in your cloud accounts.".to_owned()
    } else {
        format!(
            "{} in {}. Pick one to start, or find more in your cloud accounts.",
            count_noun(choices.len(), "context"),
            source
        )
    };
    type_scale::BODY.label(ui, &line, tokens.text.secondary);
    ui.add_space(TILES_TOP);
    if let Some(choice) = tiles(ui, &choices, left) {
        ui_state.choose_cluster(choice, commands);
    }
    ui.add_space(BUTTONS_TOP);
    let (find, settings) = centred_row(ui, BUTTONS_GAP, size::CONTROL_MD, |ui| {
        let find = Button::secondary("Find clusters")
            .leading_icon(Icon::MagnifyingGlass)
            .show(ui)
            .clicked();
        (
            find,
            Button::ghost("Open kubeconfig settings").show(ui).clicked(),
        )
    });
    if find {
        ui_state.open_cluster_discovery(commands);
    }
    if settings {
        ui_state.open_contexts_settings(commands);
    }
    ui.add_space(HINT_TOP);
    centred_row(ui, HINT_GAP, type_scale::META_ROW.line_height(), |ui| {
        Keycap::new("⌘K").show(ui);
        type_scale::META.label(ui, "switch clusters from anywhere", tokens.text.muted);
    });
}

/// The tiles in two columns; the one clicked, if any.
fn tiles<'a>(ui: &mut Ui, choices: &'a [ClusterChoice], left: f32) -> Option<&'a ClusterChoice> {
    let tile_width = (GRID_WIDTH - GRID_GAP) / 2.0;
    let mut clicked = None;
    for (row_index, row) in choices.chunks(2).enumerate() {
        if row_index > 0 {
            ui.add_space(GRID_GAP);
        }
        let (rect, _) =
            ui.allocate_exact_size(vec2(GRID_WIDTH, size::CLUSTER_TILE), Sense::hover());
        let mut row_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(Rect::from_min_size(pos2(left, rect.top()), rect.size()))
                .layout(Layout::left_to_right(Align::Min)),
        );
        row_ui.spacing_mut().item_spacing = vec2(GRID_GAP, 0.0);
        for choice in row {
            if ClusterTile::new(&choice.entry())
                .show(&mut row_ui, tile_width)
                .clicked()
            {
                clicked = Some(choice);
            }
        }
    }
    clicked
}

/// A row of controls centred on the page. Its width is measured on the first
/// pass and the row laid out again once it is known.
fn centred_row<R>(ui: &mut Ui, gap: f32, height: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let id = ui.id().with(("centred-row", ui.cursor().top() as i32));
    let width = ui.data(|data| data.get_temp::<f32>(id)).unwrap_or(0.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(Rect::from_min_max(
                pos2(rect.center().x - width / 2.0, rect.top()),
                rect.right_bottom(),
            ))
            .layout(Layout::left_to_right(Align::Center)),
    );
    row.spacing_mut().item_spacing = vec2(gap, 0.0);
    let result = add(&mut row);
    let measured = row.min_rect().width();
    if (measured - width).abs() > 0.01 {
        ui.data_mut(|data| data.insert_temp(id, measured));
        ui.ctx().request_discard("landing row changed width");
    }
    result
}
