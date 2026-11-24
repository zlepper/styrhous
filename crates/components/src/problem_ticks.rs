//! Problem ticks (C17 `.sticks`): a slim track beside a list marking every
//! problem row, with a thumb for the part on screen, so problems can be
//! found and jumped to without scrolling through the list.

use egui::{Id, Rect, Response, Sense, Ui, pos2, vec2};

use crate::design::theme::{ThemeExt, Tone};
use crate::design::{radius, size};
use crate::tooltip::TooltipExt;

/// One problem row: where it sits in the list (0 at the top, 1 at the
/// bottom) and how bad it is ([`Tone::Danger`] for failing rows,
/// [`Tone::Warning`] for degraded ones).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProblemTick {
    pub position: f32,
    pub tone: Tone,
}

/// What [`ProblemTicks`] did this frame.
pub struct ProblemTicksResponse {
    /// The track.
    pub response: Response,
    /// The tick that was clicked: scroll its row into view.
    pub clicked: Option<usize>,
    /// Where the track was dragged (or clicked, with no tick to jump to) to,
    /// as a fraction of its height: scroll the list there. Only for
    /// [`ProblemTicks::draggable`] tracks.
    pub scrubbed_to: Option<f32>,
}

/// A [`size::PROBLEM_TRACK`] wide track on `surface-raised` with a
/// `divider-strong` thumb over the visible part of the list and one
/// [`size::PROBLEM_TICK`] tick per problem, in `warning-fill` or
/// `danger-fill`, reaching a point past the track on each side.
///
/// Ticks that would overlap merge into one band, the worse tone winning,
/// so thousands of problems stay legible (S17). Clicking the track picks
/// the tick nearest the pointer.
#[must_use = "call `show` to add the ticks"]
pub struct ProblemTicks<'a> {
    ticks: &'a [ProblemTick],
    thumb: Option<(f32, f32)>,
    id_salt: Id,
    draggable: bool,
}

impl<'a> ProblemTicks<'a> {
    pub fn new(ticks: &'a [ProblemTick]) -> Self {
        Self {
            ticks,
            thumb: None,
            id_salt: Id::new("problem-ticks"),
            draggable: false,
        }
    }

    /// A stable ID salt for the track (one per track in a `Ui`).
    pub fn id_salt(mut self, id_salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        self.id_salt = Id::new(id_salt);
        self
    }

    /// Make the track a scroll bar too: dragging it reports where to scroll
    /// to ([`ProblemTicksResponse::scrubbed_to`]).
    pub fn draggable(mut self) -> Self {
        self.draggable = true;
        self
    }

    /// The visible part of the list, from `start` to `start + length` (as
    /// fractions of the list).
    pub fn thumb(mut self, start: f32, length: f32) -> Self {
        self.thumb = Some((start, length));
        self
    }

    /// Add the track at the cursor, `height` points tall.
    pub fn show(self, ui: &mut Ui, height: f32) -> ProblemTicksResponse {
        let (rect, _) = ui.allocate_exact_size(vec2(size::PROBLEM_TRACK, height), Sense::hover());
        self.show_in(ui, rect)
    }

