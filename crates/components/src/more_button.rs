//! A compact, Tailwind-inspired action menu for table rows and other dense UI.

use egui::{
    Button, Color32, Image, InnerResponse, Popup, PopupCloseBehavior, Response, SetOpenCommand,
    Stroke, Ui, Vec2,
};

use crate::design::theme::{self, ThemeExt, ThemeScope};
use crate::design::{radius, space, stroke, type_scale};
use crate::frames::popover_frame;
use crate::{ButtonSize, ButtonVariant, PointingHand, TailwindButton, icons};

const MENU_WIDTH: f32 = 192.0;
const MENU_ITEM_HEIGHT: f32 = 32.0;
const MENU_ITEM_PADDING: Vec2 = Vec2::new(space::S12, space::S8);

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
        let accessibility_label = self.accessibility_label;
        let trigger = TailwindButton::icon(
            icons::ellipsis_horizontal_icon()
                .fit_to_exact_size(Vec2::splat(16.0))
                .tint(ui.tokens().text.primary),
        )
        .variant(ButtonVariant::Secondary)
        .size(ButtonSize::Md)
        .accessibility_label(accessibility_label.clone())
        .show(ui);

        let popup_id = Popup::default_response_id(&trigger);
        if let Some(popup) = Popup::menu(&trigger)
            .align(egui::RectAlign::BOTTOM_END)
            .width(MENU_WIDTH)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            .frame(popover_frame(&ui.tokens()))
            .show(|ui| {
                theme::scope(ui, ThemeScope::Popover, |ui| {
                    let mut menu = MoreMenu { ui, popup_id };
                    add_contents(&mut menu);
                })
            })
        {
            ui.ctx()
                .accesskit_node_builder(popup.response.id, |builder| {
                    builder.set_label(format!("{accessibility_label} menu"));
                });
        }

        trigger
    }

    /// Show the same action menu when an arbitrary response is right-clicked.
    ///
    /// The popup is anchored below the cursor, which makes it suitable for
    /// row-level context menus.
    pub fn show_context_menu(response: &Response, add_contents: impl FnOnce(&mut MoreMenu<'_>)) {
        let popup_id = Popup::default_response_id(response);
        let set_open = if response.secondary_clicked() {
            Some(SetOpenCommand::Bool(true))
        } else if response.clicked() {
            Some(SetOpenCommand::Bool(false))
        } else {
            None
        };
        if let Some(popup) = Popup::context_menu(response)
            .open_memory(set_open)
            .gap(4.0)
            .width(MENU_WIDTH)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            .frame(popover_frame(&response.ctx.tokens()))
            .show(|ui| {
                theme::scope(ui, ThemeScope::Popover, |ui| {
                    let mut menu = MoreMenu { ui, popup_id };
                    add_contents(&mut menu);
                })
            })
        {
            response
                .ctx
                .accesskit_node_builder(popup.response.id, |builder| {
                    builder.set_label("Context menu");
                });
        }
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
        let color = self.ui.tokens().text.primary;
        self.menu_button(label.into(), None, color)
    }

    /// Add a standard action with a leading icon and return its response.
    pub fn action_with_icon(&mut self, label: impl Into<String>, icon: Image<'static>) -> Response {
        let color = self.ui.tokens().text.primary;
        self.menu_button(label.into(), Some(icon), color)
    }

    /// Add a nested menu for actions that require a further choice.
    pub fn submenu<R>(
        &mut self,
        label: impl Into<String>,
        add_contents: impl FnOnce(&mut MoreMenu<'_>) -> R,
    ) -> InnerResponse<Option<R>> {
        let popup_id = self.popup_id;
        let color = self.ui.tokens().text.primary;
        self.with_menu_item_style(color, |ui| {
            let label = egui::RichText::new(label.into())
                .font(type_scale::BODY.font_id(ui.ctx()))
                .color(color);
            let button = Button::new(label)
                .right_text(
                    egui::RichText::new(egui::menu::SubMenuButton::RIGHT_ARROW).color(color),
                )
                .min_size(Vec2::new(MENU_WIDTH, MENU_ITEM_HEIGHT));
            let (response, inner) = egui::menu::SubMenuButton::from_button(button).ui(ui, |ui| {
                let mut menu = MoreMenu { ui, popup_id };
                add_contents(&mut menu)
            });
            InnerResponse::new(
                inner.map(|inner| inner.inner),
                response.with_pointing_hand(),
            )
        })
    }

    /// Close the containing menu after a nested action has been selected.
    pub fn close(&self) {
        Popup::close_id(self.ui.ctx(), self.popup_id);
    }

    /// Add a destructive action and return its response.
    pub fn destructive_action(&mut self, label: impl Into<String>) -> Response {
        let color = self.ui.tokens().danger.text;
        self.menu_button(label.into(), None, color)
    }

    /// Add a destructive action with a leading icon and return its response.
    pub fn destructive_action_with_icon(
        &mut self,
        label: impl Into<String>,
        icon: Image<'static>,
    ) -> Response {
        let color = self.ui.tokens().danger.text;
        self.menu_button(label.into(), Some(icon), color)
    }

    /// Add a horizontal rule between related action groups.
    pub fn separator(&mut self) {
        self.ui.add_space(3.0);
        let width = self.ui.available_width();
        let (_, rect) = self.ui.allocate_space(Vec2::new(width, 1.0));
        self.ui.painter().line_segment(
            [rect.left_center(), rect.right_center()],
            stroke::divider(&self.ui.tokens()),
        );
        self.ui.add_space(3.0);
    }

    fn menu_button(
        &mut self,
        label: String,
        icon: Option<Image<'static>>,
        color: Color32,
    ) -> Response {
        let response = self.with_menu_item_style(color, |ui| {
            let text = egui::RichText::new(label)
                .font(type_scale::BODY.font_id(ui.ctx()))
                .color(color);
            let button = match icon {
                Some(icon) => Button::image_and_text(icon, text),
                None => Button::new(text),
            }
            .min_size(Vec2::new(MENU_WIDTH, MENU_ITEM_HEIGHT));
            ui.add(button).with_pointing_hand()
        });

        if response.clicked() {
            Popup::close_id(self.ui.ctx(), self.popup_id);
        }
        response
    }

    fn with_menu_item_style<R>(
        &mut self,
        color: Color32,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> R {
        let saved_widgets = self.ui.visuals().widgets.clone();
        let saved_padding = self.ui.spacing().button_padding;

        let tokens = self.ui.tokens();
        let widgets = &mut self.ui.visuals_mut().widgets;
        for (visuals, fill) in [
            (&mut widgets.inactive, Color32::TRANSPARENT),
            (&mut widgets.hovered, tokens.item.highlight),
            (&mut widgets.active, tokens.control.fill_active),
            (&mut widgets.open, tokens.item.highlight),
        ] {
            visuals.weak_bg_fill = fill;
            visuals.bg_fill = fill;
            visuals.bg_stroke = Stroke::NONE;
            visuals.fg_stroke = Stroke::new(1.0, color);
            visuals.corner_radius = egui::CornerRadius::same(radius::KEY);
        }
        self.ui.spacing_mut().button_padding = MENU_ITEM_PADDING;

        let response = add_contents(self.ui);

        self.ui.visuals_mut().widgets = saved_widgets;
        self.ui.spacing_mut().button_padding = saved_padding;
        response
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
        harness.get_by_label("View logs ⏵").click();
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
