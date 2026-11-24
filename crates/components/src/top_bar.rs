//! The top bar of a resource list (C16): the search pill, the scope pill,
//! the filter chips and the live meta in one [`size::TOP_BAR`] band, so the
//! list's controls are visible keys rather than menus.
//!
//! ```ignore
//! let bar = TopBar::new(window_width);
//! bar.show(
//!     ui,
//!     |ui| {
//!         SearchPill::new(&mut query).width(bar.search_width()).show(ui);
//!         ScopePill::namespaces(["kube-system"], Some(true))
//!             .width(bar.scope_width())
//!             .combobox()
//!             .filter_by(|ns: &String| ns)
//!             .show_items(ui, &namespaces, |list, ns| { list.item(ns); });
//!         if bar.shows_grouping() {
//!             FilterChip::grouping(Some("namespace")).show(ui);
//!         }
//!         FilterChip::problems(2, problems_only).show(ui);
//!     },
//!     Some(LiveMeta::new("12 pods")),
//! );
//! ```

use std::sync::Arc;

use egui::{
    Align, Galley, Key, Layout, Modifiers, Rect, Response, Sense, Ui, UiBuilder, pos2, vec2,
};

use crate::combobox::{Combobox, NoFilter};
use crate::design::theme::{ThemeExt, Tone};
use crate::design::type_scale;
use crate::design::{size, space};
use crate::field::FieldSize;
use crate::icons::Icon;
use crate::interaction::{ControlState, consume_key_with_text};
use crate::keycap::Keycap;
use crate::search::{SearchField, SearchFieldResponse, SearchStatus, format_count};
use crate::toggle::ToggleButton;

/// Gap between the items of the live meta (`.tmeta` gap 6).
const META_GAP: f32 = space::S6;
/// Gap between the controls of the top bar (`.topbar` gap 8).
const BAR_GAP: f32 = space::S8;
/// How many namespace names a scope pill spells out before its `+N`.
const SCOPE_NAMES: usize = 2;

/// The keys a list's top bar answers while nothing else has focus (S16).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TopBarKeys {
    /// `P`: toggle Problems only.
    pub toggle_problems: bool,
    /// `[`: the previous problem row.
    pub previous_problem: bool,
    /// `]`: the next problem row.
    pub next_problem: bool,
}

/// The band of a list's controls (`.topbar`): its controls left to right
/// [`BAR_GAP`] apart, vertically centred in [`size::TOP_BAR`], and the live
/// meta at its end. Narrow windows (under [`size::TOP_BAR_NARROW_WINDOW`])
/// shrink the pills, drop the grouping chip and hide the meta unless it
/// warns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TopBar {
    narrow: bool,
}

impl TopBar {
    /// The top bar of a window `window_width` points wide.
    pub fn new(window_width: f32) -> Self {
        Self {
            narrow: window_width < size::TOP_BAR_NARROW_WINDOW,
        }
    }

    /// Whether the window is narrow.
    pub fn is_narrow(&self) -> bool {
        self.narrow
    }

    /// The width of the search pill.
    pub fn search_width(&self) -> f32 {
        if self.narrow {
            size::SEARCH_PILL_NARROW
        } else {
            size::SEARCH_PILL
        }
    }

    /// The width of the scope pill.
    pub fn scope_width(&self) -> f32 {
        if self.narrow {
            size::SCOPE_PILL_NARROW
        } else {
            size::SCOPE_PILL
        }
    }

    /// Whether the grouping chip shows (not in a narrow window).
    pub fn shows_grouping(&self) -> bool {
        !self.narrow
    }

    /// Read the top bar's keys ([`TopBarKeys`]) unless a widget has focus
    /// (typing in a field must not toggle filters).
    pub fn keys(ui: &Ui) -> TopBarKeys {
        if ui.memory(|memory| memory.focused().is_some()) {
            return TopBarKeys::default();
        }
        ui.input_mut(|input| TopBarKeys {
            toggle_problems: consume_key_with_text(input, Modifiers::NONE, Key::P),
            previous_problem: consume_key_with_text(input, Modifiers::NONE, Key::OpenBracket),
            next_problem: consume_key_with_text(input, Modifiers::NONE, Key::CloseBracket),
        })
    }

