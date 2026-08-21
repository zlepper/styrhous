use super::super::transforms::{active_transform, history_transform, transformed_rect};
use super::*;

#[test]
fn navigator_restores_entries_and_discards_forward_history() {
    let mut navigator = BladeNavigator::new("one");
    assert!(navigator.push("two").is_empty());
    assert!(navigator.go_back());
    assert_eq!(navigator.current(), &"one");
    assert_eq!(navigator.push("three"), vec!["two"]);
    assert!(!navigator.can_go_forward());
}

#[test]
fn navigator_can_jump_back_multiple_steps() {
    let mut navigator = BladeNavigator::new("one");
    navigator.push("two");
    navigator.push("three");
    navigator.push("four");

    assert!(navigator.go_back_steps(2));
    assert_eq!(navigator.current(), &"two");
    assert_eq!(navigator.back_stack(), &["one"]);
    assert_eq!(navigator.forward_stack(), &["four", "three"]);
    assert_eq!(navigator.transition(), Some(BladeTransition::Back));
    assert_eq!(navigator.back_steps(), 2);

    assert!(!navigator.go_back_steps(0));
    assert!(!navigator.go_back_steps(2));
    assert_eq!(navigator.current(), &"two");
}

#[test]
fn visible_history_blades_are_clickable_without_dismissing_the_stack() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    let dismissed = Rc::new(RefCell::new(false));
    let stack = BladeStack::new("blade-clickable-history");
    let navigator_for_ui = Rc::clone(&navigator);
    let dismissed_for_ui = Rc::clone(&dismissed);
    let mut harness = Harness::new_ui(move |ui| {
        let response = stack.show_with_title(
            ui.ctx(),
            &mut navigator_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            render_test_blade,
        );
        *dismissed_for_ui.borrow_mut() = response.dismissed;
    });
    crate::test_support::setup_egui(&mut harness);
    for (id, title) in [(2, "Second"), (3, "Third"), (4, "Fourth")] {
        navigator.borrow_mut().push(TestBlade { id, title });
        navigator.borrow_mut().clear_transition();
        harness.run();
    }

    harness.get_by_label("Go back two blades").click();
    harness.run();

    assert_eq!(navigator.borrow().current().id, 2);
    assert_eq!(
        navigator
            .borrow()
            .forward_stack()
            .last()
            .map(|blade| blade.id),
        Some(3)
    );
    assert!(!*dismissed.borrow());
}

#[test]
fn clicking_the_nearest_history_blade_goes_back_one_step() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    let stack = BladeStack::new("blade-clickable-nearest-history");
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
        navigator.borrow_mut().clear_transition();
        harness.run();
    }

    harness.get_by_label("Go back one blade").click();
    harness.run();

    assert_eq!(navigator.borrow().current().id, 2);
}

#[test]
fn clicking_overlapping_history_blades_selects_the_topmost_blade() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    let stack = BladeStack::new("blade-overlapping-history-click");
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
        navigator.borrow_mut().clear_transition();
        harness.run();
    }

    let viewport = harness.ctx.content_rect();
    let older = transformed_rect(viewport, history_transform(viewport, 1));
    let nearer = transformed_rect(viewport, history_transform(viewport, 0));
    let active = transformed_rect(viewport, active_transform(viewport));
    let overlap = older.intersect(nearer);
    assert!(overlap.is_positive(), "the history blades should overlap");

    let click_position = egui::pos2((nearer.min.x + active.min.x) / 2.0, overlap.center().y);
    assert!(older.contains(click_position));
    assert!(nearer.contains(click_position));
    assert!(!active.contains(click_position));
    harness.event(egui::Event::PointerMoved(click_position));
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: click_position,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        });
    }
    harness.run();

    assert_eq!(
        navigator.borrow().current().id,
        3,
        "the nearer history blade must win its overlap with the older blade"
    );
}

#[test]
fn clicking_history_under_the_foreground_blade_keeps_the_foreground_active() {
    let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
        id: 1,
        title: "First",
    })));
    navigator.borrow_mut().clear_transition();
    let dismissed = Rc::new(RefCell::new(false));
    let stack = BladeStack::new("blade-foreground-overlap-click");
    let navigator_for_ui = Rc::clone(&navigator);
    let dismissed_for_ui = Rc::clone(&dismissed);
    let mut harness = Harness::new_ui(move |ui| {
        let response = stack.show_with_title(
            ui.ctx(),
            &mut navigator_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            render_test_blade,
        );
        *dismissed_for_ui.borrow_mut() = response.dismissed;
    });
    crate::test_support::setup_egui(&mut harness);
    for (id, title) in [(2, "Second"), (3, "Third"), (4, "Fourth")] {
        navigator.borrow_mut().push(TestBlade { id, title });
        navigator.borrow_mut().clear_transition();
        harness.run();
    }

    let viewport = harness.ctx.content_rect();
    let history = transformed_rect(viewport, history_transform(viewport, 0));
    let active = transformed_rect(viewport, active_transform(viewport));
    let overlap = history.intersect(active);
    assert!(
        overlap.is_positive(),
        "history should extend under the foreground blade"
    );
    let click_position = overlap.center();
    harness.event(egui::Event::PointerMoved(click_position));
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: click_position,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        });
    }
    harness.run();

    assert_eq!(navigator.borrow().current().id, 4);
    assert!(!*dismissed.borrow());
}

#[test]
fn shared_header_controls_navigate_and_close_the_active_blade() {
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
    let close_finished = Rc::new(RefCell::new(false));
    let stack = BladeStack::new("blade-shared-header-controls");
    let navigator_for_ui = Rc::clone(&navigator);
    let close_finished_for_ui = Rc::clone(&close_finished);
    let mut harness = Harness::new_ui(move |ui| {
        let response = stack.show_with_title(
            ui.ctx(),
            &mut navigator_for_ui.borrow_mut(),
            |blade| blade.title.to_owned(),
            render_test_blade,
        );
        *close_finished_for_ui.borrow_mut() = response.close_finished;
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();

    let back = harness.get_by_label("Back").rect();
    let close = harness.get_by_label("Close blade").rect();
    assert_eq!(
        close.right() - back.left(),
        CONTENT_WIDTH,
        "back: {back:?}, close: {close:?}"
    );

    harness.get_by_label("Back").click_accesskit();
    harness.run();
    assert_eq!(navigator.borrow().current().id, 1);

    harness.get_by_label("Forward").click_accesskit();
    harness.run();
    assert_eq!(navigator.borrow().current().id, 2);

    harness.get_by_label("Close blade").click_accesskit();
    harness.run();
    assert!(*close_finished.borrow());
}

#[test]
fn extra_mouse_buttons_navigate_blade_history() {
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
    let stack = BladeStack::new("blade-extra-mouse-navigation");
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

    assert_eq!(navigator.borrow().current().id, 2);
    assert_eq!(navigator.borrow().back_stack().len(), 1);
    assert!(navigator.borrow().forward_stack().is_empty());
}
