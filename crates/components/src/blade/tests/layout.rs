use super::transforms::{
    active_transform, blade_width, closing_transform, height, history_transform, interpolate,
    transformed_rect,
};
use super::*;
use crate::colors::{WHITE, gray};

const WIDE_ROW_WIDTH: f32 = 600.0;
const WIDE_ROW_LABEL: &str = "Wide row action with a label long enough to straddle the blade edge";

/// A row that cannot wrap: fixed-width space followed by a button.
fn render_wide_blade(ui: &mut Ui, blade: &mut TestBlade, layer: BladeLayer) {
    render_test_blade(ui, blade, layer);
    ui.horizontal(|ui| {
        ui.allocate_space(egui::vec2(WIDE_ROW_WIDTH, 20.0));
        let _ = ui.button(WIDE_ROW_LABEL);
    });
}

#[allow(clippy::type_complexity)]
fn wide_harness(
    id: &'static str,
    titles: &[&'static str],
    size: egui::Vec2,
) -> (Rc<RefCell<BladeNavigator<TestBlade>>>, Harness<'static>) {
    let mut titles = titles.iter().copied().enumerate();
    let (_, first) = titles.next().expect("at least one blade");
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: first,
    })));
    for (index, title) in titles {
        navigator.borrow_mut().push(TestBlade {
            id: index as u64 + 1,
            title,
        });
    }
    navigator.borrow_mut().clear_transition();
    let stack = BladeStack::new(id);
    let navigator_for_ui = Rc::clone(&navigator);
    let mut harness = Harness::new_ui(move |ui| {
        stack.show_with_title(
            ui.ctx(),
            &mut navigator_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            render_wide_blade,
        );
    });
    crate::test_support::setup_egui(&mut harness);
    harness.set_size(size);
    harness.run();
    harness.run();
    (navigator, harness)
}

fn flatten<'a>(shape: &'a egui::Shape, out: &mut Vec<&'a egui::Shape>) {
    if let egui::Shape::Vec(shapes) = shape {
        for shape in shapes {
            flatten(shape, out);
        }
    } else {
        out.push(shape);
    }
}

/// The frames actually painted (after visual transforms), sorted left to right.
///
/// Accessibility bounds ignore visual transforms, so the painted shapes are the
/// only observable record of where a history blade is drawn.
fn drawn_blade_frames(harness: &Harness<'_>) -> Vec<Rect> {
    let min_height =
        height(harness.ctx.content_rect()) * HISTORY_SCALES[HISTORY_SCALES.len() - 1] - 1.0;
    let mut frames = Vec::new();
    for clipped in &harness.output().shapes {
        let mut flat = Vec::new();
        flatten(&clipped.shape, &mut flat);
        for shape in flat {
            if let egui::Shape::Rect(rect) = shape
                && rect.fill == WHITE
                && rect.stroke.color == gray::_200
                && rect.rect.width() > 100.0
                && rect.rect.height() >= min_height
            {
                frames.push(rect.rect);
            }
        }
    }
    frames.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
    frames
}

/// The clip rect applied to the first painted text containing `needle`, if any
/// of it is visible (egui culls text that is clipped away entirely).
fn text_clip(harness: &Harness<'_>, needle: &str) -> Option<Rect> {
    harness.output().shapes.iter().find_map(|clipped| {
        let mut flat = Vec::new();
        flatten(&clipped.shape, &mut flat);
        flat.into_iter().any(|shape| {
                matches!(shape, egui::Shape::Text(text) if text.galley.text().contains(needle))
            })
            .then_some(clipped.clip_rect)
    })
}

fn assert_rect_close(actual: Rect, expected: Rect, what: &str) {
    let close = |a: f32, b: f32| (a - b).abs() <= 1.0;
    assert!(
        close(actual.min.x, expected.min.x)
            && close(actual.min.y, expected.min.y)
            && close(actual.width(), expected.width())
            && close(actual.height(), expected.height()),
        "{what}: expected {expected:?}, got {actual:?}"
    );
}

