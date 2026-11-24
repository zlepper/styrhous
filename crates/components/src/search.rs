//! The search field shared by every surface that searches (C04 `SearchField`):
//! the resource table filter, the log viewer and the YAML editor.

use egui::{Id, Key, Modifiers, Rect, Response, Sense, Ui, Vec2, pos2, vec2};

use crate::design::theme::ThemeExt;
use crate::design::type_scale::{self, TextRole};
use crate::design::{radius, size, space};
use crate::field::{self, EditorSpec, FieldSize, FieldState};
use crate::icons::Icon;
use crate::interaction::{ControlState, consume_key_with_text};
use crate::keycap::Keycap;
use crate::mini_button::MiniButton;

/// Default width of a search field (S04: 320, at least 200).
pub const SEARCH_FIELD_WIDTH: f32 = 320.0;
/// The match count (`.cnt`): Plex Mono 11 px in the field's line box.
const COUNT_ROLE: TextRole = type_scale::MONO.with_size(11.0, 20.0).tabular();
// C16
/// The match count of a pill search (`.inp-c.mono.t-meta`): Plex Mono 12/16.
const PILL_COUNT_ROLE: TextRole = type_scale::MONO.with_size(12.0, 16.0);
/// Gap between the items of a search field (`gap: 6px`).
const GAP: f32 = space::S6;
/// Right padding of a search field (`padding-right: 4px`): the mini
/// buttons sit 4 points from its end.
const PADDING_RIGHT: f32 = space::S4;
/// Counts up to this are shown exactly; larger ones abbreviated (12.4k).
const EXACT_COUNT_LIMIT: usize = 9_999;

/// What a search has found so far: the field's count and whether its match
/// navigation can move.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SearchStatus {
    /// No search (an empty query): no count.
    #[default]
    Idle,
    /// Still scanning, `found` matches so far ("Searching… 4").
    Searching { found: usize },
    /// A finished search: "3 / 12" with an active match, "12 results"
    /// without one, "0 results" when nothing matched (not an error).
    Matches { active: Option<usize>, total: usize },
    /// A table filter showing `shown` of `total` rows ("1 of 8").
    Filtered { shown: usize, total: usize },
    // C18
    /// A count of what can be searched, in words ("4 contexts"), set in
    /// the field's meta text rather than mono.
    Label(String),
    /// The query is not a valid regular expression: the danger ring and no
    /// count. Show the message under the field ([`crate::FieldMessage`]).
    Invalid(String),
}

impl SearchStatus {
    /// The count text, if the status shows one.
    pub fn count_text(&self) -> Option<String> {
        match self {
            Self::Idle | Self::Invalid(_) => None,
            Self::Label(label) => Some(label.clone()),
            Self::Searching { found } => Some(format!("Searching… {}", format_count(*found))),
            Self::Matches {
                active: Some(active),
                total,
            } => Some(format!(
                "{} / {}",
                format_count(active + 1),
                format_count(*total)
            )),
            Self::Matches {
                active: None,
                total,
            } => Some(format!(
                "{} {}",
                format_count(*total),
                if *total == 1 { "result" } else { "results" }
            )),
            Self::Filtered { shown, total } => Some(format!(
                "{} of {}",
                format_count(*shown),
                format_count(*total)
            )),
        }
    }

    /// Whether there are matches to move between.
    fn can_navigate(&self) -> bool {
        match self {
            Self::Searching { found } => *found > 0,
            Self::Matches { total, .. } => *total > 0,
            Self::Idle | Self::Filtered { .. } | Self::Label(_) | Self::Invalid(_) => false,
        }
    }

    /// Whether the query can be cleared: not while the search is still
    /// running or the query is invalid (the field keeps its focus to fix it).
    fn is_settled(&self) -> bool {
        !matches!(self, Self::Searching { .. } | Self::Invalid(_))
    }
}

// C17
/// `count` of something as S04 formats it, with its `singular` or
/// `plural` noun: "1 pod", "642 pods", "12.4k pods".
pub fn counted(count: usize, singular: &str, plural: &str) -> String {
    let noun = if count == 1 { singular } else { plural };
    format!("{} {noun}", format_count(count))
}

