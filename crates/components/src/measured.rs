//! Widths and heights that are only known after a widget is laid out.
//!
//! egui lays a widget out before it knows its size, so a row that centres or
//! right-aligns its content uses the width measured on the previous pass and
//! asks for the frame to be laid out again when the measurement changes. A
//! column that centres or sizes itself by its height does the same with the
//! height.

use egui::{Id, Ui};

/// A change in width smaller than this (in points) is not a change.
const WIDTH_EPSILON: f32 = 0.01;

/// The width stored under `id` by [`remember_width`] on an earlier pass.
pub fn remembered_width(ui: &Ui, id: Id) -> Option<f32> {
    ui.data(|data| data.get_temp::<f32>(id))
}

/// Store `measured` under `id` for the next pass. When it differs from the
/// remembered width the pass is discarded and laid out again (`reason` names
/// the cause for egui's debug output). Returns whether the width changed.
pub fn remember_width(ui: &Ui, id: Id, measured: f32, reason: &'static str) -> bool {
    let remembered = remembered_width(ui, id).unwrap_or(0.0);
    let changed = (measured - remembered).abs() > WIDTH_EPSILON;
    if changed {
        ui.data_mut(|data| data.insert_temp(id, measured));
        ui.ctx().request_discard(reason);
    }
    changed
}

/// The height stored under `id` by [`remember_height`] on an earlier pass.
pub fn remembered_height(ui: &Ui, id: Id) -> Option<f32> {
    remembered_width(ui, id)
}

/// [`remember_width`] for a height.
pub fn remember_height(ui: &Ui, id: Id, measured: f32, reason: &'static str) -> bool {
    remember_width(ui, id, measured, reason)
}

/// Lay `add_contents` out with the content height measured on the previous
/// pass (`estimate` on the first, so it only has to be close once).
/// `add_contents` gets that height and returns its result with the height it
/// actually used, which is remembered under `id`; a change lays the frame out
/// again.
pub fn with_measured_height<R>(
    ui: &mut Ui,
    id: Id,
    estimate: f32,
    reason: &'static str,
    add_contents: impl FnOnce(&mut Ui, f32) -> (R, f32),
) -> R {
    let height = remembered_height(ui, id).unwrap_or(estimate);
    let (result, measured) = add_contents(ui, height);
    remember_height(ui, id, measured, reason);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn a_new_width_discards_the_pass_and_a_stable_one_does_not() {
        let id = Id::new("measured-test");
        let measured = Rc::new(RefCell::new(10.0_f32));
        let seen = Rc::new(RefCell::new(Vec::new()));
        let (width, log) = (Rc::clone(&measured), Rc::clone(&seen));
        let mut harness = egui_kittest::Harness::new_ui(move |ui| {
            let measured = *width.borrow();
            log.borrow_mut().push((
                remembered_width(ui, id),
                remember_width(ui, id, measured, "test"),
            ));
        });
        harness.run();
        // The first pass is discarded; the passes after it read the width and are stable.
        assert_eq!(seen.borrow()[..2], [(None, true), (Some(10.0), false)]);
        assert!(
            seen.borrow()[1..]
                .iter()
                .all(|pass| *pass == (Some(10.0), false))
        );

        seen.borrow_mut().clear();
        *measured.borrow_mut() = 10.005;
        harness.run();
        assert_eq!(
            *seen.borrow(),
            [(Some(10.0), false)],
            "jitter is not a change"
        );

        seen.borrow_mut().clear();
        *measured.borrow_mut() = 12.0;
        harness.run();
        assert_eq!(
            seen.borrow()[..2],
            [(Some(10.0), true), (Some(12.0), false)]
        );
    }

    #[test]
    fn measured_height_starts_from_the_estimate_and_then_follows_the_content() {
        let id = Id::new("measured-height-test");
        let content = Rc::new(RefCell::new(40.0_f32));
        let seen = Rc::new(RefCell::new(Vec::new()));
        let (height, log) = (Rc::clone(&content), Rc::clone(&seen));
        let mut harness = egui_kittest::Harness::new_ui(move |ui| {
            let used = *height.borrow();
            with_measured_height(ui, id, 100.0, "test", |_, given| {
                log.borrow_mut().push(given);
                ((), used)
            });
        });
        harness.run();
        assert_eq!(seen.borrow()[..2], [100.0, 40.0]);
        assert!(seen.borrow()[1..].iter().all(|given| *given == 40.0));

        seen.borrow_mut().clear();
        *content.borrow_mut() = 60.0;
        harness.run();
        assert_eq!(seen.borrow()[..2], [40.0, 60.0]);
    }
}