fn expected_frames(viewport: Rect, history: usize) -> Vec<Rect> {
    let mut frames: Vec<_> = (0..history)
        .map(|depth| transformed_rect(viewport, history_transform(viewport, depth)))
        .collect();
    frames.push(transformed_rect(viewport, active_transform(viewport)));
    frames.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
    frames
}

fn assert_frames_match_transforms(harness: &Harness<'_>, history: usize) {
    let viewport = harness.ctx.content_rect();
    let drawn = drawn_blade_frames(harness);
    let expected = expected_frames(viewport, history);
    assert_eq!(drawn.len(), expected.len(), "drawn frames: {drawn:?}");
    for (drawn, expected) in drawn.into_iter().zip(expected) {
        assert_rect_close(drawn, expected, "blade frame");
    }
}

/// Click in the visible strip of the nearest history blade, left of the active blade.
fn click_nearest_history_strip(harness: &mut Harness<'_>) {
    let viewport = harness.ctx.content_rect();
    let active = transformed_rect(viewport, active_transform(viewport));
    let position = egui::pos2(
        (active.left() - 4.0).max(viewport.left() + 1.0),
        viewport.center().y,
    );
    harness.event(egui::Event::PointerMoved(position));
    harness.run();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        });
        harness.run();
    }
}

#[test]
fn wide_non_wrapping_content_does_not_resize_the_blade() {
    let (_, harness) = wide_harness(
        "blade-wide-active",
        &["Only"],
        crate::test_support::EGUI_TEST_SIZE,
    );
    assert_frames_match_transforms(&harness, 0);
    let frame = drawn_blade_frames(&harness)[0];
    assert_eq!(frame.width(), WIDTH);
    let clip = text_clip(&harness, WIDE_ROW_LABEL)
        .expect("the straddling label must be partially visible");
    assert!(
        clip.right() <= frame.right(),
        "overflowing content must be clipped to the blade"
    );
}

#[test]
fn wide_content_in_active_and_history_blades_keeps_their_transforms() {
    let (navigator, mut harness) = wide_harness(
        "blade-wide-history",
        &["First", "Second", "Third"],
        crate::test_support::EGUI_TEST_SIZE,
    );
    assert_frames_match_transforms(&harness, 2);

    click_nearest_history_strip(&mut harness);
    assert_eq!(
        navigator.borrow().current().id,
        2,
        "clicking the nearest history blade must navigate back one step"
    );
}

#[test]
fn blades_shrink_to_fit_a_viewport_narrower_than_a_full_blade() {
    let (navigator, mut harness) = wide_harness(
        "blade-narrow-viewport",
        &["First", "Second", "Third"],
        egui::vec2(500.0, 600.0),
    );
    let viewport = harness.ctx.content_rect();
    assert!(viewport.width() < WIDTH + INSET * 2.0);
    assert_frames_match_transforms(&harness, 2);
    let active = *drawn_blade_frames(&harness).last().expect("active frame");
    assert_eq!(active.width(), viewport.width() - INSET * 2.0);
    assert!(
        active.left() >= viewport.left() && active.right() <= viewport.right(),
        "the active blade must fit inside the viewport"
    );
    assert!(
        text_clip(&harness, WIDE_ROW_LABEL).is_none_or(|clip| clip.right() <= active.right()),
        "overflowing content must be clipped to the narrower blade"
    );

    click_nearest_history_strip(&mut harness);
    assert_eq!(navigator.borrow().current().id, 2);
}

#[test]
fn snapshots_a_blade_with_a_wide_non_wrapping_row() {
    let (_, mut harness) = wide_harness(
        "blade-wide-snapshot",
        &["First", "Second"],
        crate::test_support::EGUI_TEST_SIZE,
    );
    harness.ui_harness("blades/snapshots_a_blade_with_a_wide_non_wrapping_row/wide_row");
}

