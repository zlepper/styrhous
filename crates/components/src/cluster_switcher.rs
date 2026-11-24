//! The cluster switcher (C18): a palette of the kubeconfig's contexts as
//! [`ClusterTile`]s in sections (Pinned, Recent, Found in cloud accounts),
//! filtered by a search pill and provider chips, with a strip reading out
//! the highlighted cluster before you switch.

use egui::{
    Align, Align2, Context, Frame, Id, Key, Layout, Modal, Modifiers, Rect, Response, Sense, Ui,
    UiBuilder, Vec2, pos2, vec2,
};

use crate::avatar::Avatar;
use crate::design::theme::{self, ThemeExt, ThemeScope, Tone};
use crate::design::type_scale::{self, TextRole, Weight};
use crate::design::{radius, size, space};
use crate::frames::show_elevated;
use crate::interaction::{ControlState, Interaction, consume_key_with_text};
use crate::keycap::Keycap;
use crate::log_viewer::{VolumeBar, VolumeBars};
use crate::measured::{remember_width, remembered_width};
use crate::readout::{Readout, ReadoutRow};
use crate::search::{SearchField, SearchStatus};
use crate::toggle::ToggleButton;
use crate::{Button, PointingHand};

/// A tile's cluster name (`.ctile .nm`): UI 13/20 · 600.
const NAME_ROLE: TextRole = type_scale::UI_STRONG;
/// A tile's provider and region (`.ctile .mt`): condensed 11/14 caps, +0.06 em.
const META_ROLE: TextRole = type_scale::CAPTION_SM.with_letter_spacing(0.06);
/// A tile's node count or state (`.ctile .nn`): Plex Mono 11 in a 20 point line.
const DETAIL_ROLE: TextRole = type_scale::MONO_HINT;
/// The palette's title (`.palette .pt`).
const TITLE_ROLE: TextRole = type_scale::DIALOG_TITLE;
/// A section heading (`.palette .ps`): Bricolage 15/20.
const SECTION_ROLE: TextRole = type_scale::TITLE.with_size(15.0, 20.0);
/// The readout strip's context line: caption caps at +0.06 em.
const STRIP_META_ROLE: TextRole = type_scale::CAPTION.with_letter_spacing(0.06);
/// Gap between the parts of a tile (`.ctile` gap 12).
const TILE_GAP: f32 = space::S12;
/// Columns of tiles per row (S18).
const COLUMNS: usize = 3;
/// Gap between tiles (`grid` gap 8).
const GRID_GAP: f32 = space::S8;
/// Gap between the title row's items (`.row` gap 12).
const TITLE_GAP: f32 = space::S12;
/// Gap between the search pill and the provider chips (`.row` gap 8).
const FILTER_GAP: f32 = space::S8;
/// Space above the filter row.
const FILTER_TOP: f32 = space::S16;
/// Space above a section heading.
const SECTION_TOP: f32 = 18.0;
/// Space between a section heading and its tiles.
const SECTION_TO_TILES: f32 = 10.0;
/// Gap between a section heading and its count (`.ps` gap 8).
const SECTION_COUNT_GAP: f32 = space::S8;
/// Space above the readout strip.
const STRIP_TOP: f32 = space::S20;
/// Gap between the strip's parts (`.row` gap 20).
const STRIP_GAP: f32 = space::S20;
/// Space above the footer.
const FOOTER_TOP: f32 = 14.0;
/// Gap between the footer's parts (`.row` gap 6).
const FOOTER_GAP: f32 = space::S6;
/// The modal palette's distance from the window's top edge.
const MODAL_TOP: f32 = 48.0;
/// The room the modal palette leaves on each side of a narrow window.
const MODAL_MARGIN: f32 = space::S24;
/// The height of everything but the sections, until measured.
const FIXED_ESTIMATE: f32 = 300.0;
/// The least height the sections get.
const MIN_SCROLL: f32 = 120.0;

