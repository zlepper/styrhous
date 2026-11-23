//! The section rail of an inspector (C19 `.srail`): its sections with
//! their counts, the current one lit, so long resources stay navigable.

use egui::{CornerRadius, Id, Rect, Response, Sense, Ui, pos2, vec2};

use crate::PointingHand;
use crate::design::theme::ThemeExt;
use crate::design::type_scale::{self, TextRole, Weight};
use crate::design::{radius, size, space};
use crate::interaction::{ControlState, Interaction, step_index, vertical_arrow_step};

/// A section's name (`.si`): UI 13/20, 600 when current.
const ITEM_ROLE: TextRole = type_scale::UI;
/// A section's count (`.si .c`): Plex Mono 10.5 in the item's line box.
const COUNT_ROLE: TextRole = type_scale::MONO_COUNT;
/// The caption above the sections ("On this pod"), with its bottom padding.
const CAPTION_PADDING_BOTTOM: f32 = space::S8;
/// Gap between the items (`.srail` gap 2).
const ITEM_GAP: f32 = space::S2;

/// One section of a [`SectionRail`]: its name and, when it lists things,
/// how many.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Section<'a> {
    pub name: &'a str,
    pub count: Option<usize>,
}

impl<'a> Section<'a> {
    pub const fn new(name: &'a str) -> Self {
        Self { name, count: None }
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

        let mut top =
            content.top() + type_scale::CAPTION.line_height() + CAPTION_PADDING_BOTTOM + ITEM_GAP;
        let ids: Vec<Id> = (0..self.sections.len())
            .map(|index| response.id.with(("section", index)))
            .collect();
        let mut selected = None;
        let mut focused = None;
        for (index, section) in self.sections.iter().enumerate() {
            let item = Rect::from_min_size(
                pos2(content.left(), top),
                vec2(content.width(), size::SECTION_ITEM),
            );
            top += size::SECTION_ITEM + ITEM_GAP;
            let forced = (self.forced_hover == Some(index)).then_some(ControlState::Hovered);
            let response = self.item(ui, item, ids[index], index, section, forced);
            if response.clicked() {
                selected = Some(index);
            }
            if response.has_focus() {
                focused = Some(index);
            }
        }
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
        let label = match section.count {
            Some(count) => format!("{}, {count}", section.name),
            None => section.name.to_owned(),
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
        let count = section.count.map(|count| {
            COUNT_ROLE.galley(ui.ctx(), &count.to_string(), count_color, f32::INFINITY)
        });
        let count_width = count
            .as_ref()
            .map_or(0.0, |count| count.rect.width() + space::S8);
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
            let pos = pos2(
                inner.right() - count.rect.width(),
                center_y - count.rect.height() / 2.0,
            );
            type_scale::paint_galley(ui.painter(), pos, count, count_color);
        }
        response
    }
}