/// A count in full with its thousands grouped: 18,400.
pub fn format_exact_count(count: usize) -> String {
    let digits = count.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// A count as S04 formats it: exact up to 9,999, then abbreviated (12.4k).
pub fn format_count(count: usize) -> String {
    if count <= EXACT_COUNT_LIMIT {
        format_exact_count(count)
    } else {
        format_compact_count(count)
    }
}

/// A count abbreviated from 1,000 on: 999, 2.1k, 1.2M.
pub fn format_compact_count(count: usize) -> String {
    if count < 1_000 {
        count.to_string()
    } else if count < 999_950 {
        format!("{:.1}k", count as f64 / 1_000.0)
    } else {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    }
}

/// What a [`SearchField`] did this frame.
pub struct SearchFieldResponse {
    /// The query editor.
    pub text: Response,
    /// The query or the regex toggle changed (typing, clearing, Alt+R).
    pub changed: bool,
    /// The regex toggle was flipped.
    pub regex_toggled: bool,
    /// Move to the previous match (↑ button, Shift+Enter).
    pub previous: bool,
    /// Move to the next match (↓ button, Enter).
    pub next: bool,
}

/// How a [`SearchField`] lays out its parts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchFieldStyle {
    /// The C04 search field: a 16 point magnifier, items 6 points apart,
    /// the mini buttons 4 points from the end after a divider.
    #[default]
    Field,
    /// A pill (C16 search pill, C20 floating toolbar): a 14 point
    /// magnifier, items 8 points apart, the field's padding on both sides
    /// and no divider; the count in Plex Mono 12/16.
    Pill,
}

/// The geometry of a [`SearchFieldStyle`].
struct StyleMetrics {
    icon: f32,
    gap: f32,
    padding_right: f32,
    divider: bool,
    count_role: TextRole,
}

impl SearchFieldStyle {
    fn metrics(self, size: FieldSize) -> StyleMetrics {
        match self {
            Self::Field => StyleMetrics {
                icon: size::ICON,
                gap: GAP,
                padding_right: PADDING_RIGHT,
                divider: true,
                count_role: COUNT_ROLE,
            },
            Self::Pill => StyleMetrics {
                icon: size::ICON_SM,
                gap: space::S8,
                padding_right: size.padding_x(),
                divider: false,
                count_role: PILL_COUNT_ROLE,
            },
        }
    }
}

/// A search field (`.inp[role=search]`): magnifier, query, count, clear,
/// a divider, previous / next match and the regex toggle (`.*`), so every
/// surface searches the same way.
///
/// Enter and Shift+Enter move to the next and previous match, Alt+R
/// toggles regex, and Esc only leaves the field (the query and its
/// filtering stay). The caller runs the search (debounced as its surface needs)
/// and reports progress with [`Self::status`].
#[must_use = "call `show` to add the search field"]
pub struct SearchField<'a> {
    query: &'a mut String,
    regex: Option<&'a mut bool>,
    regex_on_demand: bool,
    id_salt: Id,
    placeholder: String,
    accessibility_label: String,
    status: SearchStatus,
    navigation: bool,
    width: f32,
    request_focus: bool,
    forced_state: Option<ControlState>,
    style: SearchFieldStyle,
    size: FieldSize,
    height: Option<f32>,
    key_hint: Option<(String, String)>,
    key_hint_when_disabled: bool,
    clear_button: bool,
}

impl<'a> SearchField<'a> {
    /// A search field editing `query`.
    pub fn new(query: &'a mut String) -> Self {
        Self {
            query,
            regex: None,
            regex_on_demand: false,
            id_salt: Id::new("search-field"),
            placeholder: "Search…".to_owned(),
            accessibility_label: "Search".to_owned(),
            status: SearchStatus::Idle,
            navigation: true,
            width: SEARCH_FIELD_WIDTH,
            request_focus: false,
            forced_state: None,
            style: SearchFieldStyle::Field,
            size: FieldSize::Md,
            height: None,
            key_hint: None,
            key_hint_when_disabled: false,
            clear_button: true,
        }
    }

    // C16
    /// A large pill search (`.inp.lg[role=search]`) editing `query`: no
    /// previous / next match buttons.
    pub fn pill(query: &'a mut String) -> Self {
        Self::new(query)
            .style(SearchFieldStyle::Pill)
            .size(FieldSize::Lg)
            .navigation(false)
    }

