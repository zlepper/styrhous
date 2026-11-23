//! The more-actions trigger (C07 `MoreButton`): a small ghost "…" button
//! that opens a [`Menu`] below it, right edges aligned, and the same menu as
//! a row's context menu.

use egui::{Popup, PopupCloseBehavior, RectAlign, Response, SetOpenCommand, Ui};

use crate::design::size;
use crate::design::theme::ThemeExt;
use crate::interaction::ControlState;
use crate::menu::{self, Menu};
use crate::{ButtonSize, ButtonVariant, icons};

/// A compact action-menu trigger: a small ghost icon button (C02 "Ghost
/// sm") that shows its pressed state while its menu is open.
#[must_use = "call `show` to add the button"]
pub struct MoreButton {
    accessibility_label: String,
    forced_state: Option<ControlState>,
}

impl MoreButton {
    /// Create a more-actions trigger with the given accessible label.
    pub fn new(accessibility_label: impl Into<String>) -> Self {
        Self {
            accessibility_label: accessibility_label.into(),
            forced_state: None,
        }
    }

    /// Paint the trigger in `state` regardless of the pointer (boards and
    /// snapshots).
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    /// Show the trigger and its anchored action menu, built by
    /// `add_contents`.
    pub fn show(self, ui: &mut Ui, add_contents: impl FnOnce(&mut Menu<'_>)) -> Response {
        let accessibility_label = self.accessibility_label;
        // The trigger's id, to know whether its menu is open before it is drawn.
        let trigger_id = ui.next_auto_id();
        let open = Popup::is_id_open(ui.ctx(), trigger_id.with("popup"));
        let forced_state = self.forced_state.or(open.then_some(ControlState::Pressed));
        let mut trigger =
            crate::Button::icon(icons::Icon::EllipsisHorizontal, accessibility_label.clone())
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Sm);
        if let Some(state) = forced_state {
            trigger = trigger.force_state(state);
        }
        let trigger = trigger.show(ui);
        ui.ctx()
            .accesskit_node_builder(trigger.id, |node| node.set_expanded(open));

        let popup = Popup::menu(&trigger)
            .align(RectAlign::BOTTOM_END)
            .gap(size::POPOVER_GAP)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside);
        menu::show_popup(popup, &ui.tokens(), &accessibility_label, add_contents);
        trigger
    }

    /// Show the same action menu at the pointer when `response` is
    /// right-clicked (a row's context menu).
    pub fn show_context_menu(response: &Response, add_contents: impl FnOnce(&mut Menu<'_>)) {
        let set_open = if response.secondary_clicked() {
            Some(SetOpenCommand::Bool(true))
        } else if response.clicked() {
            Some(SetOpenCommand::Bool(false))
        } else {
            None
        };
        let popup = Popup::context_menu(response)
            .open_memory(set_open)
            .gap(size::POPOVER_GAP)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside);
        menu::show_popup(popup, &response.ctx.tokens(), "Context", add_contents);
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use crate::test_support::UiHarnessSnapshot;
    use egui_kittest::{Harness, kittest::Queryable};

    use super::*;

    #[test]
    fn more_button_actions_are_accessible_and_close_the_menu() {
        let selected_action = Rc::new(RefCell::new(None));
        let selected_action_in_ui = selected_action.clone();
        let mut harness = Harness::new_ui(move |ui| {
            MoreButton::new("More actions for coredns").show(ui, |menu| {
                if menu.action("Edit").clicked() {
                    *selected_action_in_ui.borrow_mut() = Some("edit");
                }
                menu.separator();
                if menu.destructive_action("Delete").clicked() {
                    *selected_action_in_ui.borrow_mut() = Some("delete");
                }
            });
        });

        crate::test_support::setup_egui(&mut harness);
        harness.run();
        harness.get_by_label("More actions for coredns").click();
        harness.run();
        harness
            .ui_harness("more_button/more_button_actions_are_accessible_and_close_the_menu/open");

        harness.get_by_label("Delete").click();
        harness.run();

        assert_eq!(*selected_action.borrow(), Some("delete"));
        harness.ui_harness(
            "more_button/more_button_actions_are_accessible_and_close_the_menu/closed_after_action",
        );
    }

    #[test]
    fn more_button_supports_accessible_nested_actions() {
        let selected = Rc::new(RefCell::new(None));
        let selected_in_ui = selected.clone();
        let mut harness = Harness::new_ui(move |ui| {
            MoreButton::new("More actions for api-pod").show(ui, |menu| {
                menu.submenu("View logs", |menu| {
                    if menu.action("api — Container").clicked() {
                        *selected_in_ui.borrow_mut() = Some("api");
                    }
                });
            });
        });

        crate::test_support::setup_egui(&mut harness);
        harness.run();
        harness.get_by_label("More actions for api-pod").click();
        harness.run();
        harness.get_by_label("View logs").click();
        harness.run();
        harness.get_by_label("api — Container").click();
        harness.run();

        assert_eq!(*selected.borrow(), Some("api"));
        assert!(harness.query_by_label("api — Container").is_none());

        harness.get_by_label("More actions for api-pod").click();
        harness.run();
        assert!(harness.query_by_label("api — Container").is_none());
    }
}
