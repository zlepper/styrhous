use super::*;

#[test]
fn popup_option_overlapping_the_input_scrim_receives_a_pointer_click() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "History",
    })));
    navigator.borrow_mut().push(TestBlade {
        id: 2,
        title: "Popup",
    });
    navigator.borrow_mut().clear_transition();
    let selected = Rc::new(RefCell::new(false));
    let underlying_action_clicked = Rc::new(RefCell::new(false));
    let dismissed = Rc::new(RefCell::new(false));
    let stack = BladeStack::new("blade-popup-input-order");
    let navigator_for_ui = Rc::clone(&navigator);
    let selected_for_ui = Rc::clone(&selected);
    let underlying_action_clicked_for_ui = Rc::clone(&underlying_action_clicked);
    let dismissed_for_ui = Rc::clone(&dismissed);
    let mut harness = Harness::new_ui(move |ui| {
        if ui.button("Underlying workspace action").clicked() {
            *underlying_action_clicked_for_ui.borrow_mut() = true;
        }
        let response = stack.show_with_title(
            ui.ctx(),
            &mut navigator_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            |ui, _blade, _layer| {
                let trigger = ui.button("Open popup");
                egui::Popup::menu(&trigger)
                    .align(egui::RectAlign::BOTTOM_END)
                    .width(320.0)
                    .show(|ui| {
                        if ui.button("Choose popup option").clicked() {
                            *selected_for_ui.borrow_mut() = true;
                        }
                    });
            },
        );
        *dismissed_for_ui.borrow_mut() = response.dismissed;
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();

    harness
        .get_all_by_label("Open popup")
        .max_by(|left, right| left.rect().left().total_cmp(&right.rect().left()))
        .expect("the foreground blade must render an open-popup button")
        .click();
    harness.run();

    harness.get_by_label("Underlying workspace action").click();
    harness.run();
    assert!(
        !*underlying_action_clicked.borrow(),
        "a click outside the popup must not reach the workspace beneath the blade"
    );
    assert!(
        !*dismissed.borrow(),
        "a click outside the popup must close the popup without dismissing the blade"
    );
    assert!(
        harness.query_by_label("Choose popup option").is_none(),
        "the outside click must close the popup"
    );

    harness
        .get_all_by_label("Open popup")
        .max_by(|left, right| left.rect().left().total_cmp(&right.rect().left()))
        .expect("the foreground blade must render an open-popup button")
        .click();
    harness.run();
    harness.run_steps(1);

    let option = harness.get_by_label("Choose popup option");
    let blade_left = harness.ctx.content_rect().right() - INSET - WIDTH;
    assert!(
        option.rect().left() < blade_left,
        "the popup option must extend into the input-scrim region"
    );

    option.click();
    harness.run();
    assert!(
        *selected.borrow(),
        "the popup option must receive the physical pointer click"
    );
}

#[test]
fn snapshots_a_single_blade_and_its_visible_history() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    let stack = BladeStack::new("blade-component-snapshot");
    let navigator_for_ui = Rc::clone(&navigator);
    let mut harness = Harness::new_ui(move |ui| {
        stack.show_with_title(
            ui.ctx(),
            &mut navigator_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            render_test_blade,
        );
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.ui_harness("blades/snapshots_a_single_blade_and_its_visible_history/single");

    navigator.borrow_mut().push(TestBlade {
        id: 2,
        title: "Second",
    });
    navigator.borrow_mut().push(TestBlade {
        id: 3,
        title: "Third",
    });
    navigator.borrow_mut().clear_transition();
    harness.run();
    harness.ui_harness("blades/snapshots_a_single_blade_and_its_visible_history/history");

    navigator.borrow_mut().push(TestBlade {
        id: 4,
        title: "Fourth",
    });
    navigator.borrow_mut().clear_transition();
    harness.run();
    harness.ui_harness("blades/snapshots_a_single_blade_and_its_visible_history/history_cap");
}

#[test]
fn overlap_detection_ignores_intentionally_stacked_blade_layers() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    let stack = BladeStack::new("blade-overlap-validation");
    let navigator_for_ui = Rc::clone(&navigator);
    let mut harness = Harness::new_ui(move |ui| {
        stack.show_with_title(
            ui.ctx(),
            &mut navigator_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            render_test_blade,
        );
    });
    crate::test_support::setup_egui(&mut harness);

    for (id, title) in [(2, "Second"), (3, "Third")] {
        navigator.borrow_mut().push(TestBlade { id, title });
    }
    navigator.borrow_mut().clear_transition();
    harness.run();

    let background_buttons: Vec<_> = harness
        .get_all_by_label("Back in background blade")
        .collect();
    assert_eq!(background_buttons.len(), 2);
    assert!(
        background_buttons[0]
            .rect()
            .intersects(background_buttons[1].rect()),
        "the test must exercise intentional blade overlap"
    );
    assert!(
        harness
            .illegal_accessibility_overlaps(
                &crate::test_support::AccessibilityTreeOptions::default()
            )
            .is_empty()
    );
}

#[test]
fn snapshots_history_order_when_a_blade_returns_to_the_display_stack() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    let stack = BladeStack::new("blade-returned-to-display-stack-snapshot");
    let navigator_for_ui = Rc::clone(&navigator);
    let mut harness = Harness::new_ui(move |ui| {
        stack.show_with_title(
            ui.ctx(),
            &mut navigator_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            render_test_blade,
        );
    });
    crate::test_support::setup_egui(&mut harness);

    for (id, title) in [(2, "Second"), (3, "Third"), (4, "Fourth")] {
        navigator.borrow_mut().push(TestBlade { id, title });
    }
    navigator.borrow_mut().clear_transition();
    harness.run();

    assert!(navigator.borrow_mut().go_back());
    navigator.borrow_mut().clear_transition();
    harness.run();
    harness.ui_harness("blades/snapshots_history_order_when_a_blade_returns_to_the_display_stack/restored_history_display_stack");
}