    /// Add the band across the available width: `add_controls` from its
    /// start, the live `meta` against its end. A narrow window shows the
    /// meta only while it warns, and no window shows it where it would run
    /// into the controls.
    pub fn show(
        self,
        ui: &mut Ui,
        add_controls: impl FnOnce(&mut Ui),
        meta: Option<LiveMeta<'_>>,
    ) -> TopBarResponse {
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), size::TOP_BAR), Sense::hover());
        let mut controls = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        controls.spacing_mut().item_spacing = vec2(BAR_GAP, 0.0);
        add_controls(&mut controls);
        // The controls' cursor is one gap past the last control.
        let room = Rect::from_min_max(pos2(controls.cursor().left(), rect.top()), rect.max);
        let meta = meta
            // A narrow window keeps only a meta that warns.
            .filter(|meta| !self.narrow || meta.warning.is_some())
            .filter(|meta| meta.width(ui) <= room.width())
            .map(|meta| {
                let mut end = ui.new_child(
                    UiBuilder::new()
                        .max_rect(room)
                        .layout(Layout::right_to_left(Align::Center)),
                );
                meta.show(&mut end)
            });
        TopBarResponse { response, meta }
    }
}

/// What a [`TopBar`] did this frame.
pub struct TopBarResponse {
    /// The band.
    pub response: Response,
    /// The live meta, unless the window is narrow or the meta did not fit.
    pub meta: Option<Response>,
}

/// The search pill of a top bar (`.inp.lg[role=search]`): a 38 point pill
/// search for names, `label=value` selectors or `/regex/`, ending in the
/// key that focuses it (`/`) or, while typing, leaves it (`esc`).
///
/// `/` focuses it from anywhere while nothing else has focus. Disabled (no
/// cluster) it keeps its placeholder on `control-fill-disabled`.
#[must_use = "call `show` to add the search pill"]
pub struct SearchPill<'a> {
    query: &'a mut String,
    regex: Option<&'a mut bool>,
    status: SearchStatus,
    request_focus: bool,
    id_salt: egui::Id,
    placeholder: String,
    accessibility_label: String,
    width: f32,
    focus_key: bool,
    key_when_disabled: bool,
    forced_state: Option<ControlState>,
}

impl<'a> SearchPill<'a> {
    /// A search pill editing `query`.
    pub fn new(query: &'a mut String) -> Self {
        Self {
            query,
            regex: None,
            status: SearchStatus::Idle,
            request_focus: false,
            id_salt: egui::Id::new("search-pill"),
            placeholder: "Find a pod, label or /regex/".to_owned(),
            accessibility_label: "Search".to_owned(),
            width: size::SEARCH_PILL,
            focus_key: true,
            key_when_disabled: false,
            forced_state: None,
        }
    }

    /// Show the regex toggle (`.*`) before the key, reading and writing
    /// `regex`; Alt+R flips it while typing.
    pub fn regex(mut self, regex: &'a mut bool) -> Self {
        self.regex = Some(regex);
        self
    }

    /// What the search found, or why it cannot run (an invalid regex).
    pub fn status(mut self, status: SearchStatus) -> Self {
        self.status = status;
        self
    }

    /// Focus the pill this frame (another shortcut than `/`, such as Ctrl+F).
    pub fn request_focus(mut self, focus: bool) -> Self {
        self.request_focus = focus;
        self
    }

