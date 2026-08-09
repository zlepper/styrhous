//! A compact, Tailwind-inspired action menu for table rows and other dense UI.

use egui::{
    Button, Color32, Frame, Image, InnerResponse, Margin, Popup, PopupCloseBehavior, Response,
    SetOpenCommand, Shadow, Stroke, Ui, Vec2,
};

use crate::design::{radius, spacing, status, typography};
use crate::{
    ButtonSize, ButtonVariant, TailwindButton,
    colors::{BLACK, WHITE, gray},
    icons,
};

const MENU_WIDTH: f32 = 192.0;
const MENU_ITEM_HEIGHT: f32 = 32.0;
const MENU_ITEM_PADDING: Vec2 = Vec2::new(spacing::MD, spacing::SM);
const DESTRUCTIVE: Color32 = status::DANGER;

/// A compact action-menu trigger styled as a secondary icon-only button.
pub struct MoreButton {
    accessibility_label: String,
}

impl MoreButton {
    /// Create a more-actions trigger with the given accessible label.
    pub fn new(accessibility_label: impl Into<String>) -> Self {
        Self {
            accessibility_label: accessibility_label.into(),
        }
    }

    /// Show the trigger and its anchored action menu.
    ///
    /// The callback receives a [`MoreMenu`], whose [`MoreMenu::action`],
    /// [`MoreMenu::destructive_action`], and [`MoreMenu::separator`] methods
    /// build the menu contents.
    pub fn show(self, ui: &mut Ui, add_contents: impl FnOnce(&mut MoreMenu<'_>)) -> Response {
        let trigger = TailwindButton::icon(
            icons::ellipsis_horizontal_icon()
                .fit_to_exact_size(Vec2::splat(16.0))
                .tint(gray::_700),
        )
        .variant(ButtonVariant::Secondary)
        .size(ButtonSize::Md)
        .accessibility_label(self.accessibility_label)
        .show(ui);

        let popup_id = Popup::default_response_id(&trigger);
        let _popup = Popup::menu(&trigger)
            .align(egui::RectAlign::BOTTOM_END)
            .width(MENU_WIDTH)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            .frame(menu_frame())
            .show(|ui| {
                let mut menu = MoreMenu { ui, popup_id };
                add_contents(&mut menu);
            });

        trigger
    }

    /// Show the same action menu when an arbitrary response is right-clicked.
    ///
    /// The popup is anchored below the cursor, which makes it suitable for
    /// row-level context menus.
    pub fn show_context_menu(response: &Response, add_contents: impl FnOnce(&mut MoreMenu<'_>)) {
        let popup_id = Popup::default_response_id(response);
        let secondary_clicked_in_response = response.ctx.input(|input| {
            input.pointer.button_clicked(egui::PointerButton::Secondary)
                && input
                    .pointer
                    .latest_pos()
                    .is_some_and(|position| response.interact_rect.contains(position))
        });
        let set_open = if secondary_clicked_in_response {
            Some(SetOpenCommand::Bool(true))
        } else if response.clicked() {
            Some(SetOpenCommand::Bool(false))
        } else {
            None
        };
        let _popup = Popup::context_menu(response)
            .open_memory(set_open)
            .gap(4.0)
            .width(MENU_WIDTH)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            .frame(menu_frame())
            .show(|ui| {
                let mut menu = MoreMenu { ui, popup_id };
                add_contents(&mut menu);
            });
    }
}

/// Builder passed to [`MoreButton::show`] for adding menu content.
pub struct MoreMenu<'a> {
    ui: &'a mut Ui,
    popup_id: egui::Id,
}

impl MoreMenu<'_> {
    /// Add a standard action and return its response.
    pub fn action(&mut self, label: impl Into<String>) -> Response {
        self.menu_button(label.into(), None, gray::_700)
    }

    /// Add a standard action with a leading icon and return its response.
    pub fn action_with_icon(&mut self, label: impl Into<String>, icon: Image<'static>) -> Response {
        self.menu_button(label.into(), Some(icon), gray::_700)
    }