#[test]
fn snapshots_history_order_after_crossing_the_display_cap_repeatedly() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    let stack = BladeStack::new("blade-deep-history-cycle-snapshot");
    let navigator_for_ui = Rc::clone(&navigator);
    let mut harness = Harness::new_ui(move |ui| {
        stack.show_with_title(
            ui.ctx(),
            &mut navigator_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            render_test_blade,
        );
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();

    for (id, title) in [
        (2, "Second"),
        (3, "Third"),
        (4, "Fourth"),
        (5, "Fifth"),
        (6, "Sixth"),
    ] {
        navigator.borrow_mut().push(TestBlade { id, title });
        navigator.borrow_mut().clear_transition();
        harness.run();
    }
    for _ in 0..3 {
        assert!(navigator.borrow_mut().go_back());
        navigator.borrow_mut().clear_transition();
        harness.run();
    }
    for _ in 0..2 {
        assert!(navigator.borrow_mut().go_forward());
        navigator.borrow_mut().clear_transition();
        harness.run();
    }

    assert_eq!(navigator.borrow().current().id, 5);
    harness.ui_harness("blades/snapshots_history_order_after_crossing_the_display_cap_repeatedly/deep_history_cycle");
}

#[test]
fn snapshots_an_interrupted_back_to_forward_transition() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    let stack = BladeStack::new("blade-interrupted-transition-snapshot");
    let navigator_for_ui = Rc::clone(&navigator);
    let mut harness = Harness::new_ui(move |ui| {
        stack.show_with_title(
            ui.ctx(),
            &mut navigator_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            render_test_blade,
        );
    });
    crate::test_support::setup_egui(&mut harness);
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 1.0);
    harness.input_mut().time = Some(1.0);
    harness.step();

    for (id, title) in [(2, "Second"), (3, "Third")] {
        navigator.borrow_mut().push(TestBlade { id, title });
        navigator.borrow_mut().clear_transition();
        harness.step();
    }

    assert!(navigator.borrow_mut().go_back());
    harness.input_mut().time = Some(10.0);
    harness.step();
    harness.input_mut().time = Some(10.0 + f64::from(TRANSITION_DURATION / 2.0));
    harness.step();

    assert!(navigator.borrow_mut().go_forward());
    harness.input_mut().time = Some(20.0);
    harness.step();
    harness.input_mut().time = Some(20.0 + f64::from(TRANSITION_DURATION / 2.0));
    harness.step();
    harness.ui_harness(
        "blades/snapshots_an_interrupted_back_to_forward_transition/interrupted_back_to_forward",
    );
}