    /// Show the regex toggle, reading and writing `regex`.
    pub fn regex(mut self, regex: &'a mut bool) -> Self {
        self.regex = Some(regex);
        self
    }

    /// Show the regex toggle only while the field has focus or the pointer
    /// is over it, and while regex matching is on (a field at rest then
    /// shows just its key, as the top bars of the design do).
    pub fn regex_on_demand(mut self) -> Self {
        self.regex_on_demand = true;
        self
    }

    /// A stable ID salt for the field's state.
    pub fn id_salt(mut self, id_salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        self.id_salt = Id::new(id_salt);
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

    /// What the search has found ([`SearchStatus`]).
    pub fn status(mut self, status: SearchStatus) -> Self {
        self.status = status;
        self
    }

    /// Show the previous / next match buttons (on by default; off for table
    /// filters, which hide rows instead).
    pub fn navigation(mut self, navigation: bool) -> Self {
        self.navigation = navigation;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Focus the query in this frame (Ctrl/⌘ F).
    pub fn request_focus(mut self, focus: bool) -> Self {
        self.request_focus = focus;
        self
    }

    /// Paint `state` whatever the pointer and keyboard do (design boards); a
    /// forced focus draws the design's caret after the query.
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    // C16
    /// Lay the parts out in `style` ([`SearchFieldStyle::Field`] by default).
    pub fn style(mut self, style: SearchFieldStyle) -> Self {
        self.style = style;
        self
    }

    /// The field's size ([`FieldSize::Md`] by default).
    pub fn size(mut self, size: FieldSize) -> Self {
        self.size = size;
        self
    }

    /// Make the field `height` points tall instead of its size's height
    /// (the 36 point search of a floating toolbar).
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// End the enabled field with a keycap naming the key that focuses it
    /// (`rest`, `/`) or, while it has focus, leaves it (`focused`, `esc`).
    pub fn key_hint(mut self, rest: impl Into<String>, focused: impl Into<String>) -> Self {
        self.key_hint = Some((rest.into(), focused.into()));
        self
    }

    /// Keep the key hint while the field is disabled (A01's idle top bar).
    pub fn key_hint_when_disabled(mut self) -> Self {
        self.key_hint_when_disabled = true;
        self
    }

    /// Show the clear button while there is a query (on by default).
    pub fn clear_button(mut self, clear_button: bool) -> Self {
        self.clear_button = clear_button;
        self
    }

    /// Add the field.
    pub fn show(self, ui: &mut Ui) -> SearchFieldResponse {
        let Self {
            query,
            regex,
            regex_on_demand,
            id_salt,
            placeholder,
            accessibility_label,
            status,
            navigation,
            width,
            request_focus,
            forced_state,
            style,
            size,
            height,
            key_hint,
            key_hint_when_disabled,
            clear_button,
        } = self;
        let metrics = style.metrics(size);
        let height = height.unwrap_or(size.height());
        let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        let id = ui.make_persistent_id(id_salt);
        let editor_id = id.with("query");
        let enabled = ui.is_enabled();
        let theme = ui.tokens();
        let has_focus = ui.memory(|memory| memory.has_focus(editor_id));
        let regex = regex.filter(|regex| {
            !regex_on_demand || has_focus || **regex || ui.rect_contains_pointer(rect)
        });

        // Keys the editor must not see. Enter moves between matches while
        // there are any; otherwise it leaves the field as in any text edit.
        let (mut previous, mut next, mut regex_toggled, mut cleared) = (false, false, false, false);
        let can_navigate = status.can_navigate();
        if has_focus {
            ui.input_mut(|input| {
                if can_navigate {
                    // Shift first: a key without modifiers also matches it with Shift.
                    previous |= input.consume_key(Modifiers::SHIFT, Key::Enter);
                    next |= input.consume_key(Modifiers::NONE, Key::Enter);
                }
                regex_toggled |=
                    regex.is_some() && consume_key_with_text(input, Modifiers::ALT, Key::R);
            });
        }
        let count = status.count_text();
        let count_role = match status {
            SearchStatus::Label(_) => type_scale::META,
            _ => metrics.count_role,
        };
        let count_galley = count
            .as_deref()
            .map(|count| field::text_galley(ui, count_role, count, f32::INFINITY, true));
        let show_clear = clear_button && !query.is_empty() && status.is_settled();
        let design_caret = forced_state == Some(ControlState::Focused);
        let keycap = key_hint
            .as_ref()
            .filter(|_| enabled || key_hint_when_disabled)
            .map(|(rest, focused)| {
                Keycap::new(if has_focus || design_caret {
                    focused
                } else {
                    rest
                })
            });
        let mini = size::CONTROL_MINI;
        let gap = metrics.gap;
        // The right-hand cluster, laid out from the field's end.
        let mut right = rect.right() - metrics.padding_right;
        let mut slot_left = |width: f32| {
            right -= width;
            let left = right;
            right -= gap;
            left
        };
        let key_slot = keycap.as_ref().map(|keycap| {
            let size = keycap.size(ui);
            (slot_left(size.x), size)
        });
        let regex_left = regex.as_ref().map(|_| slot_left(mini));
        let (next_left, previous_left) = if navigation {
            (Some(slot_left(mini)), Some(slot_left(mini)))
        } else {
            (None, None)
        };
        let divider_left =
            (metrics.divider && (navigation || regex.is_some())).then(|| slot_left(size::HAIRLINE));
        let clear_left = show_clear.then(|| slot_left(mini));
        let count_left = count_galley
            .as_ref()
            .map(|galley| slot_left(galley.rect.width()));
        let text_right = right + gap;

        let well = field::reserve_well(ui);
        let mut margin = size.text_edit_margin();
        margin.top = ((height - size.role().line_height()) / 2.0) as i8;
        margin.bottom = margin.top;
        if design_caret && style == SearchFieldStyle::Pill {
            // The design's caret sits in the query's line box, which it
            // makes taller; the line box stays centred, the text moves down.
            let shift = field::caret_line_shift(size.role(), ui.pixels_per_point()) as i8;
            margin.top += shift;
            margin.bottom -= shift;
        }
        let icon_left = rect.left() + margin.leftf();
        margin.left += (metrics.icon + gap) as i8;
        // With an empty query the design's caret stands where the text starts
        // and the placeholder follows it.
        let caret_before_placeholder =
            design_caret && style == SearchFieldStyle::Pill && query.is_empty();
        let text_left = rect.left() + margin.leftf();
        if caret_before_placeholder {
            margin.left += size::CARET.x as i8;
        }
        // The editor stops a gap before the right-hand cluster, so the two
        // never overlap.
        margin.right = 0;
        let editor_rect = Rect::from_min_max(rect.min, pos2(text_right - gap, rect.max.y));
        let invalid = matches!(status, SearchStatus::Invalid(_));
        let spec = EditorSpec {
            id: editor_id,
            role: size.role(),
            color: field::value_color(&theme, enabled),
            placeholder: Some(&placeholder),
            placeholder_color: theme.text.muted,
            accessibility_label: &accessibility_label,
            enabled,
            multiline: false,
            margin,
        };
        // A disabled field keeps its placeholder and icon at full opacity
        // (`.inp.is-disabled { opacity: 1 }`).
        let full_opacity = crate::control::disabled_fade_compensation(ui);
        let text = crate::control::with_opacity(ui, full_opacity, |ui| {
            field::show_editor(ui, editor_rect, query, &spec)
        });
        if request_focus {
            text.request_focus();
        }

        let center_y = rect.center().y;
        let mini_rect =
            |left: f32| Rect::from_min_size(pos2(left, center_y - mini / 2.0), Vec2::splat(mini));
        if let (Some(left), Some(galley)) = (count_left, count_galley) {
            let pos = pos2(left, center_y - galley.rect.height() / 2.0);
            let count_rect = Rect::from_min_size(pos, galley.rect.size());
            let response =
                type_scale::interact_text(ui, count_rect, id.with("count"), galley.text());
            type_scale::paint_text(ui, &response, pos, galley, theme.text.muted);
        }
        if let Some(left) = clear_left
            && MiniButton::icon(Icon::XMark, "Clear search")
                .show(ui, mini_rect(left), id.with("clear"))
                .clicked()
        {
            query.clear();
            cleared = true;
            ui.memory_mut(|memory| memory.request_focus(editor_id));
        }
        if let Some(left) = divider_left {
            let divider = Rect::from_center_size(
                pos2(left + size::HAIRLINE / 2.0, center_y),
                vec2(size::HAIRLINE, size::INLINE_DIVIDER),
            );
            ui.painter().rect_filled(divider, 0, theme.divider.strong);
        }
        if let Some(left) = previous_left {
            previous |= MiniButton::icon(Icon::ChevronUp, "Previous match")
                .enabled(can_navigate)
                .show(ui, mini_rect(left), id.with("previous"))
                .clicked();
        }
        if let Some(left) = next_left {
            next |= MiniButton::icon(Icon::ChevronDown, "Next match")
                .enabled(can_navigate)
                .show(ui, mini_rect(left), id.with("next"))
                .clicked();
        }
        if let (Some(left), Some(regex)) = (regex_left, regex) {
            regex_toggled |= MiniButton::regex(*regex)
                .show(ui, mini_rect(left), id.with("regex"))
                .clicked();
            if regex_toggled {
                *regex = !*regex;
            }
        }
        if let (Some((left, key_size)), Some(keycap)) = (key_slot, &keycap) {
            let key_rect = Rect::from_min_size(pos2(left, center_y - key_size.y / 2.0), key_size);
            keycap.paint(ui, key_rect);
        }
        let next = next && can_navigate;
        let previous = previous && can_navigate;

        let mut interaction = field::field_interaction(ui, &text, editor_id, forced_state);
        // The whole field lightens on hover, its buttons included.
        interaction.hovered |= forced_state.is_none() && ui.rect_contains_pointer(rect);
        let state = FieldState::new(interaction, invalid, enabled);
        field::paint_well(ui, well, rect, radius::FULL.into(), state);
        let icon = Rect::from_min_size(
            pos2(icon_left, center_y - metrics.icon / 2.0),
            Vec2::splat(metrics.icon),
        );
        crate::control::with_opacity(ui, full_opacity, |ui| {
            Icon::MagnifyingGlass.paint(ui, icon, theme.text.muted);
        });
        if design_caret {
            let row = crate::text_input::text_row(rect, margin);
            let width = field::text_galley(ui, size.role(), query, f32::INFINITY, true)
                .rect
                .width();
            match style {
                SearchFieldStyle::Field => field::paint_caret(ui, row, row.left() + width),
                SearchFieldStyle::Pill => {
                    let end = if caret_before_placeholder {
                        text_left - 1.0
                    } else {
                        row.left() + width
                    };
                    field::paint_baseline_caret(ui, size.role(), row, end);
                }
            }
        }

        let changed = text.changed() || cleared || regex_toggled;
        SearchFieldResponse {
            text,
            changed,
            regex_toggled,
            previous,
            next,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::{UiHarnessSnapshot, setup_egui};
    use egui::accesskit::Toggled;
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// What a search field reported, accumulated over frames.
    #[derive(Default)]
    struct Events {
        query: String,
        regex: bool,
        status: SearchStatus,
        next: usize,
        previous: usize,
        changed: usize,
    }

    fn search_harness(events: Rc<RefCell<Events>>) -> Harness<'static> {
        let mut harness = Harness::builder()
            .with_size(egui::vec2(360.0, 48.0))
            .build_ui(move |ui| {
                let mut events = events.borrow_mut();
                let Events {
                    query,
                    regex,
                    status,
                    ..
                } = &mut *events;
                let response = SearchField::new(query)
                    .regex(regex)
                    .accessibility_label("Search logs")
                    .placeholder("Search logs…")
                    .status(status.clone())
                    .show(ui);
                events.next += usize::from(response.next);
                events.previous += usize::from(response.previous);
                events.changed += usize::from(response.changed);
            });
        setup_egui(&mut harness);
        harness.run();
        harness
    }

    #[test]
    fn enter_moves_between_matches_and_escape_only_leaves_the_field() {
        let events = Rc::new(RefCell::new(Events {
            query: "http".to_owned(),
            status: SearchStatus::Matches {
                active: Some(0),
                total: 12,
            },
            ..Default::default()
        }));
        let mut harness = search_harness(events.clone());
        harness.get_by_label("Search logs").focus();
        harness.run();

        harness.key_press(egui::Key::Enter);
        harness.run();
        harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Enter);
        harness.run();
        assert_eq!((events.borrow().next, events.borrow().previous), (1, 1));
        assert!(harness.get_by_label("Search logs").is_focused());

        // Esc only leaves the field: the query stays, and so does its filtering.
        harness.key_press(egui::Key::Escape);
        harness.run();
        harness.run();
        assert_eq!(events.borrow().query, "http");
        assert!(!harness.get_by_label("Search logs").is_focused());
    }

    /// A real key press: the platform sends the key and the text it types
    /// in one frame (`Harness::event` would run a frame per event).
    fn type_key(
        harness: &mut Harness<'static>,
        modifiers: egui::Modifiers,
        key: egui::Key,
        text: &str,
    ) {
        let events = &mut harness.input_mut().events;
        events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        });
        events.push(egui::Event::Text(text.to_owned()));
        harness.run();
    }

