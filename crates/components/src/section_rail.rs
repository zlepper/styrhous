//! The section rail of an inspector (C19 `.srail`): its sections with
//! their counts, the current one lit, so long resources stay navigable.

use egui::{CornerRadius, Id, Rect, Response, Sense, Ui, Vec2, pos2, vec2};

use crate::PointingHand;
use crate::design::theme::ThemeExt;
use crate::design::type_scale::{self, TextRole, Weight};
use crate::design::{radius, size, space};
use crate::interaction::{ControlState, Interaction, step_index, vertical_arrow_step};

/// A section's name (`.si`): UI 13/20, 600 when current.
const ITEM_ROLE: TextRole = type_scale::UI;
/// A section's count (`.si .c`): Plex Mono 10.5 in the item's line box.
const COUNT_ROLE: TextRole = type_scale::MONO_COUNT;
/// Gap between a section's problem count and "/ count".
const ALERT_GAP: f32 = space::S4;
/// The caption above the sections ("On this pod"), with its bottom padding.
const CAPTION_PADDING_BOTTOM: f32 = space::S8;
/// Gap between the items (`.srail` gap 2).
const ITEM_GAP: f32 = space::S2;

/// One section of a [`SectionRail`]: its name and, when it lists things,
/// how many, and how many of them are a problem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Section<'a> {
    pub name: &'a str,
    pub count: Option<usize>,
    /// How many of the `count` are problems; shown in `danger-text` before
    /// "/ count" while above zero ("2 / 37").
    pub alerts: usize,
}

impl<'a> Section<'a> {
    pub const fn new(name: &'a str) -> Self {
        Self {
            name,
            count: None,
            alerts: 0,
        }
    }

    /// How many of the section's items are problems.
    pub const fn alerts(mut self, alerts: usize) -> Self {
        self.alerts = alerts;
        self
    }

    pub const fn count(mut self, count: usize) -> Self {
        self.count = Some(count);
        self
    }
}

/// A [`size::SECTION_RAIL`] wide rail on `surface-sunken` listing an
/// inspector's sections under a caption ("On this pod"). The current
/// section is `item-selected` in `accent-text` at 600. Clicking a section
/// (or ↑ ↓ while one has focus) selects it: scroll the body there; set the
/// current section from the scroll position. Inspectors with fewer than
/// four sections skip the rail (S19).
#[must_use = "call `show` to add the rail"]
pub struct SectionRail<'s, 'a> {
    caption: &'a str,
    sections: &'s [Section<'a>],
    current: &'s mut usize,
    corner_radius: CornerRadius,
    forced_hover: Option<usize>,
}

impl<'s, 'a> SectionRail<'s, 'a> {
    pub fn new(caption: &'a str, sections: &'s [Section<'a>], current: &'s mut usize) -> Self {
        Self {
            caption,
            sections,
            current,
            corner_radius: CornerRadius::ZERO,
            forced_hover: None,
        }
    }

    /// Round the rail's corners (a rail standing on its own; in a blade the
    /// blade clips it).
    pub fn corner_radius(mut self, corner_radius: impl Into<CornerRadius>) -> Self {
        self.corner_radius = corner_radius.into();
        self
    }

    /// Paint section `index` hovered whatever the pointer does (boards).
    pub fn force_hover(mut self, index: usize) -> Self {
        self.forced_hover = Some(index);
        self
    }

    /// Add the rail, `height` points tall. Returns the section selected
    /// this frame, if any.
    pub fn show(self, ui: &mut Ui, height: f32) -> Option<usize> {
        let theme = ui.tokens();
        let (rect, response) =
            ui.allocate_exact_size(vec2(size::SECTION_RAIL, height), Sense::hover());
        if ui.is_rect_visible(rect) {
            ui.painter()
                .rect_filled(rect, self.corner_radius, theme.surface.sunken);
        }
        let content = rect.shrink2(size::SECTION_RAIL_PADDING);
        let caption_pos = pos2(content.left() + size::SECTION_ITEM_PADDING_X, content.top());
        let caption = type_scale::CAPTION.galley_elided(
            ui.ctx(),
            self.caption,
            theme.text.muted,
            content.width() - 2.0 * size::SECTION_ITEM_PADDING_X,
            1,
        );
        type_scale::text_at(
            ui,
            response.id.with("caption"),
            caption_pos,
            caption,
            self.caption,
            theme.text.muted,
        );

        let list_top =
            content.top() + type_scale::CAPTION.line_height() + CAPTION_PADDING_BOTTOM + ITEM_GAP;
        let ids: Vec<Id> = (0..self.sections.len())
            .map(|index| response.id.with(("section", index)))
            .collect();
        let mut selected = None;
        let mut focused = None;
        // The sections scroll when there are more than the rail has room for.
        let list = Rect::from_min_max(pos2(content.left(), list_top), content.max);
        let mut list_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("section-list")
                .max_rect(list),
        );
        list_ui.set_clip_rect(list.intersect(ui.clip_rect()));
        crate::scroll::vertical()
            .id_salt(response.id.with("list-scroll"))
            .auto_shrink([false, false])
            .show(&mut list_ui, |list_ui| {
                let mut top = list_ui.cursor().top();
                list_ui.allocate_space(vec2(
                    content.width(),
                    self.sections.len() as f32 * (size::SECTION_ITEM + ITEM_GAP),
                ));
                for (index, section) in self.sections.iter().enumerate() {
                    let item = Rect::from_min_size(
                        pos2(content.left(), top),
                        vec2(content.width(), size::SECTION_ITEM),
                    );
                    top += size::SECTION_ITEM + ITEM_GAP;
                    let forced =
                        (self.forced_hover == Some(index)).then_some(ControlState::Hovered);
                    let response = self.item(list_ui, item, ids[index], index, section, forced);
                    if response.clicked() {
                        selected = Some(index);
                    }
                    if response.has_focus() {
                        focused = Some(index);
                    }
                }
            });
        // ↑/↓ move from the focused section once, after every item has
        // run, so the section focus moves to does not step again.
        if let Some(index) = focused {
            let step = vertical_arrow_step(ui);
            if let Some(target) = step_index(Some(index), step, self.sections.len(), |_| true)
                .filter(|target| *target != index)
            {
                ui.memory_mut(|memory| memory.request_focus(ids[target]));
                selected = Some(target);
            }
        }
        if let Some(index) = selected {
            *self.current = index;
        }
        selected
    }

