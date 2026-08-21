use super::*;

#[test]
fn discarded_forward_history_is_never_rendered_again() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    let rendered = Rc::new(RefCell::new(Vec::new()));
    let stack = BladeStack::new("blade-discarded-forward-history");
    let navigator_for_ui = Rc::clone(&navigator);
    let rendered_for_ui = Rc::clone(&rendered);
    let mut harness = Harness::new_ui(move |ui| {
        stack.show_with_title(
            ui.ctx(),
            &mut navigator_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            |ui, blade, layer| {
                rendered_for_ui.borrow_mut().push(blade.id);
                render_test_blade(ui, blade, layer);
            },
        );
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    for (id, title) in [(2, "Second"), (3, "Third")] {
        navigator.borrow_mut().push(TestBlade { id, title });
        navigator.borrow_mut().clear_transition();
        harness.run();
    }
    assert!(navigator.borrow_mut().go_back());
    navigator.borrow_mut().clear_transition();
    harness.run();
    assert_eq!(
        navigator
            .borrow()
            .forward_stack()
            .last()
            .map(|blade| blade.id),
        Some(3)
    );

    let discarded = navigator.borrow_mut().push(TestBlade {
        id: 4,
        title: "Replacement",
    });
    assert_eq!(discarded.len(), 1);
    assert_eq!(discarded[0].id, 3);
    navigator.borrow_mut().clear_transition();
    rendered.borrow_mut().clear();
    harness.run();

    assert!(
        !rendered.borrow().contains(&3),
        "discarded forward history must not remain in the display stack"
    );
}

#[test]
fn snapshots_the_most_recently_rendered_stack_above_other_stacks() {
    let first_navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First stack",
    })));
    let second_navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 2,
        title: "Second stack",
    })));
    first_navigator.borrow_mut().clear_transition();
    second_navigator.borrow_mut().clear_transition();
    let first_stack = BladeStack::new("first-concurrent-blade-stack");
    let second_stack = BladeStack::new("second-concurrent-blade-stack");
    let first_for_ui = Rc::clone(&first_navigator);
    let second_for_ui = Rc::clone(&second_navigator);
    let mut harness = Harness::new_ui(move |ui| {
        first_stack.show_with_title(
            ui.ctx(),
            &mut first_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            render_test_blade,
        );
        second_stack.show_with_title(
            ui.ctx(),
            &mut second_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            render_test_blade,
        );
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.ui_harness(
        "blades/snapshots_the_most_recently_rendered_stack_above_other_stacks/concurrent_stacks",
    );
}

#[test]
fn restored_history_keeps_focus_and_keyboard_navigation_on_the_active_blade() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    let stack = BladeStack::new("blade-restored-history-accessibility");
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
    harness.run();

    assert_eq!(
        harness.get_all_by_label("Back in background blade").count(),
        2,
        "the restored history blades must not expose foreground controls"
    );
    harness.get_by_label("Back").focus();
    harness.run();
    assert!(harness.get_by_label("Back").is_focused());

    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(navigator.borrow().current().id, 2);
}

#[test]
fn snapshots_opening_and_forward_animation_frames() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    let stack = BladeStack::new("blade-animation-snapshot");
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

    // Harness construction renders once before the test can configure the
    // clock. Restart this transition so the frames below use only our
    // explicit timestamps.
    {
        let mut navigator = navigator.borrow_mut();
        navigator.transition = Some(BladeTransition::Opening);
        navigator.transition_started_at = None;
    }
    harness.input_mut().time = Some(1.0);
    harness.step();
    harness.ui_harness("blades/snapshots_opening_and_forward_animation_frames/opening_first_frame");
    harness.input_mut().time = Some(1.0 + f64::from(TRANSITION_DURATION / 2.0));
    harness.step();
    harness.ui_harness("blades/snapshots_opening_and_forward_animation_frames/opening_mid_frame");
    harness.input_mut().time = Some(1.0 + f64::from(TRANSITION_DURATION));
    harness.step();
    harness.ui_harness("blades/snapshots_opening_and_forward_animation_frames/opening_final_frame");

    navigator.borrow_mut().push(TestBlade {
        id: 2,
        title: "Second",
    });
    harness.input_mut().time = Some(20.0);
    harness.step();
    harness.ui_harness("blades/snapshots_opening_and_forward_animation_frames/forward_first_frame");
    harness.input_mut().time = Some(20.0 + f64::from(TRANSITION_DURATION / 2.0));
    harness.step();
    harness.ui_harness("blades/snapshots_opening_and_forward_animation_frames/forward_mid_frame");

    assert!(navigator.borrow_mut().go_back());
    harness.input_mut().time = Some(30.0);
    harness.step();
    harness.ui_harness("blades/snapshots_opening_and_forward_animation_frames/back_first_frame");
    harness.input_mut().time = Some(30.0 + f64::from(TRANSITION_DURATION / 2.0));
    harness.step();
    harness.ui_harness("blades/snapshots_opening_and_forward_animation_frames/back_mid_frame");
}

