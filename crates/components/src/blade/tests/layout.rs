use super::transforms::{
    active_transform, blade_width, closing_transform, height, history_transform, interpolate,
    transformed_rect,
};
use super::*;
use crate::test_support::primary_click;

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

/// [`wide_harness`] with every role in its own colour, so painted frames
/// are found by their role rather than a colour several roles share.
#[allow(clippy::type_complexity)]
fn wide_role_harness(
    id: &'static str,
    titles: &[&'static str],
    size: egui::Vec2,
) -> (Rc<RefCell<BladeNavigator<TestBlade>>>, Harness<'static>) {
    let (navigator, mut harness) = wide_harness(id, titles, size);
    crate::test_support::install_distinct_role_theme(&harness.ctx);
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
    drawn_blade_frames_in(harness, harness.ctx.content_rect())
}

/// [`drawn_blade_frames`] for blades laid out in `viewport`.
fn drawn_blade_frames_in(harness: &Harness<'_>, viewport: Rect) -> Vec<Rect> {
    let min_height = height(viewport) * HISTORY_SCALES[HISTORY_SCALES.len() - 1] - 1.0;
    let tokens = harness.ctx.tokens();
    let mut frames = Vec::new();
    for clipped in &harness.output().shapes {
        let mut flat = Vec::new();
        flatten(&clipped.shape, &mut flat);
        for shape in flat {
            if let egui::Shape::Rect(rect) = shape
                && rect.fill == tokens.surface.panel
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
    primary_click(harness, position);
    harness.run();
}

#[test]
fn wide_non_wrapping_content_does_not_resize_the_blade() {
    let (_, harness) = wide_role_harness(
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
    let (navigator, mut harness) = wide_role_harness(
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
    let (navigator, mut harness) = wide_role_harness(
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

    primary_click(
        &mut harness,
        egui::pos2(frame.right() + 4.0, button.center().y),
    );
    harness.run();
    assert_eq!(clicks.get(), 0, "the clipped part must not receive clicks");
    primary_click(
        &mut harness,
        egui::pos2(button.left() + 10.0, button.center().y),
    );
    harness.run();
    assert_eq!(clicks.get(), 1, "the visible part must receive clicks");
}

/// Click in the part of the blade `depth` steps back that the blades in front
/// of it leave visible, halfway between its left edge and the next blade's.
fn click_history_blade(harness: &mut Harness<'_>, depth: usize) {
    let viewport = harness.ctx.content_rect();
    click_history_blade_in(harness, viewport, depth);
}

/// [`click_history_blade`] for blades laid out in `viewport`, which is the
/// window less any top inset.
fn click_history_blade_in(harness: &mut Harness<'_>, viewport: Rect, depth: usize) {
    primary_click(harness, history_blade_click_position(viewport, depth));
    harness.run();
}

/// Where [`click_history_blade`] clicks: in the visible strip of the blade
/// `depth` steps back, which starts at the window's left edge when the fan
/// reaches past it.
fn history_blade_click_position(viewport: Rect, depth: usize) -> egui::Pos2 {
    let this = transformed_rect(viewport, history_transform(viewport, depth));
    let in_front = if depth == 0 {
        transformed_rect(viewport, active_transform(viewport))
    } else {
        transformed_rect(viewport, history_transform(viewport, depth - 1))
    };
    let left = this.left().max(viewport.left());
    egui::pos2((left + in_front.left()) / 2.0, this.center().y)
}

#[test]
fn clicking_the_visible_part_of_each_back_blade_goes_back_to_it() {
    for (depth, expected) in [(0, 2), (1, 1)] {
        let (navigator, mut harness) = wide_role_harness(
            "blade-click-each-back-blade",
            &["First", "Second", "Third"],
            crate::test_support::EGUI_TEST_SIZE,
        );
        click_history_blade(&mut harness, depth);
        assert_eq!(
            navigator.borrow().current().id,
            expected,
            "clicking the back blade at depth {depth}"
        );
    }
}

#[test]
fn back_blades_show_their_content_and_are_not_dimmed_by_the_scrim() {
    let (_, harness) = wide_role_harness(
        "blade-scrim-behind-stack",
        &["First", "Second", "Third"],
        crate::test_support::EGUI_TEST_SIZE,
    );
    // The back blades render their real content, not a placeholder.
    harness.get_by_label("History · First");
    harness.get_by_label("History · Second");
    harness.get_by_label("Active · Third");

    let viewport = harness.ctx.content_rect();
    let scrim = harness.ctx.tokens().shade.scrim;
    let panel = harness.ctx.tokens().surface.panel;
    let mut scrim_at = None;
    let mut blade_surfaces = Vec::new();
    let mut texts = Vec::new();
    for (index, clipped) in harness.output().shapes.iter().enumerate() {
        let mut flat = Vec::new();
        flatten(&clipped.shape, &mut flat);
        for shape in flat {
            match shape {
                egui::Shape::Rect(rect) if rect.fill == scrim && rect.rect == viewport => {
                    scrim_at = Some(index);
                }
                egui::Shape::Rect(rect)
                    if rect.fill == panel
                        && rect.rect.width() > 100.0
                        && rect.rect.height() > 100.0 =>
                {
                    blade_surfaces.push(index);
                }
                egui::Shape::Text(text)
                    if text.galley.text().contains("History ·")
                        || text.galley.text().contains("Active ·") =>
                {
                    texts.push(index);
                }
                _ => {}
            }
        }
    }
    let scrim_at = scrim_at.expect("the scrim is painted");
    assert_eq!(blade_surfaces.len(), 3, "every blade of the stack is drawn");
    assert_eq!(texts.len(), 3, "every blade draws its content");
    assert!(
        blade_surfaces
            .iter()
            .chain(&texts)
            .all(|index| *index > scrim_at),
        "the scrim dims the workspace only: it is painted before every blade \
         (scrim {scrim_at}, blades {blade_surfaces:?}, content {texts:?})"
    );
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
    let (navigator, mut harness) = wide_role_harness(
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

/// Blades are 720 points wide, at most half the window but never under 560,
/// and never wider than the window allows.
#[test]
fn blade_width_follows_the_window() {
    let viewport = |width: f32| Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 900.0));
    assert_eq!(blade_width(viewport(2000.0)), WIDTH);
    assert_eq!(blade_width(viewport(1200.0)), 600.0);
    assert_eq!(blade_width(viewport(1000.0)), size::BLADE_MIN);
    assert_eq!(blade_width(viewport(500.0)), 500.0 - 2.0 * INSET);
}

/// The body footer runs to the blade's edges and bottom, its buttons centred
/// in it with the first one on the right.
#[test]
fn the_body_footer_spans_the_bottom_of_the_blade() {
    let mut navigator = BladeNavigator::new(TestBlade {
        id: 1,
        title: "Edit",
    });
    navigator.clear_transition();
    let navigator = Rc::new(RefCell::new(navigator));
    let navigator_for_ui = Rc::clone(&navigator);
    let stack = BladeStack::new("blade-body-footer");
    let mut harness = Harness::new_ui(move |ui| {
        stack.show_with_title(
            ui.ctx(),
            &mut navigator_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            |ui, _, _| {
                chrome::blade_body_footer(ui, |ui| {
                    crate::Button::primary("Save changes").show(ui);
                    crate::Button::secondary("Cancel").show(ui);
                });
            },
        );
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();

    let viewport = harness.ctx.content_rect();
    let blade = transformed_rect(viewport, active_transform(viewport));
    let save = harness.get_by_label("Save changes").rect();
    let cancel = harness.get_by_label("Cancel").rect();
    let footer_top = blade.bottom() - size::BLADE_FOOTER;
    let row_centre = (footer_top + size::HAIRLINE + blade.bottom()) / 2.0;
    assert!(
        (save.center().y - row_centre).abs() <= 0.5,
        "{save:?} in {blade:?}"
    );
    assert!(
        (save.right() - (blade.right() - space::S24)).abs() <= 0.5,
        "{save:?}"
    );
    assert!(
        cancel.right() < save.left(),
        "the primary action is rightmost"
    );
}

#[test]
fn only_the_two_nearest_of_many_back_blades_are_drawn_and_clickable() {
    let (navigator, mut harness) = wide_role_harness(
        "blade-click-many-back-blades",
        &["First", "Second", "Third", "Fourth", "Fifth"],
        crate::test_support::EGUI_TEST_SIZE,
    );
    assert_eq!(drawn_blade_frames(&harness).len(), 3);
    assert!(harness.query_by_label("History · First").is_none());
    click_history_blade(&mut harness, 1);
    assert_eq!(
        navigator.borrow().current().id,
        3,
        "the second visible back blade is two steps back"
    );
}

/// A stack of plain test blades `size` large, with `inset` points of window
/// chrome above it, and whether the stack reported a dismissal.
#[allow(clippy::type_complexity)]
fn fan_harness(
    id: &'static str,
    titles: &[&'static str],
    size: egui::Vec2,
    inset: f32,
) -> (
    Rc<RefCell<BladeNavigator<TestBlade>>>,
    Rc<std::cell::Cell<bool>>,
    Harness<'static>,
) {
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
    let dismissed = Rc::new(std::cell::Cell::new(false));
    let mut stack = BladeStack::new(id);
    stack.set_top_inset(inset);
    let navigator_for_ui = Rc::clone(&navigator);
    let dismissed_for_ui = Rc::clone(&dismissed);
    let mut harness = Harness::new_ui(move |ui| {
        let response = stack.show_with_title(
            ui.ctx(),
            &mut navigator_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            render_test_blade,
        );
        if response.dismissed {
            dismissed_for_ui.set(true);
        }
    });
    crate::test_support::setup_egui(&mut harness);
    harness.set_size(size);
    crate::test_support::install_distinct_role_theme(&harness.ctx);
    harness.run();
    harness.run();
    (navigator, dismissed, harness)
}

#[test]
fn a_top_inset_keeps_the_fanned_back_blades_below_the_banner() {
    const BANNER: f32 = 40.0;
    let (navigator, dismissed, mut harness) = fan_harness(
        "blade-inset-fan",
        &["First", "Second", "Third"],
        crate::test_support::EGUI_TEST_SIZE,
        BANNER,
    );
    let window = harness.ctx.content_rect();
    let viewport = Rect::from_min_max(egui::pos2(window.left(), window.top() + BANNER), window.max);

    let mut expected = expected_frames(viewport, 2);
    expected.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
    let drawn = drawn_blade_frames_in(&harness, viewport);
    assert_eq!(drawn.len(), expected.len(), "drawn frames: {drawn:?}");
    for (drawn, expected) in drawn.iter().zip(&expected) {
        assert_rect_close(*drawn, *expected, "blade frame under the banner");
        assert!(
            drawn.top() >= window.top() + BANNER + INSET - 1.0,
            "a blade must stay below the banner: {drawn:?}"
        );
    }

    // The fanned blades are still reachable below the banner...
    click_history_blade_in(&mut harness, viewport, 1);
    assert_eq!(navigator.borrow().current().id, 1);
    assert!(!dismissed.get());

    // ...and the strip of the banner above them is not a blade.
    let (navigator, dismissed, mut harness) = fan_harness(
        "blade-inset-fan-banner-click",
        &["First", "Second"],
        crate::test_support::EGUI_TEST_SIZE,
        BANNER,
    );
    let back = transformed_rect(viewport, history_transform(viewport, 0));
    primary_click(&mut harness, egui::pos2(back.left() + 8.0, BANNER / 2.0));
    harness.run();
    assert_eq!(
        navigator.borrow().current().id,
        2,
        "the banner is not part of a back blade"
    );
    assert!(!dismissed.get(), "a click on the banner must not dismiss");
}

#[test]
fn clicking_a_back_blade_while_the_stack_is_sliding_does_nothing() {
    // While blades move, `active_interactable` is false: back blades get no
    // click target and a click on one is swallowed (it neither goes back a
    // second time nor dismisses the stack).
    let (navigator, dismissed, mut harness) = fan_harness(
        "blade-click-mid-transition",
        &["First", "Second", "Third"],
        crate::test_support::EGUI_TEST_SIZE,
        0.0,
    );
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 1.0);
    harness.input_mut().time = Some(100.0);
    harness.step();
    let viewport = harness.ctx.content_rect();

    // The first click goes back once and starts the slide.
    primary_click(&mut harness, history_blade_click_position(viewport, 0));
    harness.step();
    harness.step();
    assert_eq!(navigator.borrow().current().id, 2);
    assert_eq!(navigator.borrow().transition(), Some(BladeTransition::Back));

    harness.input_mut().time = Some(100.0 + f64::from(TRANSITION_DURATION / 2.0));
    harness.step();
    assert!(
        navigator.borrow().transition().is_some(),
        "the slide is still running"
    );
    for _ in 0..2 {
        primary_click(&mut harness, history_blade_click_position(viewport, 0));
        harness.step();
        harness.step();
    }
    let navigator = navigator.borrow();
    assert_eq!(navigator.current().id, 2, "no second step back");
    assert_eq!(navigator.back_stack().len(), 1);
    assert_eq!(navigator.forward_stack().len(), 1);
    assert!(!dismissed.get(), "the click must not dismiss the stack");
}

#[test]
fn forward_history_survives_going_back_by_clicking_a_back_blade() {
    let (navigator, _, mut harness) = fan_harness(
        "blade-forward-after-click-back",
        &["First", "Second", "Third"],
        crate::test_support::EGUI_TEST_SIZE,
        0.0,
    );
    click_history_blade(&mut harness, 0);
    assert_eq!(navigator.borrow().current().id, 2);
    assert!(navigator.borrow().can_go_forward());
    navigator.borrow_mut().clear_transition();
    harness.run();

    crate::test_support::click_label(&mut harness, "Forward");
    let navigator = navigator.borrow();
    assert_eq!(navigator.current().id, 3);
    assert_eq!(
        navigator
            .back_stack()
            .iter()
            .map(|blade| blade.id)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert!(!navigator.can_go_forward());
}

/// Below roughly 755 points the fan reaches past the window's left edge so far
/// that the depth-1 blade has no visible strip at all.
#[test]
fn the_second_back_blade_stays_clickable_as_blades_shrink_toward_their_minimum() {
    for width in [1000.0, 900.0, 800.0] {
        let (navigator, _, mut harness) = fan_harness(
            "blade-narrow-second-back",
            &["First", "Second", "Third"],
            egui::vec2(width, 600.0),
            0.0,
        );
        let viewport = harness.ctx.content_rect();
        assert!(blade_width(viewport) < WIDTH, "{width} must shrink blades");
        click_history_blade(&mut harness, 1);
        assert_eq!(
            navigator.borrow().current().id,
            1,
            "depth-1 back blade at width {width}"
        );
    }
}