/// One cluster: a kubeconfig context or one found in a cloud account.
#[derive(Clone, Debug)]
pub struct ClusterEntry<'a> {
    pub name: &'a str,
    /// The avatar's initials.
    pub initials: &'a str,
    /// The avatar's category colour (`av-(n + 1)`).
    pub category: usize,
    /// Provider and region ("KIND · LOCAL", "EKS · NOT IN KUBECONFIG").
    pub meta: &'a str,
    /// The node count or state ("3 nodes", "unreachable", "add").
    pub detail: &'a str,
    /// The connection lamp; `None` for a cluster not in the kubeconfig.
    pub status: Option<Tone>,
    /// The pinned cluster's key ("⌘1").
    pub key: Option<&'a str>,
    /// What the readout strip shows while the cluster is highlighted.
    pub summary: Option<ClusterSummary<'a>>,
}

/// The readout strip of a highlighted cluster.
#[derive(Clone, Debug)]
pub struct ClusterSummary<'a> {
    /// Provider, region and version ("KIND · LOCAL · KUBERNETES 1.31").
    pub meta: &'a str,
    pub readouts: Vec<Readout<'a>>,
    /// The latency history, newest last, as fractions of the chart.
    pub latency: Vec<f32>,
}

/// A cluster tile (`.ctile`): avatar with its lamp, name over provider ·
/// region, node count or state and the pinned key, 60 points tall on
/// `surface-panel`. The highlighted (or current) tile is `item-selected`
/// with an accent ring. An unreachable cluster stays selectable.
#[must_use = "call `show` to add the tile"]
pub struct ClusterTile<'e, 'a> {
    entry: &'e ClusterEntry<'a>,
    highlighted: bool,
    forced_state: Option<ControlState>,
}

impl<'e, 'a> ClusterTile<'e, 'a> {
    pub fn new(entry: &'e ClusterEntry<'a>) -> Self {
        Self {
            entry,
            highlighted: false,
            forced_state: None,
        }
    }

    pub fn highlighted(mut self, highlighted: bool) -> Self {
        self.highlighted = highlighted;
        self
    }

    /// Paint `state` whatever the pointer does (boards).
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    /// Add the tile, `width` points wide.
    pub fn show(self, ui: &mut Ui, width: f32) -> Response {
        let entry = self.entry;
        let (rect, response) =
            ui.allocate_exact_size(vec2(width, size::CLUSTER_TILE), Sense::click());
        let response = response.with_pointing_hand();
        let label = format!("{}, {}, {}", entry.name, entry.meta, entry.detail);
        let highlighted = self.highlighted;
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Button,
                ui.is_enabled(),
                highlighted,
                &label,
            )
        });
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let theme = ui.tokens();
        let state = Interaction::of(&response, self.forced_state, false);
        ui.painter().add(crate::selection::selectable_surface(
            &theme,
            rect,
            radius::CLUSTER_TILE,
            (self.highlighted, state.is_active()),
            Some(theme.surface.panel),
            true,
        ));
        let center_y = rect.center().y;
        let mut left = rect.left() + size::CLUSTER_TILE_PADDING_X;
        Avatar::new(entry.initials)
            .category(entry.category)
            .status(entry.status)
            .paint(ui, pos2(left + size::AVATAR / 2.0, center_y));
        left += size::AVATAR + TILE_GAP;

        let mut right = rect.right() - size::CLUSTER_TILE_PADDING_X;
        if let Some(key) = entry.key {
            let keycap = Keycap::new(key);
            let key_size = keycap.size(ui);
            right -= key_size.x;
            keycap.paint(
                ui,
                Rect::from_min_size(pos2(right, center_y - key_size.y / 2.0), key_size),
            );
            right -= TILE_GAP;
        }
        let detail = DETAIL_ROLE.galley(ui.ctx(), entry.detail, theme.text.muted, f32::INFINITY);
        right -= detail.rect.width();
        type_scale::paint_galley(
            ui.painter(),
            pos2(right, center_y - detail.rect.height() / 2.0),
            detail,
            theme.text.muted,
        );
        right -= TILE_GAP;

        let text_width = (right - left).max(0.0);
        let block = NAME_ROLE.line_height() + META_ROLE.line_height();
        let top = center_y - block / 2.0;
        let name = NAME_ROLE.galley_elided(ui.ctx(), entry.name, theme.text.primary, text_width, 1);
        type_scale::paint_galley(ui.painter(), pos2(left, top), name, theme.text.primary);
        let meta = META_ROLE.galley_elided(ui.ctx(), entry.meta, theme.text.muted, text_width, 1);
        type_scale::paint_galley(
            ui.painter(),
            pos2(left, top + NAME_ROLE.line_height()),
            meta,
            theme.text.muted,
        );
        response
    }
}