    fn item(
        &self,
        ui: &Ui,
        rect: Rect,
        id: Id,
        index: usize,
        section: &Section<'_>,
        forced: Option<ControlState>,
    ) -> Response {
        let theme = ui.tokens();
        let current = *self.current == index;
        let response = ui.interact(rect, id, Sense::click()).with_pointing_hand();
        let label = match (section.count, section.alerts) {
            (Some(count), 0) => format!("{}, {count}", section.name),
            (Some(count), 1) => format!("{}, {count}, 1 problem", section.name),
            (Some(count), alerts) => format!("{}, {count}, {alerts} problems", section.name),
            (None, _) => section.name.to_owned(),
        };
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), current, &label)
        });
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let state = Interaction::of(&response, forced, false);
        ui.painter().add(crate::selection::selectable_surface(
            &theme,
            rect,
            radius::SECTION_ITEM,
            (current, state.is_active()),
            None,
            false,
        ));
        let (role, color, count_color) = if current {
            (
                ITEM_ROLE.with_weight(Weight::SemiBold),
                theme.accent.text,
                theme.accent.text,
            )
        } else {
            (ITEM_ROLE, theme.nav.text, theme.text.muted)
        };
        let inner = rect.shrink2(vec2(size::SECTION_ITEM_PADDING_X, 0.0));
        let alerting = section.count.is_some() && section.alerts > 0;
        let count = section.count.map(|count| {
            let text = if alerting {
                format!("/ {count}")
            } else {
                count.to_string()
            };
            COUNT_ROLE.galley(ui.ctx(), &text, count_color, f32::INFINITY)
        });
        let alerts = alerting.then(|| {
            COUNT_ROLE.galley(
                ui.ctx(),
                &section.alerts.to_string(),
                theme.danger.text,
                f32::INFINITY,
            )
        });
        let count_width = count.as_ref().map_or(0.0, |count| {
            count.rect.width()
                + alerts
                    .as_ref()
                    .map_or(0.0, |alerts| alerts.rect.width() + ALERT_GAP)
                + space::S8
        });
        let name = role.galley_elided(
            ui.ctx(),
            section.name,
            color,
            inner.width() - count_width,
            1,
        );
        let center_y = rect.center().y;
        type_scale::paint_text_as(
            ui,
            &response,
            pos2(inner.left(), center_y - name.rect.height() / 2.0),
            name,
            color,
            egui::accesskit::Role::Button,
        );
        if let Some(count) = count {
            let mut right = inner.right() - count.rect.width();
            let pos = pos2(right, center_y - count.rect.height() / 2.0);
            type_scale::paint_galley(ui.painter(), pos, count, count_color);
            if let Some(alerts) = alerts {
                right -= ALERT_GAP + alerts.rect.width();
                let pos = pos2(right, center_y - alerts.rect.height() / 2.0);
                type_scale::paint_galley(ui.painter(), pos, alerts, theme.danger.text);
            }
        }
        response
    }
}

/// Padding of an inspector's scrolling column (`padding: 4px 20px 20px`).
const COLUMN_PADDING: egui::Margin = egui::Margin {
    left: 20,
    right: 20,
    top: 4,
    bottom: 20,
};

/// What [`InspectorBody`] remembers between frames.
#[derive(Clone, Default)]
struct BodyState {
    /// The section a click or key chose: it stays current until the reader scrolls.
    pinned: Option<usize>,
    /// The section a click asked to scroll to.
    scroll_to: Option<usize>,
    current: usize,
}

/// The body of an inspector blade (C19): under a divider, the
/// [`SectionRail`] on the left (from four sections up) and the sections laid
/// out one after another in a scrolling column. Clicking a section scrolls
/// there; scrolling lights the section at the top.
#[must_use = "call `show` to add the body"]
pub struct InspectorBody<'s, 'a> {
    id_salt: Id,
    caption: &'a str,
    sections: &'s [Section<'a>],
    scroll_to: Option<usize>,
}