#[test]
fn snapshots_history_overflow_delayed_removal_and_direct_two_step_back_animation() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    let stack = BladeStack::new("blade-history-overflow-animation");
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

    navigator.borrow_mut().push(TestBlade {
        id: 4,
        title: "Fourth",
    });
    harness.input_mut().time = Some(10.0);
    harness.step();
    harness.ui_harness("blades/snapshots_history_overflow_delayed_removal_and_direct_two_step_back_animation/history_overflow_first_frame");
    // The capped history blade remains fully visible until the other
    // history layers have completed their transition.
    harness.input_mut().time = Some(10.0 + f64::from(TRANSITION_DURATION / 2.0));
    harness.step();
    harness.ui_harness("blades/snapshots_history_overflow_delayed_removal_and_direct_two_step_back_animation/history_overflow_mid_frame");
    harness.input_mut().time = Some(10.0 + f64::from(TRANSITION_DURATION));
    harness.step();
    harness.ui_harness("blades/snapshots_history_overflow_delayed_removal_and_direct_two_step_back_animation/history_overflow_final_frame");

    assert!(navigator.borrow_mut().go_back_steps(2));
    harness.input_mut().time = Some(20.0);
    harness.step();
    harness.ui_harness("blades/snapshots_history_overflow_delayed_removal_and_direct_two_step_back_animation/direct_two_step_back_first_frame");
    harness.input_mut().time = Some(20.0 + f64::from(TRANSITION_DURATION / 2.0));
    harness.step();
    harness.ui_harness("blades/snapshots_history_overflow_delayed_removal_and_direct_two_step_back_animation/direct_two_step_back_mid_frame");
}