    /// A stable ID salt for the pill's state.
    pub fn id_salt(mut self, id_salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        self.id_salt = egui::Id::new(id_salt);
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn accessibility_label(mut self, label: impl Into<String>) -> Self {
        self.accessibility_label = label.into();
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Let `/` focus the pill (on by default; off when two pills share a
    /// window).
    pub fn focus_key(mut self, focus_key: bool) -> Self {
        self.focus_key = focus_key;
        self
    }

    /// Keep the `/` key on the pill while it is disabled (the idle top bar of
    /// a window with no cluster).
    pub fn key_when_disabled(mut self) -> Self {
        self.key_when_disabled = true;
        self
    }

    /// Paint `state` whatever the pointer and keyboard do (design boards).
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    /// Add the pill.
    pub fn show(self, ui: &mut Ui) -> SearchFieldResponse {
        let focus = self.request_focus
            || self.focus_key
                && ui.is_enabled()
                && ui.memory(|memory| memory.focused().is_none())
                && ui.input_mut(|input| consume_key_with_text(input, Modifiers::NONE, Key::Slash));
        let mut field = SearchField::pill(self.query)
            .width(self.width)
            .id_salt(self.id_salt)
            .placeholder(self.placeholder)
            .accessibility_label(self.accessibility_label)
            .request_focus(focus);
        // The keys show only on a pill that `/` focuses.
        if self.focus_key {
            field = field.key_hint("/", "esc");
            if self.key_when_disabled {
                field = field.key_hint_when_disabled();
            }
        }
        if let Some(regex) = self.regex {
            field = field.regex(regex).regex_on_demand();
        }
        field = field.status(self.status);
        if let Some(state) = self.forced_state {
            field = field.force_state(state);
        }
        field.show(ui)
    }
}

/// What a [`ScopePill`] shows.
#[derive(Clone, Debug, PartialEq)]
enum Scope {
    /// One or several namespaces, led by the watch lamp (lit while the
    /// watch is healthy) when its state is known.
    Namespaces {
        names: Vec<String>,
        watching: Option<bool>,
    },
    /// Every namespace, with how many there are when known.
    AllNamespaces { count: Option<usize> },
    /// A cluster-scoped kind: no namespace applies.
    ClusterWide,
}

/// The scope pill of a top bar (`.sel.lg`): which namespaces the list
/// shows, opening the namespace combobox (C05). One namespace shows its
/// watch lamp and name, several their names and a `+N`, all namespaces a
/// globe and the count; a cluster-scoped kind shows "Cluster-wide" and
/// cannot be opened.
#[must_use = "call `combobox` and show it"]
pub struct ScopePill {
    scope: Scope,
    id_salt: egui::Id,
    width: f32,
    forced_state: Option<ControlState>,
}

impl ScopePill {
    fn new(scope: Scope) -> Self {
        Self {
            scope,
            id_salt: egui::Id::new("scope-pill"),
            width: size::SCOPE_PILL,
            forced_state: None,
        }
    }

    /// The namespaces `names`, led by the watch lamp: `Some(true)` lit,
    /// `Some(false)` neutral, `None` none.
    pub fn namespaces(
        names: impl IntoIterator<Item = impl Into<String>>,
        watching: Option<bool>,
    ) -> Self {
        Self::new(Scope::Namespaces {
            names: names.into_iter().map(Into::into).collect(),
            watching,
        })
    }

    /// Every namespace; `count` of them when known.
    pub fn all_namespaces(count: Option<usize>) -> Self {
        Self::new(Scope::AllNamespaces { count })
    }

    /// A cluster-scoped kind.
    pub fn cluster_wide() -> Self {
        Self::new(Scope::ClusterWide)
    }

    /// A stable ID salt for the pill's combobox.
    pub fn id_salt(mut self, id_salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        self.id_salt = egui::Id::new(id_salt);
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Paint `state` whatever the pointer and keyboard do (design boards;
    /// a forced focus draws the ring of the open pill).
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    /// The text the pill shows.
    pub fn text(&self) -> String {
        match &self.scope {
            Scope::Namespaces { names, .. } => names
                .iter()
                .take(SCOPE_NAMES)
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", "),
            Scope::AllNamespaces { count: Some(count) } => {
                format!("All namespaces · {}", format_count(*count))
            }
            Scope::AllNamespaces { count: None } => "All namespaces".to_owned(),
            Scope::ClusterWide => "Cluster-wide".to_owned(),
        }
    }

    /// The namespace combobox behind the pill, its trigger configured as
    /// the pill; give it its namespaces with `filter_by` and `show_items`.
    pub fn combobox(self) -> Combobox<NoFilter> {
        let text = self.text();
        let mut combobox = Combobox::new(self.id_salt)
            .accessibility_label("Namespace scope")
            .noun("namespaces")
            .size(FieldSize::Lg)
            .width(self.width)
            .selected_text(text);
        combobox = match self.scope {
            Scope::Namespaces { names, watching } => combobox
                .selected_status(watching)
                .selected_more(names.len().saturating_sub(SCOPE_NAMES))
                .multi_select(),
            Scope::AllNamespaces { .. } => {
                combobox.selected_value_icon(Icon::GlobeAlt).multi_select()
            }
            Scope::ClusterWide => combobox.selected_value_icon(Icon::GlobeAlt).read_only(),
        };
        if let Some(state) = self.forced_state {
            combobox = combobox.force_state(state);
        }
        combobox
    }
}

/// The filter chips of a top bar (`.tgl-solo`): standalone toggles that
/// light up cream while on.
pub struct FilterChip;

impl FilterChip {
    /// The grouping chip: "Group: namespace" (on, inserting group headers,
    /// C17) while grouped `by` something, "Group: none" otherwise.
    pub fn grouping(by: Option<&str>) -> ToggleButton {
        match by {
            Some(by) => ToggleButton::text(format!("Group: {by}"), true),
            None => ToggleButton::text("Group: none", false),
        }
    }

    /// The problems chip: "Problems only N" with the danger lamp, hiding
    /// healthy rows while `on`. The count stays live.
    pub fn problems(count: usize, on: bool) -> ToggleButton {
        ToggleButton::text(format!("Problems only {count}"), on)
            .leading_dot(Tone::Danger)
            .tooltip(if on {
                "Show every row (P)"
            } else {
                "Show only rows with problems (P)"
            })
    }
}

/// A line of the live meta: its galley and colour.
type MetaLine = (Arc<Galley>, egui::Color32);

/// The laid-out parts of a [`LiveMeta`].
struct MetaParts<'k> {
    words: Vec<MetaLine>,
    keys: Vec<Keycap<'k>>,
    tail: Option<MetaLine>,
    width: f32,
}

/// The live meta at the end of a top bar (`.tmeta`): "12 pods · live" while
/// the watch is healthy, a warning such as "reconnecting" in `warning-text`
/// in place of "live" otherwise, then the `[` `]` keys that step through
/// problem rows.
#[must_use = "call `show` to add the live meta"]
pub struct LiveMeta<'a> {
    count: &'a str,
    live: bool,
    warning: Option<&'a str>,
    problem_keys: bool,
}

impl<'a> LiveMeta<'a> {
    /// The meta for `count` ("12 pods").
    pub fn new(count: &'a str) -> Self {
        Self {
            count,
            live: true,
            warning: None,
            problem_keys: true,
        }
    }

    /// Whether the list is live (on by default); a list that is not yet
    /// (or no longer) watched shows only its count.
    pub fn live(mut self, live: bool) -> Self {
        self.live = live;
        self
    }

    /// Say `warning` ("namespaces unavailable") in `warning-text` in place
    /// of "live".
    pub fn warning(mut self, warning: Option<&'a str>) -> Self {
        self.warning = warning;
        self
    }

    /// The watch is reconnecting.
    pub fn reconnecting(self, reconnecting: bool) -> Self {
        self.warning(reconnecting.then_some("reconnecting"))
    }

    /// End with the `[` `]` keys that step through problem rows (on by
    /// default; off for a list that cannot step through its problems).
    pub fn problem_keys(mut self, problem_keys: bool) -> Self {
        self.problem_keys = problem_keys;
        self
    }

    /// The meta's words, each run of one colour as one label: the count,
    /// then the warning or "live", `·` between them and before the keys.
    fn words(&self, theme: &crate::design::theme::Theme) -> Vec<(String, egui::Color32)> {
        let status = match (self.warning, self.live) {
            (Some(warning), _) => Some((warning, theme.warning.text)),
            (None, true) => Some(("live", theme.text.muted)),
            (None, false) => None,
        };
        let mut words = vec![(self.count.to_owned(), theme.text.muted)];
        if let Some((status, color)) = status {
            let last = words.last_mut().expect("the count is a word");
            last.0.push_str(" ·");
            if last.1 == color {
                last.0.push(' ');
                last.0.push_str(status);
            } else {
                words.push((status.to_owned(), color));
            }
        }
        if self.problem_keys {
            words
                .last_mut()
                .expect("the count is a word")
                .0
                .push_str(" ·");
        }
        words
    }

    /// The meta's runs of text (each laid out on one line), its keys and
    /// its tail ("next problem").
    fn parts(&self, ui: &Ui) -> MetaParts<'static> {
        let theme = ui.tokens();
        let keys = if self.problem_keys {
            vec![Keycap::new("["), Keycap::new("]")]
        } else {
            Vec::new()
        };
        // Each run on one line, measured and then drawn from the same galley
        // so it never wraps in the room it was measured for.
        let line = |(text, color): (String, egui::Color32)| {
            (
                type_scale::META_ROW.galley(ui.ctx(), &text, color, f32::INFINITY),
                color,
            )
        };
        let words: Vec<_> = self.words(&theme).into_iter().map(line).collect();
        let tail = self
            .problem_keys
            .then(|| line(("next problem".to_owned(), theme.text.muted)));
        let widths: Vec<f32> = words
            .iter()
            .map(|(galley, _)| galley.rect.width())
            .chain(keys.iter().map(|key| key.size(ui).x))
            .chain(tail.iter().map(|(galley, _)| galley.rect.width()))
            .collect();
        let width = widths.iter().sum::<f32>() + META_GAP * (widths.len() - 1) as f32;
        MetaParts {
            words,
            keys,
            tail,
            width,
        }
    }

    /// How wide the meta is.
    pub fn width(&self, ui: &Ui) -> f32 {
        self.parts(ui).width
    }

    /// Add the meta as one row.
    pub fn show(self, ui: &mut Ui) -> Response {
        let MetaParts {
            words,
            keys,
            tail,
            width,
        } = self.parts(ui);
        let (rect, response) = ui.allocate_exact_size(
            vec2(width, type_scale::META_ROW.line_height()),
            Sense::hover(),
        );
        let mut row = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        row.spacing_mut().item_spacing = vec2(META_GAP, 0.0);
        let add_line = |row: &mut Ui, (galley, color): (Arc<Galley>, egui::Color32)| {
            let (rect, response) =
                type_scale::allocate_text(row, galley.rect.size(), galley.text());
            type_scale::paint_text(row, &response, rect.min, galley, color);
        };
        for word in words {
            add_line(&mut row, word);
        }
        for key in keys {
            key.show(&mut row);
        }
        if let Some(tail) = tail {
            add_line(&mut row, tail);
        }
        response
    }
}