/// A section of the switcher: its heading and its clusters.
#[derive(Clone, Debug)]
pub struct ClusterSection<'a> {
    pub title: &'a str,
    pub entries: Vec<ClusterEntry<'a>>,
}

/// A provider chip: "Kind 2".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderFilter<'a> {
    pub label: &'a str,
    pub count: usize,
}

/// A cluster of the switcher: its section and its place in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClusterTarget {
    pub section: usize,
    pub entry: usize,
}

/// What the switcher asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClusterAction {
    /// Switch this window to the cluster (Enter, a click, ⌘1…9); a cluster
    /// found in a cloud account is added first.
    Switch(ClusterTarget),
    /// Open the cluster in a new window (⌘↵).
    OpenInNewWindow(ClusterTarget),
    /// Pin or unpin the cluster (⌘P).
    TogglePin(ClusterTarget),
    /// Close the switcher (Esc, Close).
    Close,
}

/// What the switcher remembers between frames: the query, the provider
/// chip and the highlighted cluster (counted across the shown sections).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClusterSwitcherState {
    pub query: String,
    pub provider: usize,
    pub highlight: usize,
}

/// The cluster switcher palette (`.palette`): "Clusters" with its context
/// count and Close; the search pill (focused on open) and the provider
/// chips; the sections of tiles, three per row; the readout strip of the
/// highlighted cluster; and a footer with a notice, the unreachable count
/// and the keys.
///
/// Typing filters by name, provider and region; ↑ ↓ ← → move the
/// highlight, Enter switches, ⌘↵ opens a new window, ⌘P pins, ⌘1…9 switch
/// to a pinned cluster and Esc closes (S18). The caller narrows the
/// sections to the chosen provider ([`ClusterSwitcherState::provider`]).
#[must_use = "call `show` to add the switcher"]
pub struct ClusterSwitcher<'s, 'a> {
    state: &'s mut ClusterSwitcherState,
    providers: Vec<ProviderFilter<'a>>,
    sections: Vec<ClusterSection<'a>>,
    notice: Option<&'a str>,
    width: f32,
    focus_search: bool,
    forced_search_focus: bool,
}

impl<'s, 'a> ClusterSwitcher<'s, 'a> {
    pub fn new(state: &'s mut ClusterSwitcherState) -> Self {
        Self {
            state,
            providers: Vec::new(),
            sections: Vec::new(),
            notice: None,
            width: size::CLUSTER_SWITCHER_MAX_WIDTH,
            focus_search: false,
            forced_search_focus: false,
        }
    }

    pub fn provider(mut self, label: &'a str, count: usize) -> Self {
        self.providers.push(ProviderFilter { label, count });
        self
    }

    pub fn section(mut self, title: &'a str, entries: Vec<ClusterEntry<'a>>) -> Self {
        self.sections.push(ClusterSection { title, entries });
        self
    }

    /// A notice at the start of the footer ("Found 1 new cluster in AWS").
    pub fn notice(mut self, notice: &'a str) -> Self {
        self.notice = Some(notice);
        self
    }

    /// The palette's width, at most [`size::CLUSTER_SWITCHER_MAX_WIDTH`].
    pub fn width(mut self, width: f32) -> Self {
        self.width = width.min(size::CLUSTER_SWITCHER_MAX_WIDTH);
        self
    }