    #[test]
    fn alt_r_toggles_regex_without_typing_into_the_query() {
        let events = Rc::new(RefCell::new(Events {
            query: "foo".to_owned(),
            ..Default::default()
        }));
        let mut harness = search_harness(events.clone());
        harness.get_by_label("Search logs").focus();
        harness.run();
        type_key(&mut harness, egui::Modifiers::ALT, egui::Key::R, "r");
        assert!(events.borrow().regex);
        assert_eq!(events.borrow().query, "foo");
    }

    #[test]
    fn enter_leaves_the_field_when_there_is_nothing_to_move_between() {
        let events = Rc::new(RefCell::new(Events {
            query: "timeout".to_owned(),
            status: SearchStatus::Matches {
                active: None,
                total: 0,
            },
            ..Default::default()
        }));
        let mut harness = search_harness(events.clone());
        harness.get_by_label("Search logs").focus();
        harness.run();
        harness.key_press(egui::Key::Enter);
        harness.run();
        assert_eq!(events.borrow().next, 0);
        assert!(!harness.get_by_label("Search logs").is_focused());
    }

    #[test]
    fn match_buttons_act_only_when_there_are_matches() {
        let events = Rc::new(RefCell::new(Events {
            query: "timeout".to_owned(),
            status: SearchStatus::Matches {
                active: None,
                total: 0,
            },
            ..Default::default()
        }));
        let mut harness = search_harness(events.clone());
        assert!(
            harness
                .get_by_label("Next match")
                .accesskit_node()
                .is_disabled()
        );
        harness.get_by_label("Next match").click();
        harness.run();
        assert_eq!(events.borrow().next, 0);

        events.borrow_mut().status = SearchStatus::Matches {
            active: Some(2),
            total: 12,
        };
        harness.run();
        harness.get_by_label("Next match").click();
        harness.run();
        harness.get_by_label("Previous match").click();
        harness.run();
        assert_eq!((events.borrow().next, events.borrow().previous), (1, 1));
    }

