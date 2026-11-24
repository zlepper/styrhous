//! Shared scrolling behavior for application surfaces.

use egui::{Context, ScrollArea, Ui, Vec2};

/// Logical points moved by a line-based mouse-wheel event.
pub const LINE_SCROLL_SPEED: f32 = 80.0;
/// Multiplier for vertical point-based scrolling, including touchpads.
pub const VERTICAL_SCROLL_MULTIPLIER: f32 = 2.0;

/// Configure input behavior on the egui context shared by every native viewport.
pub fn configure_input(ctx: &Context) {
    ctx.options_mut(|options| {
        options.input_options.line_scroll_speed = LINE_SCROLL_SPEED;
    });
}

/// A scroll area with the application's standard touchpad and wheel behavior.
pub fn both() -> ScrollArea {
    with_standard_scroll_speed(ScrollArea::both())
}

/// [`both`] whose vertical bar stops above the horizontal one.
///
/// egui lets the two bars of a `ScrollArea::both` run to the same corner, so
/// when both are enlarged (hovered or dragged) they overlap in a
/// `bar_width` square. The area is the space `ui` has left, which a
/// non-shrinking scroll area fills.
pub fn both_clear_of_corner(ui: &Ui) -> ScrollArea {
    let scroll = ui.spacing().scroll;
    let area = ui.available_rect_before_wrap();
    let bars = area.with_max_y(area.max.y - scroll.bar_width - scroll.bar_outer_margin);
    both().scroll_bar_rect(bars)
}

/// A vertically scrolling area with the application's standard scroll behavior.
pub fn vertical() -> ScrollArea {
    with_standard_scroll_speed(ScrollArea::vertical())
}

/// A horizontally scrolling area with the application's standard scroll behavior.
pub fn horizontal() -> ScrollArea {
    with_standard_scroll_speed(ScrollArea::horizontal())
}

fn with_standard_scroll_speed(scroll_area: ScrollArea) -> ScrollArea {
    scroll_area.wheel_scroll_multiplier(Vec2::new(1.0, VERTICAL_SCROLL_MULTIPLIER))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two bars of a `ScrollArea::both` must not share the corner square.
    #[test]
    fn both_clear_of_corner_stops_the_vertical_bar_above_the_horizontal_one() {
        let ctx = Context::default();
        let bar_width = 10.0;
        ctx.all_styles_mut(|style| {
            // Floating bars that are always full width: when egui lets them overlap.
            style.spacing.scroll = egui::style::ScrollStyle {
                floating_width: bar_width,
                bar_outer_margin: 2.0,
                ..egui::style::ScrollStyle::floating()
            };
        });

        let mut shapes = Vec::new();
        for frame in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(320.0, 200.0),
                )),
                time: Some(f64::from(frame) * 0.1),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    both_clear_of_corner(ui)
                        .id_salt("corner-test")
                        .auto_shrink([false, false])
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                        )
                        .show(ui, |ui| ui.set_min_size(egui::vec2(1_000.0, 1_000.0)));
                });
            });
            output.textures_delta.clear();
            shapes = output.shapes;
        }

        let bars: Vec<egui::Rect> = shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Rect(rect) => Some(rect.rect),
                _ => None,
            })
            .collect();
        let vertical = bars
            .iter()
            .find(|rect| (rect.width() - bar_width).abs() < 0.01 && rect.height() > bar_width * 2.0)
            .expect("vertical bar was painted");
        let horizontal = bars
            .iter()
            .find(|rect| (rect.height() - bar_width).abs() < 0.01 && rect.width() > bar_width * 2.0)
            .expect("horizontal bar was painted");

        assert!(
            vertical.max.y <= horizontal.min.y,
            "vertical={vertical:?}, horizontal={horizontal:?}"
        );
    }

    #[test]
    fn standard_scroll_area_doubles_touchpad_scroll_distance() {
        let standard_offset = scroll_offset(both);
        let egui_default_offset = scroll_offset(ScrollArea::both);

        assert!(egui_default_offset > 0.0, "default scroll did not move");
        assert!(
            (standard_offset - egui_default_offset * VERTICAL_SCROLL_MULTIPLIER).abs()
                <= f32::EPSILON,
            "standard={standard_offset}, default={egui_default_offset}"
        );
    }

    fn scroll_offset(scroll_area: fn() -> ScrollArea) -> f32 {
        let ctx = Context::default();
        let input = |time| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 200.0),
            )),
            time: Some(time),
            ..Default::default()
        };
        let mut initial_output = None;
        let mut first_frame = ctx.run_ui(input(0.0), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                initial_output = Some(show_test_scroll_area(ui, scroll_area));
            });
        });
        first_frame.textures_delta.clear();

        let initial_output = initial_output.expect("scroll area was rendered");
        assert!(
            initial_output.content_size.y > initial_output.inner_rect.height(),
            "content={:?}, viewport={:?}",
            initial_output.content_size,
            initial_output.inner_rect
        );
        let mut scroll_input = input(0.1);
        scroll_input.events = vec![
            egui::Event::PointerMoved(initial_output.inner_rect.center()),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -120.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::default(),
            },
        ];
        let mut scrolled_output = None;
        let mut second_frame = ctx.run_ui(scroll_input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                scrolled_output = Some(show_test_scroll_area(ui, scroll_area));
            });
        });
        second_frame.textures_delta.clear();

        let mut third_frame = ctx.run_ui(input(0.2), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                scrolled_output = Some(show_test_scroll_area(ui, scroll_area));
            });
        });
        third_frame.textures_delta.clear();

        scrolled_output
            .expect("scroll area was rendered")
            .state
            .offset
            .y
    }

    fn show_test_scroll_area(
        ui: &mut egui::Ui,
        scroll_area: fn() -> ScrollArea,
    ) -> egui::scroll_area::ScrollAreaOutput<()> {
        scroll_area()
            .id_salt("scroll-speed-test")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_height(2_000.0);
            })
    }
}