    /// Focus the search pill this frame (on open).
    pub fn focus_search(mut self, focus: bool) -> Self {
        self.focus_search = focus;
        self
    }

    /// Paint the search pill focused (boards).
    #[doc(hidden)]
    pub fn force_search_focus(mut self) -> Self {
        self.forced_search_focus = true;
        self
    }

    /// The sections narrowed to the query: name, provider or region.
    fn filtered(&self) -> Vec<(usize, Vec<usize>)> {
        let query = self.state.query.to_lowercase();
        self.sections
            .iter()
            .enumerate()
            .map(|(section, group)| {
                let entries = group
                    .entries
                    .iter()
                    .enumerate()
                    .filter(|(_, entry)| {
                        query.is_empty()
                            || entry.name.to_lowercase().contains(&query)
                            || entry.meta.to_lowercase().contains(&query)
                    })
                    .map(|(index, _)| index)
                    .collect::<Vec<_>>();
                (section, entries)
            })
            .filter(|(_, entries)| !entries.is_empty())
            .collect()
    }

    /// Show the palette as a modal over the scrim, centred and
    /// [`MODAL_TOP`] below the window's top edge. A click on the scrim
    /// closes it, like Esc.
    pub fn show_modal(mut self, ctx: &Context) -> Option<ClusterAction> {
        let id = Id::new("cluster-switcher-modal");
        self.width = self
            .width
            .min((ctx.content_rect().width() - 2.0 * MODAL_MARGIN).max(0.0));
        let response = Modal::new(id)
            .area(
                Modal::default_area(id)
                    .anchor(Align2::CENTER_TOP, vec2(0.0, MODAL_TOP))
                    .fade_in(false),
            )
            .backdrop_color(ctx.tokens().shade.scrim)
            .frame(Frame::NONE)
            .show(ctx, |ui| self.show(ui));
        let clicked_away = response.is_top_modal
            && !response.any_popup_open
            && response.backdrop_response.clicked();
        response
            .inner
            .or(clicked_away.then_some(ClusterAction::Close))
    }