#[test]
fn snapshots_custom_header_content_with_shared_controls() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    let stack = BladeStack::new("blade-custom-header-snapshot");
    let navigator_for_ui = Rc::clone(&navigator);
    let mut harness = Harness::new_ui(move |ui| {
        stack.show(
            ui.ctx(),
            &mut navigator_for_ui.borrow_mut(),
            |ui, blade, _| {
                ui.label(egui::RichText::new(format!("Custom: {}", blade.title)).strong());
            },
            render_test_blade,
        );
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.ui_harness("blades/snapshots_custom_header_content_with_shared_controls/custom_header");
}

#[test]
fn only_the_two_most_recent_history_blades_are_rendered() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    let rendered = Rc::new(RefCell::new(Vec::new()));
    let stack = BladeStack::new("blade-history-cap");
    let navigator_for_ui = Rc::clone(&navigator);
    let rendered_for_ui = Rc::clone(&rendered);
    let mut harness = Harness::new_ui(move |ui| {
        stack.show_with_title(
            ui.ctx(),
            &mut navigator_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            |ui, blade, layer| {
                rendered_for_ui.borrow_mut().push(blade.id);
                render_test_blade(ui, blade, layer);
            },
        );
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    for (id, title) in [(2, "Second"), (3, "Third"), (4, "Fourth")] {
        navigator.borrow_mut().push(TestBlade { id, title });
    }
    navigator.borrow_mut().clear_transition();
    rendered.borrow_mut().clear();
    harness.run();

    let rendered = rendered.borrow();
    assert!(!rendered.contains(&1), "the oldest blade must be hidden");
    assert!(
        rendered.chunks_exact(3).all(|frame| frame == [2, 3, 4]),
        "only the two newest history blades and the active blade should render: {rendered:?}"
    );
}

#[test]
fn stable_content_ids_preserve_child_state_through_history_navigation() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    let initialized = Rc::new(RefCell::new(Vec::new()));
    let stack = BladeStack::new("blade-child-state");
    let navigator_for_ui = Rc::clone(&navigator);
    let initialized_for_ui = Rc::clone(&initialized);
    let mut harness = Harness::new_ui(move |ui| {
        stack.show_with_title(
            ui.ctx(),
            &mut navigator_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            |ui, blade, layer| {
                let state_id = layer.content_id.with("child-state");
                if !ui
                    .ctx()
                    .data(|data| data.get_temp::<bool>(state_id).unwrap_or(false))
                {
                    ui.ctx().data_mut(|data| data.insert_temp(state_id, true));
                    initialized_for_ui.borrow_mut().push(blade.id);
                }
                render_test_blade(ui, blade, layer);
            },
        );
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    navigator.borrow_mut().push(TestBlade {
        id: 2,
        title: "Second",
    });
    navigator.borrow_mut().clear_transition();
    harness.run();
    assert!(navigator.borrow_mut().go_back());
    navigator.borrow_mut().clear_transition();
    harness.run();
    assert!(navigator.borrow_mut().go_forward());
    navigator.borrow_mut().clear_transition();
    harness.run();

    assert_eq!(&*initialized.borrow(), &[1, 2]);
}

#[test]
fn content_ids_are_synthesized_from_stack_positions() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    let stack_source = "blade-stack-position-content-ids";
    let stack = BladeStack::new(stack_source);
    let rendered = Rc::new(RefCell::new(Vec::new()));
    let navigator_for_ui = Rc::clone(&navigator);
    let rendered_for_ui = Rc::clone(&rendered);
    let mut harness = Harness::new_ui(move |ui| {
        stack.show_with_title(
            ui.ctx(),
            &mut navigator_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            |ui, blade, layer| {
                rendered_for_ui
                    .borrow_mut()
                    .push((blade.id, layer.content_id));
                render_test_blade(ui, blade, layer);
            },
        );
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    navigator.borrow_mut().push(TestBlade {
        id: 2,
        title: "Second",
    });
    navigator.borrow_mut().push(TestBlade {
        id: 3,
        title: "Third",
    });
    navigator.borrow_mut().clear_transition();
    rendered.borrow_mut().clear();
    harness.run();

    let expected = [
        (1, Id::new(stack_source).with(("blade-content", 0))),
        (2, Id::new(stack_source).with(("blade-content", 1))),
        (3, Id::new(stack_source).with(("blade-content", 2))),
    ];
    assert!(
        rendered
            .borrow()
            .chunks_exact(expected.len())
            .all(|frame| frame == expected),
        "content IDs should be derived from each blade's stack position: {rendered:?}"
    );
}

#[test]
fn hidden_history_blades_restore_their_existing_child_state() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    let initialized = Rc::new(RefCell::new(Vec::new()));
    let stack = BladeStack::new("blade-hidden-child-state");
    let navigator_for_ui = Rc::clone(&navigator);
    let initialized_for_ui = Rc::clone(&initialized);
    let mut harness = Harness::new_ui(move |ui| {
        stack.show_with_title(
            ui.ctx(),
            &mut navigator_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            |ui, blade, layer| {
                let state_id = layer.content_id.with("child-state");
                if !ui
                    .ctx()
                    .data(|data| data.get_temp::<bool>(state_id).unwrap_or(false))
                {
                    ui.ctx().data_mut(|data| data.insert_temp(state_id, true));
                    initialized_for_ui.borrow_mut().push(blade.id);
                }
                render_test_blade(ui, blade, layer);
            },
        );
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();

    for (id, title) in [(2, "Second"), (3, "Third"), (4, "Fourth")] {
        navigator.borrow_mut().push(TestBlade { id, title });
        navigator.borrow_mut().clear_transition();
        harness.run();
    }
    assert_eq!(navigator.borrow().current().id, 4);
    assert_eq!(&*initialized.borrow(), &[1, 2, 3, 4]);

    for _ in 0..3 {
        assert!(navigator.borrow_mut().go_back());
        navigator.borrow_mut().clear_transition();
        harness.run();
    }

    assert_eq!(navigator.borrow().current().id, 1);
    assert_eq!(
        &*initialized.borrow(),
        &[1, 2, 3, 4],
        "returning to a hidden entry must use its original egui content id"
    );
}