#[test]
fn only_the_visible_part_of_an_overflowing_widget_is_clickable() {
    let clicks = Rc::new(std::cell::Cell::new(0));
    let clicks_for_ui = Rc::clone(&clicks);
    let mut navigator = BladeNavigator::new(TestBlade {
        id: 1,
        title: "Only",
    });
    navigator.clear_transition();
    let stack = BladeStack::new("blade-clipped-click");
    let mut harness = Harness::new_ui(move |ui| {
        stack.show_with_title(
            ui.ctx(),
            &mut navigator,
            |blade| blade.title.to_owned(),
            |ui, _, _| {
                ui.horizontal(|ui| {
                    ui.allocate_space(egui::vec2(WIDE_ROW_WIDTH, 20.0));
                    if ui.button(WIDE_ROW_LABEL).clicked() {
                        clicks_for_ui.set(clicks_for_ui.get() + 1);
                    }
                });
            },
        );
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.run();

    let viewport = harness.ctx.content_rect();
    let frame = transformed_rect(viewport, active_transform(viewport));
    let button = harness.get_by_label(WIDE_ROW_LABEL).rect();
    assert!(button.left() < frame.right() && button.right() > viewport.right() - 4.0);

    click_at(
        &mut harness,
        egui::pos2(frame.right() + 4.0, button.center().y),
    );
    assert_eq!(clicks.get(), 0, "the clipped part must not receive clicks");
    click_at(
        &mut harness,
        egui::pos2(button.left() + 10.0, button.center().y),
    );
    assert_eq!(clicks.get(), 1, "the visible part must receive clicks");
}

fn click_at(harness: &mut Harness<'_>, position: egui::Pos2) {
    harness.event(egui::Event::PointerMoved(position));
    harness.run();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        });
        harness.run();
    }
}

fn step_to_transition_midpoint(harness: &mut Harness<'_>, start: f64) {
    harness.input_mut().time = Some(start);
    harness.step();
    harness.input_mut().time = Some(start + f64::from(TRANSITION_DURATION / 2.0));
    harness.step();
}

fn assert_a_frame_matches(harness: &Harness<'_>, expected: Rect, what: &str) {
    let frames = drawn_blade_frames(harness);
    assert!(
        frames.iter().any(|frame| {
            (frame.min - expected.min).abs().max_elem() <= 1.0
                && (frame.size() - expected.size()).abs().max_elem() <= 1.0
        }),
        "{what}: expected {expected:?} among {frames:?}"
    );
}

#[test]
fn wide_content_keeps_the_active_blade_on_its_interpolated_transform_mid_transition() {
    let (navigator, mut harness) = wide_harness(
        "blade-wide-transitions",
        &["First"],
        crate::test_support::EGUI_TEST_SIZE,
    );
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 1.0);
    let viewport = harness.ctx.content_rect();
    let active = active_transform(viewport);
    // Both transitions below are at progress 0.5, where the easing is also 0.5.

    navigator.borrow_mut().push(TestBlade {
        id: 2,
        title: "Second",
    });
    step_to_transition_midpoint(&mut harness, 100.0);
    let mut forward = active;
    forward.position.x += 0.5 * blade_width(viewport);
    assert_a_frame_matches(
        &harness,
        transformed_rect(viewport, forward),
        "forward active frame",
    );
    harness.input_mut().time = Some(110.0);
    harness.step();
    navigator.borrow_mut().clear_transition();
    harness.run_steps(2);

    assert!(navigator.borrow_mut().go_back());
    step_to_transition_midpoint(&mut harness, 200.0);
    assert_a_frame_matches(
        &harness,
        transformed_rect(
            viewport,
            interpolate(history_transform(viewport, 0), active, 0.5),
        ),
        "back active frame",
    );
    harness.input_mut().time = Some(210.0);
    harness.step();
    navigator.borrow_mut().clear_transition();
    harness.run_steps(2);

    navigator.borrow_mut().begin_close();
    step_to_transition_midpoint(&mut harness, 300.0);
    assert_a_frame_matches(
        &harness,
        transformed_rect(viewport, closing_transform(viewport, active, 0.5)),
        "closing active frame",
    );
}

#[test]
fn tiny_viewports_do_not_panic() {
    for size in [egui::vec2(10.0, 10.0), egui::Vec2::ZERO] {
        let (_, mut harness) = wide_harness("blade-tiny-viewport", &["First", "Second"], size);
        harness.run();
    }
}