    /// Add the palette at the cursor.
    pub fn show(mut self, ui: &mut Ui) -> Option<ClusterAction> {
        let theme = ui.tokens();
        let filtered = self.filtered();
        let targets: Vec<ClusterTarget> = filtered
            .iter()
            .flat_map(|(section, entries)| {
                entries.iter().map(|entry| ClusterTarget {
                    section: *section,
                    entry: *entry,
                })
            })
            .collect();
        if !targets.is_empty() {
            self.state.highlight = self.state.highlight.min(targets.len() - 1);
        }
        let mut action = self.keyboard(ui, &targets);
        let frame = egui::Frame::new()
            .fill(theme.surface.raised)
            .corner_radius(radius::OVERLAY)
            .inner_margin(egui::Margin::same(size::CLUSTER_SWITCHER_PADDING as i8));
        let Self {
            state,
            providers,
            sections,
            notice,
            width,
            focus_search,
            forced_search_focus,
        } = self;
        show_elevated(ui, frame, theme.shadow.overlay, |ui| {
            theme::scope(ui, ThemeScope::CommandPalette, |ui| {
                let inner = width - 2.0 * size::CLUSTER_SWITCHER_PADDING;
                ui.set_width(inner);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                // Clusters found in cloud accounts (no lamp) are not contexts yet.
                let total = sections
                    .iter()
                    .flat_map(|section| section.entries.iter())
                    .filter(|entry| entry.status.is_some())
                    .count();
                let contexts = crate::search::counted(total, "context", "contexts");
                if title_row(ui, &contexts) {
                    action = Some(ClusterAction::Close);
                }
                ui.add_space(FILTER_TOP);
                filter_row(
                    ui,
                    state,
                    &providers,
                    &contexts,
                    inner,
                    focus_search,
                    forced_search_focus,
                );

                let tile_width = (inner - GRID_GAP * (COLUMNS - 1) as f32) / COLUMNS as f32;
                // The sections scroll when the window is too short for them; the
                // title, search, strip and footer stay.
                let fixed_id = ui.id().with("cluster-switcher-fixed");
                let seen_id = ui.id().with("cluster-switcher-highlight");
                let fixed = ui
                    .data(|data| data.get_temp::<f32>(fixed_id))
                    .unwrap_or(FIXED_ESTIMATE);
                let room = ui.ctx().content_rect().height()
                    - MODAL_TOP
                    - MODAL_MARGIN
                    - 2.0 * size::CLUSTER_SWITCHER_PADDING;
                let moved =
                    ui.data(|data| data.get_temp::<usize>(seen_id)) != Some(state.highlight);
                let scrolled = crate::scroll::vertical()
                    .id_salt("cluster-switcher-sections")
                    .max_height((room - fixed).max(MIN_SCROLL))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        let mut flat = 0;
                        for (section_index, entries) in &filtered {
                            let section = &sections[*section_index];
                            ui.add_space(SECTION_TOP);
                            section_heading(ui, section.title, entries.len());
                            ui.add_space(SECTION_TO_TILES);
                            for (row_index, row) in entries.chunks(COLUMNS).enumerate() {
                                if row_index > 0 {
                                    ui.add_space(GRID_GAP);
                                }
                                ui.allocate_ui_with_layout(
                                    vec2(inner, size::CLUSTER_TILE),
                                    Layout::left_to_right(Align::Min),
                                    |ui| {
                                        ui.spacing_mut().item_spacing = vec2(GRID_GAP, 0.0);
                                        for entry_index in row {
                                            let entry = &section.entries[*entry_index];
                                            let target = ClusterTarget {
                                                section: *section_index,
                                                entry: *entry_index,
                                            };
                                            let highlighted = state.highlight == flat;
                                            let tile = ClusterTile::new(entry)
                                                .highlighted(highlighted)
                                                .show(ui, tile_width);
                                            if highlighted && moved {
                                                tile.scroll_to_me(None);
                                            }
                                            if tile.clicked() {
                                                action = Some(ClusterAction::Switch(target));
                                            }
                                            flat += 1;
                                        }
                                    },
                                );
                            }
                        }
                    });
                ui.data_mut(|data| data.insert_temp(seen_id, state.highlight));
                let scroll_height = scrolled.inner_rect.height();
                let highlighted = targets
                    .get(state.highlight)
                    .map(|target| &sections[target.section].entries[target.entry]);
                if let Some(entry) = highlighted
                    && let Some(summary) = &entry.summary
                {
                    ui.add_space(STRIP_TOP);
                    readout_strip(ui, entry, summary, inner);
                }
                ui.add_space(FOOTER_TOP);
                let unreachable = sections
                    .iter()
                    .flat_map(|section| section.entries.iter())
                    .filter(|entry| entry.status == Some(Tone::Danger))
                    .count();
                footer(ui, notice, unreachable, inner);
                let fixed_now = ui.min_rect().height() - scroll_height;
                if (fixed_now - fixed).abs() > 0.5 {
                    ui.data_mut(|data| data.insert_temp(fixed_id, fixed_now));
                    ui.ctx().request_discard("cluster switcher height changed");
                }
            });
        });
        action
    }

    /// The switcher's keys.
    fn keyboard(&mut self, ui: &Ui, targets: &[ClusterTarget]) -> Option<ClusterAction> {
        let count = targets.len();
        let current = targets.get(self.state.highlight).copied();
        ui.input_mut(|input| {
            if input.consume_key(Modifiers::NONE, Key::Escape) {
                return Some(ClusterAction::Close);
            }
            if let Some(target) = current {
                if input.consume_key(Modifiers::COMMAND, Key::Enter) {
                    return Some(ClusterAction::OpenInNewWindow(target));
                }
                if input.consume_key(Modifiers::NONE, Key::Enter) {
                    return Some(ClusterAction::Switch(target));
                }
                if consume_key_with_text(input, Modifiers::COMMAND, Key::P) {
                    return Some(ClusterAction::TogglePin(target));
                }
            }
            None
        })
        .or_else(|| self.pinned_key(ui, targets))
        .or_else(|| {
            if count > 0 {
                self.state.highlight = self.moved_highlight(ui, targets);
            }
            None
        })
    }

    /// The highlight after the arrow keys: ↑ ↓ to the tile above or below
    /// (the nearest one in a shorter row), ← → to the previous or next
    /// tile. While a query is typed ← → move the search's caret instead.
    fn moved_highlight(&self, ui: &Ui, targets: &[ClusterTarget]) -> usize {
        let highlight = self.state.highlight.min(targets.len() - 1);
        let horizontal = self.state.query.is_empty();
        let (vertical, sideways) = ui.input_mut(|input| {
            let mut pressed = |key| input.consume_key(Modifiers::NONE, key);
            let vertical =
                isize::from(pressed(Key::ArrowDown)) - isize::from(pressed(Key::ArrowUp));
            let sideways = if horizontal {
                isize::from(pressed(Key::ArrowRight)) - isize::from(pressed(Key::ArrowLeft))
            } else {
                0
            };
            (vertical, sideways)
        });
        // The grid's rows: each section's tiles, three to a row.
        let mut rows: Vec<std::ops::Range<usize>> = Vec::new();
        let mut start = 0;
        for (index, target) in targets.iter().enumerate() {
            let next = targets.get(index + 1);
            if next.is_none_or(|next| next.section != target.section)
                || index + 1 - start == COLUMNS
            {
                rows.push(start..index + 1);
                start = index + 1;
            }
        }
        let row = rows
            .iter()
            .position(|row| row.contains(&highlight))
            .unwrap_or_default();
        let column = highlight - rows[row].start;
        let highlight = match row
            .checked_add_signed(vertical)
            .and_then(|row| rows.get(row))
        {
            Some(target_row) if vertical != 0 => {
                (target_row.start + column).min(target_row.end - 1)
            }
            _ => highlight,
        };
        highlight
            .saturating_add_signed(sideways)
            .min(targets.len() - 1)
    }

    /// ⌘1…9: the pinned cluster showing that key.
    fn pinned_key(&self, ui: &Ui, targets: &[ClusterTarget]) -> Option<ClusterAction> {
        const DIGITS: [Key; 9] = [
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
        let pressed = ui.input_mut(|input| {
            DIGITS
                .iter()
                .position(|key| consume_key_with_text(input, Modifiers::COMMAND, *key))
        })?;
        let wanted = format!("⌘{}", pressed + 1);
        targets
            .iter()
            .find(|target| {
                self.sections[target.section].entries[target.entry].key == Some(wanted.as_str())
            })
            .map(|target| ClusterAction::Switch(*target))
    }
}