#[test]
fn snapshots_an_interrupted_forward_to_back_transition() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    let stack = BladeStack::new("blade-interrupted-transition-snapshot");
    let navigator_for_ui = Rc::clone(&navigator);
    let mut harness = Harness::new_ui(move |ui| {
        stack.show_with_title(
            ui.ctx(),
            &mut navigator_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            render_test_blade,
        );
    });
    crate::test_support::setup_egui(&mut harness);
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 1.0);
    harness.input_mut().time = Some(1.0);
    harness.step();

    for (id, title) in [(2, "Second"), (3, "Third")] {
        navigator.borrow_mut().push(TestBlade { id, title });
        navigator.borrow_mut().clear_transition();
        harness.step();
    }

    assert!(navigator.borrow_mut().go_back());
    harness.input_mut().time = Some(10.0);
    harness.step();
    harness.input_mut().time = Some(10.0 + f64::from(TRANSITION_DURATION / 2.0));
    harness.step();

    assert!(navigator.borrow_mut().go_forward());
    harness.input_mut().time = Some(20.0);
    harness.step();
    harness.input_mut().time = Some(20.0 + f64::from(TRANSITION_DURATION / 2.0));
    harness.step();
    assert!(navigator.borrow_mut().go_back());
    harness.input_mut().time = Some(30.0);
    harness.step();
    harness.input_mut().time = Some(30.0 + f64::from(TRANSITION_DURATION / 2.0));
    harness.step();
    harness.ui_harness(
        "blades/snapshots_an_interrupted_forward_to_back_transition/interrupted_forward_to_back",
    );
}

#[test]
fn snapshots_a_reopened_stack_without_stale_layers() {
    let navigator = Rc::new(RefCell::new(Some(BladeNavigator::new(TestBlade {
        id: 1,
        title: "Original",
    }))));
    navigator
        .borrow_mut()
        .as_mut()
        .expect("navigator is open")
        .clear_transition();
    let stack = BladeStack::new("blade-reopened-stack-snapshot");
    let navigator_for_ui = Rc::clone(&navigator);
    let mut harness = Harness::new_ui(move |ui| {
        if let Some(navigator) = navigator_for_ui.borrow_mut().as_mut() {
            stack.show_with_title(
                ui.ctx(),
                navigator,
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        }
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();

    navigator.borrow_mut().take();
    harness.run();

    *navigator.borrow_mut() = Some(BladeNavigator::new(TestBlade {
        id: 2,
        title: "Reopened",
    }));
    navigator
        .borrow_mut()
        .as_mut()
        .expect("navigator was reopened")
        .clear_transition();
    harness.run();
    harness.ui_harness("blades/snapshots_a_reopened_stack_without_stale_layers/reopened_stack");
}

#[test]
fn snapshots_restored_history_after_resizing_the_viewport() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    let stack = BladeStack::new("blade-resized-history-snapshot");
    let navigator_for_ui = Rc::clone(&navigator);
    let mut harness = Harness::new_ui(move |ui| {
        stack.show_with_title(
            ui.ctx(),
            &mut navigator_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            render_test_blade,
        );
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    for (id, title) in [(2, "Second"), (3, "Third"), (4, "Fourth")] {
        navigator.borrow_mut().push(TestBlade { id, title });
        navigator.borrow_mut().clear_transition();
        harness.run();
    }
    assert!(navigator.borrow_mut().go_back());
    navigator.borrow_mut().clear_transition();
    harness.set_size(egui::vec2(1024.0, 768.0));
    harness.run();
    harness.ui_harness(
        "blades/snapshots_restored_history_after_resizing_the_viewport/resized_restored_history",
    );
}
