use super::*;

#[test]
fn extra_mouse_buttons_do_not_navigate_without_available_history() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "Only",
    })));
    navigator.borrow_mut().clear_transition();
    let stack = BladeStack::new("blade-extra-mouse-navigation-unavailable");
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

    for button in [egui::PointerButton::Extra1, egui::PointerButton::Extra2] {
        harness.event(egui::Event::PointerButton {
            pos: egui::pos2(0.0, 0.0),
            button,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
        harness.run();
    }

    assert_eq!(navigator.borrow().current().id, 1);
    assert!(navigator.borrow().back_stack().is_empty());
    assert!(navigator.borrow().forward_stack().is_empty());
}

#[test]
fn extra_mouse_buttons_immediately_replace_blade_transitions() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    navigator.borrow_mut().push(TestBlade {
        id: 2,
        title: "Second",
    });
    let stack = BladeStack::new("blade-extra-mouse-navigation-transition");
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
    // `Harness::new_ui` renders once before the configured test clock.
    // Restart this transition so the next frame is its first animation frame.
    {
        let mut navigator = navigator.borrow_mut();
        navigator.transition = Some(BladeTransition::Forward);
        navigator.transition_started_at = None;
    }
    harness.input_mut().time = Some(1.0);
    harness.event(egui::Event::PointerButton {
        pos: egui::pos2(0.0, 0.0),
        button: egui::PointerButton::Extra1,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.step();
    assert_eq!(navigator.borrow().current().id, 2);

    harness.input_mut().time = Some(1.0 + f64::from(TRANSITION_DURATION));
    harness.step();
    assert_eq!(navigator.borrow().current().id, 1);
}

#[test]
fn extra_mouse_buttons_navigate_only_the_topmost_blade_stack() {
    let background = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "Background first",
    })));
    let foreground = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 3,
        title: "Foreground first",
    })));
    for (navigator, next) in [
        (
            &background,
            TestBlade {
                id: 2,
                title: "Background second",
            },
        ),
        (
            &foreground,
            TestBlade {
                id: 4,
                title: "Foreground second",
            },
        ),
    ] {
        navigator.borrow_mut().clear_transition();
        navigator.borrow_mut().push(next);
        navigator.borrow_mut().clear_transition();
    }
    let background_stack = BladeStack::new("background-blade-stack");
    let foreground_stack = BladeStack::new("foreground-blade-stack");
    let background_for_ui = Rc::clone(&background);
    let foreground_for_ui = Rc::clone(&foreground);
    let mut harness = Harness::new_ui(move |ui| {
        background_stack.show_with_title(
            ui.ctx(),
            &mut background_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            render_test_blade,
        );
        foreground_stack.show_with_title(
            ui.ctx(),
            &mut foreground_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            render_test_blade,
        );
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();

    harness.event(egui::Event::PointerButton {
        pos: egui::pos2(0.0, 0.0),
        button: egui::PointerButton::Extra1,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();

    assert_eq!(background.borrow().current().id, 2);
    assert_eq!(foreground.borrow().current().id, 3);
}

#[test]
fn extra_mouse_buttons_navigate_the_remaining_stack_after_the_foreground_closes() {
    let background = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "Background first",
    })));
    background.borrow_mut().clear_transition();
    background.borrow_mut().push(TestBlade {
        id: 2,
        title: "Background second",
    });
    background.borrow_mut().clear_transition();
    let foreground = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 3,
        title: "Foreground",
    })));
    foreground.borrow_mut().clear_transition();
    let show_foreground = Rc::new(RefCell::new(true));
    let background_stack = BladeStack::new("remaining-background-blade-stack");
    let foreground_stack = BladeStack::new("removed-foreground-blade-stack");
    let background_for_ui = Rc::clone(&background);
    let foreground_for_ui = Rc::clone(&foreground);
    let show_foreground_for_ui = Rc::clone(&show_foreground);
    let mut harness = Harness::new_ui(move |ui| {
        background_stack.show_with_title(
            ui.ctx(),
            &mut background_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            render_test_blade,
        );
        if *show_foreground_for_ui.borrow() {
            foreground_stack.show_with_title(
                ui.ctx(),
                &mut foreground_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        }
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();

    *show_foreground.borrow_mut() = false;
    harness.event(egui::Event::PointerButton {
        pos: egui::pos2(0.0, 0.0),
        button: egui::PointerButton::Extra1,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();

    assert_eq!(background.borrow().current().id, 1);
}

#[test]
fn extra_mouse_buttons_are_ignored_while_a_blade_is_closing() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    navigator.borrow_mut().push(TestBlade {
        id: 2,
        title: "Second",
    });
    navigator.borrow_mut().clear_transition();
    let stack = BladeStack::new("blade-extra-mouse-navigation-closing");
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

    assert!(navigator.borrow_mut().begin_close());
    harness.event(egui::Event::PointerButton {
        pos: egui::pos2(0.0, 0.0),
        button: egui::PointerButton::Extra1,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();

    assert_eq!(navigator.borrow().current().id, 2);
    assert_eq!(navigator.borrow().back_stack().len(), 1);
    assert!(navigator.borrow().forward_stack().is_empty());
}
#[test]
fn closing_is_idempotent() {
    let mut navigator = BladeNavigator::new(());
    assert!(navigator.begin_close());
    assert!(!navigator.begin_close());
}

#[test]
fn exposes_the_shared_blade_width() {
    assert_eq!(BladeStack::new("blade-width").width(), BLADE_WIDTH);
}

#[test]
fn body_receives_the_fixed_blade_content_width() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    let stack = BladeStack::new("blade-content-width");
    let navigator_for_ui = Rc::clone(&navigator);
    let observed_width = Rc::new(RefCell::new(None));
    let observed_width_for_ui = Rc::clone(&observed_width);
    let mut harness = Harness::new_ui(move |ui| {
        stack.show_with_title(
            ui.ctx(),
            &mut navigator_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            |ui, _, _| {
                *observed_width_for_ui.borrow_mut() = Some(ui.available_width());
            },
        );
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();

    assert_eq!(*observed_width.borrow(), Some(CONTENT_WIDTH));
}