/// "Clusters 4 CONTEXTS … ×"; returns whether Close was clicked.
fn title_row(ui: &mut Ui, contexts: &str) -> bool {
    let theme = ui.tokens();
    let (rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), TITLE_ROLE.line_height()),
        Sense::hover(),
    );
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    row.spacing_mut().item_spacing = vec2(TITLE_GAP, 0.0);
    TITLE_ROLE.label(&mut row, "Clusters", theme.text.primary);
    type_scale::CAPTION.label(&mut row, contexts, theme.text.muted);
    let mut close = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::right_to_left(Align::Center)),
    );
    Button::close("Close").show(&mut close).clicked()
}

/// The search pill and the provider chips.
fn filter_row(
    ui: &mut Ui,
    state: &mut ClusterSwitcherState,
    providers: &[ProviderFilter<'_>],
    contexts: &str,
    width: f32,
    focus: bool,
    forced_focus: bool,
) {
    let chips: Vec<ToggleButton> = providers
        .iter()
        .enumerate()
        .map(|(index, provider)| {
            ToggleButton::text(
                format!("{} {}", provider.label, provider.count),
                state.provider == index,
            )
        })
        .collect();
    // The chips' width from the previous pass: the search pill fills the rest.
    let chips_id = ui.id().with("cluster-switcher-chips");
    let chips_width = remembered_width(ui, chips_id).unwrap_or(0.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, size::TOP_BAR_CONTROL), Sense::hover());
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    row.spacing_mut().item_spacing = vec2(FILTER_GAP, 0.0);
    let search_width = (width - chips_width - FILTER_GAP).max(0.0);
    let mut search = SearchField::pill(&mut state.query)
        .width(search_width)
        .id_salt("cluster-switcher-search")
        .placeholder("Find a cluster, provider or region")
        .accessibility_label("Find a cluster")
        .status(SearchStatus::Label(contexts.to_owned()))
        .request_focus(focus);
    if forced_focus {
        search = search.force_state(ControlState::Focused);
    }
    if search.show(&mut row).changed {
        state.highlight = 0;
    }
    let chips_left = row.cursor().left();
    for (index, chip) in chips.into_iter().enumerate() {
        if chip.show(&mut row).clicked() {
            state.provider = index;
            state.highlight = 0;
        }
    }
    let measured = (row.min_rect().right() - chips_left).max(0.0);
    remember_width(
        ui,
        chips_id,
        measured,
        "cluster switcher chips changed width",
    );
}