    /// Add a nested menu for actions that require a further choice.
    pub fn submenu<R>(
        &mut self,
        label: impl Into<egui::WidgetText>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<Option<R>> {
        self.ui.menu_button(label, add_contents)
    }

    /// Close the containing menu after a nested action has been selected.
    pub fn close(&self) {
        Popup::close_id(self.ui.ctx(), self.popup_id);
    }

    /// Add a destructive action and return its response.
    pub fn destructive_action(&mut self, label: impl Into<String>) -> Response {
        self.menu_button(label.into(), None, DESTRUCTIVE)
    }

    /// Add a destructive action with a leading icon and return its response.
    pub fn destructive_action_with_icon(
        &mut self,
        label: impl Into<String>,
        icon: Image<'static>,
    ) -> Response {
        self.menu_button(label.into(), Some(icon), DESTRUCTIVE)
    }

    /// Add a horizontal rule between related action groups.
    pub fn separator(&mut self) {
        self.ui.add_space(3.0);
        let width = self.ui.available_width();
        let (_, rect) = self.ui.allocate_space(Vec2::new(width, 1.0));
        self.ui.painter().line_segment(
            [rect.left_center(), rect.right_center()],
            Stroke::new(1.0, gray::_200),
        );
        self.ui.add_space(3.0);
    }

    fn menu_button(
        &mut self,
        label: String,
        icon: Option<Image<'static>>,
        color: Color32,
    ) -> Response {
        let saved_widgets = self.ui.visuals().widgets.clone();
        let saved_padding = self.ui.spacing().button_padding;

        let visuals = &mut self.ui.visuals_mut().widgets;
        visuals.inactive.weak_bg_fill = WHITE;
        visuals.inactive.bg_fill = WHITE;
        visuals.inactive.bg_stroke = Stroke::NONE;
        visuals.inactive.fg_stroke = Stroke::new(1.0, color);
        visuals.inactive.corner_radius = radius::control();
        visuals.hovered.weak_bg_fill = gray::_100;
        visuals.hovered.bg_fill = gray::_100;
        visuals.hovered.bg_stroke = Stroke::NONE;
        visuals.hovered.fg_stroke = Stroke::new(1.0, color);
        visuals.hovered.corner_radius = radius::control();
        visuals.active.weak_bg_fill = gray::_200;
        visuals.active.bg_fill = gray::_200;
        visuals.active.bg_stroke = Stroke::NONE;
        visuals.active.fg_stroke = Stroke::new(1.0, color);
        visuals.active.corner_radius = radius::control();
        self.ui.spacing_mut().button_padding = MENU_ITEM_PADDING;

        let text = egui::RichText::new(label)
            .font(typography::body())
            .color(color);
        let button = match icon {
            Some(icon) => Button::image_and_text(icon, text),
            None => Button::new(text),
        }
        .min_size(Vec2::new(MENU_WIDTH, MENU_ITEM_HEIGHT));
        let response = self.ui.add(button);

        self.ui.visuals_mut().widgets = saved_widgets;
        self.ui.spacing_mut().button_padding = saved_padding;

        if response.clicked() {
            Popup::close_id(self.ui.ctx(), self.popup_id);
        }
        response
    }
}

fn menu_frame() -> Frame {
    Frame::new()
        .fill(WHITE)
        .stroke(Stroke::new(1.0, gray::_200))
        .corner_radius(radius::surface())
        .inner_margin(Margin::same(spacing::XS as i8))
        .shadow(Shadow {
            offset: [0, 2],
            blur: 8,
            spread: 0,
            color: BLACK.gamma_multiply(0.12),
        })
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use egui_kittest::{Harness, kittest::Queryable};

    use super::*;

    #[test]
    fn more_button_actions_are_accessible_and_close_the_menu() {
        let selected_action = Rc::new(RefCell::new(None));
        let selected_action_in_ui = selected_action.clone();
        let mut harness = Harness::new_ui(move |ui| {
            MoreButton::new("More actions for coredns").show(ui, |menu| {
                if menu.action("Edit YAML").clicked() {
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
        harness
            .get_by_label("More actions for coredns")
            .click_accesskit();
        harness.run();
        harness.snapshot("more_button/open");

        harness.get_by_label("Delete").click_accesskit();
        harness.run();

        assert_eq!(*selected_action.borrow(), Some("delete"));
        harness.snapshot("more_button/closed_after_action");
    }

    #[test]
    fn more_button_supports_accessible_nested_actions() {
        let selected = Rc::new(RefCell::new(None));
        let selected_in_ui = selected.clone();
        let mut harness = Harness::new_ui(move |ui| {
            MoreButton::new("More actions for api-pod").show(ui, |menu| {
                menu.submenu("View logs", |ui| {
                    if ui.button("api — Container").clicked() {
                        *selected_in_ui.borrow_mut() = Some("api");
                    }
                });
            });
        });

        crate::test_support::setup_egui(&mut harness);
        harness.run();
        harness
            .get_by_label("More actions for api-pod")
            .click_accesskit();
        harness.run();
        harness.get_by_label("View logs ⏵").click_accesskit();
        harness.run();
        harness.get_by_label("api — Container").click_accesskit();
        harness.run();

        assert_eq!(*selected.borrow(), Some("api"));
    }
}