    /// Show the track in `rect` (already allocated).
    pub fn show_in(self, ui: &mut Ui, rect: Rect) -> ProblemTicksResponse {
        let sense = if self.draggable {
            Sense::click_and_drag()
        } else {
            Sense::click()
        };
        let response = ui.interact(rect, ui.id().with(self.id_salt), sense);
        let count = self.ticks.len();
        let label = crate::search::counted(count, "problem", "problems");
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), &label)
        });
        let clicked = response
            .interact_pointer_pos()
            .filter(|_| response.clicked())
            .and_then(|pointer| self.nearest(rect, pointer.y));
        let pointer_fraction = || {
            response
                .interact_pointer_pos()
                .map(|pointer| ((pointer.y - rect.top()) / rect.height()).clamp(0.0, 1.0))
        };
        let scrubbed_to = if !self.draggable {
            None
        } else if response.dragged() || (response.clicked() && clicked.is_none()) {
            pointer_fraction()
        } else {
            None
        };
        if ui.is_rect_visible(rect) {
            self.paint(ui, rect);
        }
        let response = if count > 0 {
            response.tooltip("Click to jump to the nearest problem; [ ] step through them")
        } else {
            response
        };
        ProblemTicksResponse {
            response,
            clicked,
            scrubbed_to,
        }
    }

    /// The index of the tick nearest `y`.
    fn nearest(&self, rect: Rect, y: f32) -> Option<usize> {
        self.ticks
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let distance = |tick: &ProblemTick| (tick_top(rect, tick.position) - y).abs();
                distance(a).total_cmp(&distance(b))
            })
            .map(|(index, _)| index)
    }

    fn paint(&self, ui: &Ui, rect: Rect) {
        let theme = ui.tokens();
        let painter = ui.painter();
        painter.rect_filled(rect, radius::PROBLEM_TRACK, theme.surface.raised);
        if let Some((start, length)) = self.thumb {
            let thumb = Rect::from_min_size(
                pos2(rect.left(), rect.top() + start * rect.height()),
                vec2(rect.width(), length * rect.height()),
            );
            painter.rect_filled(thumb, radius::PROBLEM_TRACK, theme.divider.strong);
        }
        for (top, bottom, tone) in bands(rect, self.ticks) {
            let band = Rect::from_min_max(
                pos2(rect.left() - size::PROBLEM_TICK_OVERHANG, top),
                pos2(rect.right() + size::PROBLEM_TICK_OVERHANG, bottom),
            );
            painter.rect_filled(band, radius::PROBLEM_TICK, theme.tone(tone).fill);
        }
    }
}

/// The top of the tick at `position` on the track `rect`; a tick at the
/// very end still ends on the track.
fn tick_top(rect: Rect, position: f32) -> f32 {
    (rect.top() + position.clamp(0.0, 1.0) * rect.height())
        .min(rect.bottom() - size::PROBLEM_TICK)
        .max(rect.top())
}

/// The ticks as bands from top to bottom: overlapping ticks merge, the
/// worse tone winning.
fn bands(rect: Rect, ticks: &[ProblemTick]) -> Vec<(f32, f32, Tone)> {
    let mut sorted: Vec<(f32, Tone)> = ticks
        .iter()
        .map(|tick| (tick_top(rect, tick.position), tick.tone))
        .collect();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut bands: Vec<(f32, f32, Tone)> = Vec::new();
    for (top, tone) in sorted {
        let bottom = top + size::PROBLEM_TICK;
        match bands.last_mut() {
            Some(band) if top < band.1 => {
                band.1 = band.1.max(bottom);
                if tone == Tone::Danger {
                    band.2 = Tone::Danger;
                }
            }
            _ => bands.push((top, bottom, tone)),
        }
    }
    bands
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick(position: f32, tone: Tone) -> ProblemTick {
        ProblemTick { position, tone }
    }

    #[test]
    fn overlapping_ticks_merge_into_bands_with_the_worse_tone() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(6.0, 100.0));
        let ticks = [
            tick(0.50, Tone::Warning),
            tick(0.10, Tone::Warning),
            tick(0.11, Tone::Danger),
            tick(0.90, Tone::Danger),
        ];
        assert_eq!(
            bands(rect, &ticks),
            vec![
                (10.0, 14.0, Tone::Danger),
                (50.0, 53.0, Tone::Warning),
                (90.0, 93.0, Tone::Danger),
            ]
        );
    }

    #[test]
    fn a_click_picks_the_nearest_tick() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(6.0, 100.0));
        let ticks = [tick(0.2, Tone::Warning), tick(0.7, Tone::Danger)];
        let track = ProblemTicks::new(&ticks);
        assert_eq!(track.nearest(rect, 30.0), Some(0));
        assert_eq!(track.nearest(rect, 60.0), Some(1));
        assert_eq!(ProblemTicks::new(&[]).nearest(rect, 60.0), None);
    }

    #[test]
    fn a_tick_at_the_end_of_the_list_stays_on_the_track() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(6.0, 100.0));
        assert_eq!(
            bands(rect, &[tick(1.0, Tone::Danger)]),
            vec![(97.0, 100.0, Tone::Danger)]
        );
    }
}