/// "Pinned 2": the heading with its count on one baseline.
fn section_heading(ui: &mut Ui, title: &str, count: usize) {
    let theme = ui.tokens();
    let ppp = ui.pixels_per_point();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(SECTION_COUNT_GAP, 0.0);
        ui.set_height(SECTION_ROLE.line_height());
        let baseline = SECTION_ROLE.css_baseline(ppp);
        SECTION_ROLE.label_on_baseline(ui, title, theme.text.primary, baseline);
        type_scale::CAPTION.label_on_baseline(ui, &count.to_string(), theme.text.muted, baseline);
    });
}

/// The readout strip of the highlighted cluster.
fn readout_strip(ui: &mut Ui, entry: &ClusterEntry<'_>, summary: &ClusterSummary<'_>, width: f32) {
    let theme = ui.tokens();
    let readouts = ReadoutRow::new(&summary.readouts).on_raised_surface();
    let content_height = readouts.height(ui).max(size::AVATAR);
    let height = content_height + 2.0 * size::CLUSTER_STRIP_PADDING.y;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    ui.painter()
        .rect_filled(rect, radius::CARD, theme.surface.panel);
    let content = rect.shrink2(size::CLUSTER_STRIP_PADDING);
    let center_y = content.center().y;
    let mut left = content.left();
    Avatar::new(entry.initials)
        .category(entry.category)
        .status(entry.status)
        .paint(ui, pos2(left + size::AVATAR / 2.0, center_y));
    left += size::AVATAR + STRIP_GAP;

    let block = type_scale::UI.line_height() + STRIP_META_ROLE.line_height();
    let top = center_y - block / 2.0;
    let name = NAME_ROLE.galley_elided(
        ui.ctx(),
        entry.name,
        theme.text.primary,
        size::CLUSTER_STRIP_NAME,
        1,
    );
    let id = ui.id().with(("cluster-strip", entry.name));
    type_scale::text_at(
        ui,
        id.with("name"),
        pos2(left, top),
        name,
        entry.name,
        theme.text.primary,
    );
    let meta = STRIP_META_ROLE.galley_elided(
        ui.ctx(),
        summary.meta,
        theme.text.muted,
        size::CLUSTER_STRIP_NAME,
        1,
    );
    type_scale::text_at(
        ui,
        id.with("meta"),
        pos2(left, top + type_scale::UI.line_height()),
        meta,
        summary.meta,
        theme.text.muted,
    );
    left += size::CLUSTER_STRIP_NAME + STRIP_GAP;

    let chart_left = content.right() - size::CLUSTER_LATENCY.x;
    let readouts_rect = Rect::from_min_max(
        pos2(left, content.top()),
        pos2(chart_left - STRIP_GAP, content.bottom()),
    );
    let mut readouts_ui = ui.new_child(UiBuilder::new().max_rect(readouts_rect));
    readouts.show(&mut readouts_ui);

    // No history, no chart (and no empty chart node).
    if summary.latency.is_empty() {
        return;
    }
    let bars: Vec<VolumeBar> = summary
        .latency
        .iter()
        .map(|height| VolumeBar {
            height: *height,
            tone: None,
        })
        .collect();
    let chart = Rect::from_center_size(
        pos2(chart_left + size::CLUSTER_LATENCY.x / 2.0, center_y),
        size::CLUSTER_LATENCY,
    );
    let mut chart_ui = ui.new_child(UiBuilder::new().max_rect(chart));
    let _ = VolumeBars::new(&bars)
        .height(size::CLUSTER_LATENCY.y)
        .show(&mut chart_ui);
}

