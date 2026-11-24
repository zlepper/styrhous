//! The colour swatches (A09): the chosen swatch wears the focus ring and a
//! click reports the swatch hit.

use std::cell::Cell;
use std::rc::Rc;

use components::Swatches;
use components::design::space;
use components::design::theme::ThemeExt;
use components::test_support::{UiHarnessSnapshot, canvas_harness};
use egui_kittest::kittest::{NodeT, Queryable};

#[test]
fn swatches_ring_the_chosen_colour_and_report_clicks() {
    let selected = Rc::new(Cell::new(Some(4)));
    let shown = Rc::clone(&selected);
    let mut harness = canvas_harness(220.0, 64.0, Some(space::S16), move |ui| {
        let colours = ui.tokens().components.avatars();
        if let Some(clicked) = Swatches::new(&colours, shown.get(), "Colour").show(ui) {
            shown.set(Some(clicked));
        }
    });
    harness.run();
    harness.ui_harness("swatches/chosen_colour_ringed");

    harness.get_by_label("Colour 2").click();
    harness.run();
    assert_eq!(selected.get(), Some(1));
    harness.ui_harness("swatches/clicked_colour_ringed");
}

/// The swatches are one radio group: a single tab stop, and the arrow keys
/// move both the choice and the focus.
#[test]
fn swatches_are_one_tab_stop_and_move_with_the_arrow_keys() {
    let selected = Rc::new(Cell::new(Some(2)));
    let shown = Rc::clone(&selected);
    let mut harness = canvas_harness(220.0, 64.0, Some(space::S16), move |ui| {
        let colours = ui.tokens().components.avatars();
        if let Some(clicked) = Swatches::new(&colours, shown.get(), "Colour").show(ui) {
            shown.set(Some(clicked));
        }
    });
    harness.run();
    // Tab lands on the chosen swatch.
    harness.key_press(egui::Key::Tab);
    harness.run();
    assert!(
        harness
            .get_by_label("Colour 3")
            .accesskit_node()
            .is_focused()
    );
    harness.key_press(egui::Key::ArrowRight);
    harness.run();
    assert_eq!(selected.get(), Some(3));
    harness.run();
    assert!(
        harness
            .get_by_label("Colour 4")
            .accesskit_node()
            .is_focused()
    );
    harness.key_press(egui::Key::ArrowLeft);
    harness.key_press(egui::Key::ArrowLeft);
    harness.run();
    assert_eq!(selected.get(), Some(1));
    harness.key_press(egui::Key::Tab);
    harness.run();
    // With nothing else to focus, Tab comes back to the one tab stop: the chosen swatch.
    assert!(
        harness
            .get_by_label("Colour 2")
            .accesskit_node()
            .is_focused()
    );
}