impl<'s, 'a> InspectorBody<'s, 'a> {
    pub fn new(
        id_salt: impl std::hash::Hash + std::fmt::Debug,
        caption: &'a str,
        sections: &'s [Section<'a>],
    ) -> Self {
        Self {
            id_salt: Id::new(id_salt),
            caption,
            sections,
            scroll_to: None,
        }
    }

    /// Scroll to section `index` this frame (a key or header action that jumps there).
    pub fn scroll_to(mut self, index: Option<usize>) -> Self {
        self.scroll_to = index;
        self
    }

    /// Fill the room `ui` has left; `add_section` lays section `index` out.
    pub fn show(self, ui: &mut Ui, mut add_section: impl FnMut(&mut Ui, usize)) {
        let theme = ui.tokens();
        let rect = ui.available_rect_before_wrap();
        let id = ui.id().with(self.id_salt);
        let mut state = ui
            .data(|data| data.get_temp::<BodyState>(id))
            .unwrap_or_default();
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, vec2(rect.width(), size::HAIRLINE)),
            0,
            theme.divider.base,
        );
        let top = rect.top() + size::HAIRLINE;
        let mut column = Rect::from_min_max(pos2(rect.left(), top), rect.max);
        if self.sections.len() >= 4 {
            let rail = Rect::from_min_size(
                pos2(rect.left(), top),
                vec2(size::SECTION_RAIL, rect.bottom() - top),
            );
            let mut rail_ui = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt("section-rail")
                    .max_rect(rail),
            );
            let mut current = state.current.min(self.sections.len() - 1);
            if let Some(index) = SectionRail::new(self.caption, self.sections, &mut current)
                .corner_radius(CornerRadius {
                    sw: radius::OVERLAY,
                    ..CornerRadius::ZERO
                })
                .show(&mut rail_ui, rail.height())
            {
                state.scroll_to = Some(index);
            }
            state.current = current;
            column.min.x = rail.right();
        }
        let mut column_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("section-column")
                .max_rect(column),
        );
        column_ui.set_clip_rect(column.intersect(ui.clip_rect()));
        let scroll_to = state.scroll_to.take().or(self.scroll_to);
        if scroll_to.is_some() {
            state.pinned = scroll_to;
        }
        let mut tops = Vec::with_capacity(self.sections.len());
        let output = crate::scroll::vertical()
            .id_salt(id.with("scroll"))
            .auto_shrink([false, false])
            .show(&mut column_ui, |ui| {
                egui::Frame::new()
                    .inner_margin(COLUMN_PADDING)
                    .show(ui, |ui| {
                        // Sections set their own gaps; content that relies on the app's item
                        // spacing asks for it.
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        let origin = ui.cursor().top();
                        for index in 0..self.sections.len() {
                            let section_top = ui.cursor().top();
                            tops.push(section_top - origin);
                            if scroll_to == Some(index) {
                                let target = Rect::from_min_size(
                                    pos2(ui.cursor().left(), section_top),
                                    vec2(1.0, 1.0),
                                );
                                ui.scroll_to_rect(target, Some(egui::Align::Min));
                            }
                            add_section(ui, index);
                        }
                    });
            });
        ui.allocate_rect(rect, Sense::hover());
        if self.sections.len() >= 4 {
            let offset = output.state.offset.y;
            let max_offset = (output.content_size.y - output.inner_rect.height()).max(0.0);
            let reached_end = max_offset > 0.0 && offset >= max_offset - 1.0;
            let lead = COLUMN_PADDING.top as f32;
            state.current = if reached_end {
                self.sections.len() - 1
            } else {
                tops.iter()
                    .rposition(|top| *top <= offset + lead + 1.0)
                    .unwrap_or(0)
            };
            // A chosen section stays lit until the reader scrolls on their own.
            let wheel = ui.input(|input| input.smooth_scroll_delta != Vec2::ZERO)
                && ui.rect_contains_pointer(column);
            if wheel {
                state.pinned = None;
            }
            if let Some(index) = state.pinned {
                state.current = index;
            }
        }
        ui.data_mut(|data| data.insert_temp(id, state));
    }
}

/// The title of an inspector section (`.sec`): Bricolage 15/20 with a muted
/// meta ("metrics.k8s.io · last 10 min") after it on one baseline, 20 points
/// of room above and 12 below.
pub fn inspector_section_heading(ui: &mut Ui, title: &str, meta: Option<&str>) {
    const TITLE: TextRole = type_scale::CARD_TITLE.with_size(15.0, 20.0);
    let theme = ui.tokens();
    let ppp = ui.pixels_per_point();
    ui.add_space(space::S20);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(space::S8, 0.0);
        ui.set_height(TITLE.line_height());
        let baseline = TITLE.css_baseline(ppp);
        TITLE.label_on_baseline(ui, title, theme.text.primary, baseline);
        if let Some(meta) = meta {
            type_scale::META.label_on_baseline(ui, meta, theme.text.muted, baseline);
        }
    });
    ui.add_space(space::S12);
}