/// The footer: the notice, the unreachable count and the keys.
fn footer(ui: &mut Ui, notice: Option<&str>, unreachable: usize, width: f32) {
    let theme = ui.tokens();
    let role = type_scale::META;
    let ppp = ui.pixels_per_point();
    let (rect, response) = ui.allocate_exact_size(vec2(width, size::KEY_HEIGHT), Sense::hover());
    let id = response.id;
    // Text centred in the row, and text running on the keycaps' baseline
    // (`.kbd` inline in the key hints).
    let centred_top = rect.top() + (size::KEY_HEIGHT - role.line_height()) / 2.0;
    let key_baseline = (size::KEY_HEIGHT - type_scale::KEY.line_height()) / 2.0
        + type_scale::KEY.css_baseline(ppp);
    let inline_top = rect.top() + key_baseline - role.css_baseline(ppp);
    let text = |ui: &Ui, role: TextRole, text: &str, color, left: f32, top: f32, salt: usize| {
        let galley = role.galley(ui.ctx(), text, color, f32::INFINITY);
        let width = galley.rect.width();
        type_scale::text_at(ui, id.with(salt), pos2(left, top), galley, text, color);
        width
    };
    let width_of = |ui: &Ui, text: &str| {
        role.galley(ui.ctx(), text, theme.text.muted, f32::INFINITY)
            .rect
            .width()
    };
    if let Some(notice) = notice {
        let success = theme.tone(Tone::Success).text;
        text(
            ui,
            role.with_weight(Weight::SemiBold),
            notice,
            success,
            rect.left(),
            centred_top,
            0,
        );
    }
    let space = width_of(ui, " ");
    let mut right = rect.right();
    let mut runs: Vec<(&str, bool)> = vec![
        ("pin", false),
        ("⌘P", true),
        ("new window ·", false),
        ("⌘↵", true),
        ("· Enter switch ·", false),
    ];
    runs.reverse();
    let widths: Vec<f32> = runs
        .iter()
        .map(|(run, key)| {
            if *key {
                Keycap::new(run).size(ui).x
            } else {
                width_of(ui, run)
            }
        })
        .collect();
    let hints_width = widths.iter().sum::<f32>() + space * (runs.len() - 1) as f32;
    let mut left = right - hints_width;
    right = left;
    for (index, ((run, key), width)) in runs.iter().zip(&widths).enumerate() {
        if *key {
            let keycap = Keycap::new(run);
            keycap.paint(
                ui,
                Rect::from_min_size(pos2(left, rect.top()), keycap.size(ui)),
            );
        } else {
            text(ui, role, run, theme.text.muted, left, inline_top, index + 1);
        }
        left += width + space;
    }
    if unreachable > 0 {
        let words = format!("{unreachable} unreachable");
        let width = width_of(ui, &words);
        text(
            ui,
            role,
            &words,
            theme.danger.text,
            right - FOOTER_GAP - width,
            centred_top,
            99,
        );
    }
}