    #[test]
    fn an_on_demand_regex_toggle_shows_while_focused_hovered_or_on() {
        let regex = Rc::new(std::cell::Cell::new(false));
        let regex_ui = regex.clone();
        let mut query = String::new();
        let mut harness = Harness::new_ui(move |ui| {
            let mut on = regex_ui.get();
            SearchField::pill(&mut query)
                .width(300.0)
                .accessibility_label("Search pods")
                .regex(&mut on)
                .regex_on_demand()
                .show(ui);
            regex_ui.set(on);
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();
        assert!(
            harness.query_by_label("Regular expression").is_none(),
            "a field at rest shows no toggle"
        );

        harness.get_by_label("Search pods").focus();
        harness.run();
        harness.get_by_label("Regular expression").click();
        harness.run();
        assert!(regex.get(), "the toggle works once it is there");

        harness.event(egui::Event::PointerGone);
        harness.run();
        harness.run();
        harness.get_by_label("Regular expression");
        assert_eq!(
            harness
                .get_by_label("Regular expression")
                .accesskit_node()
                .toggled(),
            Some(Toggled::True),
            "regex matching stays visible while on"
        );
    }

    #[test]
    fn the_regex_toggle_and_alt_r_flip_regex_and_clear_empties_the_query() {
        let events = Rc::new(RefCell::new(Events {
            query: "GET /v1/.*".to_owned(),
            status: SearchStatus::Matches {
                active: Some(4),
                total: 5,
            },
            ..Default::default()
        }));
        let mut harness = search_harness(events.clone());
        harness.get_by_label("Regular expression").click();
        harness.run();
        assert!(events.borrow().regex);
        assert_eq!(
            harness
                .get_by_label("Regular expression")
                .accesskit_node()
                .toggled(),
            Some(Toggled::True)
        );

        harness.get_by_label("Search logs").focus();
        harness.run();
        harness.key_press_modifiers(egui::Modifiers::ALT, egui::Key::R);
        harness.run();
        assert!(!events.borrow().regex);

        harness.get_by_label("Clear search").click();
        harness.run();
        assert_eq!(events.borrow().query, "");
        assert_eq!(events.borrow().changed, 3, "regex click, Alt+R and clear");
        harness.run();
        assert!(harness.query_by_label("Clear search").is_none());
    }

    /// A pill with text shows the clear button (Esc no longer clears); a click
    /// empties the query and keeps the focus in the field.
    #[test]
    fn the_pill_has_a_clear_button_that_keeps_the_focus() {
        let query = Rc::new(RefCell::new("pod".to_owned()));
        let shared = Rc::clone(&query);
        let mut harness = Harness::builder()
            .with_size(egui::vec2(360.0, 48.0))
            .build_ui(move |ui| {
                SearchField::pill(&mut shared.borrow_mut())
                    .accessibility_label("Search pods")
                    .show(ui);
            });
        setup_egui(&mut harness);
        harness.run();
        harness.get_by_label("Search pods").focus();
        harness.run();

        harness.get_by_label("Clear search").click();
        harness.run();
        harness.run();
        assert_eq!(*query.borrow(), "");
        assert!(harness.get_by_label("Search pods").is_focused());
        assert!(harness.query_by_label("Clear search").is_none());
    }

    #[test]
    fn snapshots_the_search_field_states() {
        let mut harness = Harness::builder()
            .with_size(egui::vec2(360.0, 260.0))
            .build_ui(|ui| {
                ui.spacing_mut().item_spacing.y = space::S12;
                let states = [
                    ("", false, SearchStatus::Idle),
                    ("http", false, SearchStatus::Searching { found: 4 }),
                    (
                        "http",
                        false,
                        SearchStatus::Matches {
                            active: Some(2),
                            total: 12,
                        },
                    ),
                    ("pod", false, SearchStatus::Filtered { shown: 1, total: 8 }),
                    (
                        "coredns[a-z",
                        true,
                        SearchStatus::Invalid("Unclosed character class".to_owned()),
                    ),
                ];
                for (index, (query, regex, status)) in states.into_iter().enumerate() {
                    let mut query = query.to_owned();
                    let mut regex = regex;
                    SearchField::new(&mut query)
                        .regex(&mut regex)
                        .id_salt(index)
                        .accessibility_label(format!("Search {index}"))
                        .navigation(!matches!(status, SearchStatus::Filtered { .. }))
                        .status(status)
                        .show(ui);
                }
                // Room for the invalid field's halo.
                ui.add_space(size::FIELD_HALO);
            });
        setup_egui(&mut harness);
        harness.run();
        harness.ui_harness("search/search_field_states");
    }

    #[test]
    fn counts_are_exact_up_to_four_digits_then_abbreviated() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1_284), "1,284");
        assert_eq!(format_count(9_999), "9,999");
        assert_eq!(format_count(12_400), "12.4k");
        assert_eq!(format_count(999_949), "999.9k");
        assert_eq!(format_count(999_950), "1.0M");
        assert_eq!(format_count(2_500_000), "2.5M");
        assert_eq!(format_compact_count(999), "999");
        assert_eq!(format_compact_count(2_100), "2.1k");
        assert_eq!(format_compact_count(999_950), "1.0M");
    }

    #[test]
    fn statuses_format_their_counts() {
        let count = |status: SearchStatus| status.count_text();
        assert_eq!(count(SearchStatus::Idle), None);
        assert_eq!(
            count(SearchStatus::Searching { found: 4 }).as_deref(),
            Some("Searching… 4")
        );
        assert_eq!(
            count(SearchStatus::Matches {
                active: Some(2),
                total: 12
            })
            .as_deref(),
            Some("3 / 12")
        );
        assert_eq!(
            count(SearchStatus::Matches {
                active: None,
                total: 0
            })
            .as_deref(),
            Some("0 results")
        );
        assert_eq!(
            count(SearchStatus::Filtered { shown: 1, total: 8 }).as_deref(),
            Some("1 of 8")
        );
        assert_eq!(count(SearchStatus::Invalid("bad".into())), None);
    }
}
