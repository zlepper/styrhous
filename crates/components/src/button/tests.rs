use std::cell::Cell;
use std::rc::Rc;

use super::*;
use crate::test_support::{UiHarnessSnapshot, install_distinct_role_theme, setup_egui};
use egui::epaint::{RectShape, Shape};
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, kittest::NodeT};

fn harness(add: impl FnMut(&mut Ui) + 'static) -> Harness<'static> {
    let mut add = add;
    let mut harness = Harness::builder().build_ui(move |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = size::BUTTON_GAP;
            add(ui);
        });
    });
    setup_egui(&mut harness);
    install_distinct_role_theme(&harness.ctx);
    harness.run();
    harness
}

/// The fills of every rectangle painted in the last frame.
fn rect_fills(harness: &Harness<'_>) -> Vec<Color32> {
    harness
        .output()
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Rect(RectShape { fill, .. }) => Some(*fill),
            _ => None,
        })
        .collect()
}

/// Whether a shape painted in the last frame used `color` as a stroke.
fn strokes_with(harness: &Harness<'_>, color: Color32) -> bool {
    harness
        .output()
        .shapes
        .iter()
        .any(|clipped| match &clipped.shape {
            Shape::Rect(rect) => rect.stroke.color == color,
            _ => false,
        })
}

/// Galley colours painted in the last frame.
fn text_colors(harness: &Harness<'_>) -> Vec<Color32> {
    harness
        .output()
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Text(text) => Some(text.fallback_color),
            _ => None,
        })
        .collect()
}

fn width_of(harness: &Harness<'_>, label: &str) -> f32 {
    harness.get_by_label(label).rect().width()
}

#[test]
fn button_geometry_follows_the_size_ladder() {
    let harness = harness(|ui| {
        for (size, label) in [
            (ButtonSize::Sm, "Small"),
            (ButtonSize::Md, "Medium"),
            (ButtonSize::Lg, "Large"),
        ] {
            Button::secondary(label).size(size).show(ui);
        }
        Button::icon(Icon::Pencil, "Edit").show(ui);
        Button::icon(Icon::Pencil, "Edit small")
            .size(ButtonSize::Sm)
            .show(ui);
        Button::link("Manage license").show(ui);
    });
    let text_width = |role: TextRole, text: &str| {
        role.galley(&harness.ctx, text, Color32::PLACEHOLDER, f32::INFINITY)
            .rect
            .width()
    };
    for (label, size, role, inset) in [
        ("Small", 28.0, type_scale::BUTTON_SM, 12.0),
        ("Medium", 32.0, type_scale::BUTTON, 15.0),
        ("Large", 36.0, type_scale::BUTTON_LG, 17.0),
    ] {
        let rect = harness.get_by_label(label).rect();
        assert_eq!(rect.height(), size, "{label} height");
        assert!(
            (rect.width() - text_width(role, label) - 2.0 * inset).abs() < 0.01,
            "{label} is padded by {inset} points"
        );
    }
    assert_eq!(
        harness.get_by_label("Edit").rect().size(),
        egui::Vec2::splat(32.0)
    );
    assert_eq!(
        harness.get_by_label("Edit small").rect().size(),
        egui::Vec2::splat(28.0)
    );
    let link = harness.get_by_label("Manage license").rect();
    assert_eq!(link.height(), type_scale::BUTTON.line_height());
    assert!((link.width() - text_width(type_scale::BUTTON, "Manage license")).abs() < 0.01);
}

#[test]
fn icons_and_spinners_take_the_leading_slot() {
    let harness = harness(|ui| {
        Button::secondary("Plain").show(ui);
        Button::secondary("Plain")
            .accessibility_label("Plain loading")
            .loading(true)
            .show(ui);
        Button::secondary("Icon")
            .leading_icon(Icon::ArrowPath)
            .show(ui);
        Button::secondary("Icon")
            .leading_icon(Icon::ArrowPath)
            .accessibility_label("Icon loading")
            .loading(true)
            .show(ui);
        Button::secondary("Trailing")
            .trailing_icon(Icon::ArrowTopRightOnSquare)
            .show(ui);
    });
    let plain = width_of(&harness, "Plain");
    assert_eq!(
        width_of(&harness, "Plain loading"),
        plain + size::BUTTON_SPINNER + size::BUTTON_GAP,
        "a spinner leads a label without an icon"
    );
    assert_eq!(
        width_of(&harness, "Icon loading"),
        width_of(&harness, "Icon") - size::ICON + size::BUTTON_SPINNER,
        "a spinner replaces the leading icon"
    );
    assert!(width_of(&harness, "Trailing") > width_of(&harness, "Plain"));
}

#[test]
fn clicks_activate_only_enabled_ready_buttons() {
    let clicks = Rc::new(Cell::new([0_u32; 4]));
    let counter = clicks.clone();
    let mut harness = harness(move |ui| {
        let mut counts = counter.get();
        let buttons = [
            Button::primary("Ready"),
            Button::primary("Disabled").enabled(false),
            Button::primary("Loading").loading(true),
            Button::primary("Unbuilt").not_implemented(),
        ];
        for (count, button) in counts.iter_mut().zip(buttons) {
            if button.show(ui).clicked() {
                *count += 1;
            }
        }
        counter.set(counts);
    });
    for label in ["Ready", "Disabled", "Loading", "Unbuilt"] {
        harness.get_by_label(label).click();
        harness.run();
    }
    assert_eq!(clicks.get(), [1, 0, 0, 0]);
    assert!(
        harness
            .get_by_label("Disabled")
            .accesskit_node()
            .is_disabled()
    );
    assert!(
        harness
            .get_by_label("Unbuilt")
            .accesskit_node()
            .is_disabled(),
        "an unbuilt action is announced as unavailable"
    );
    assert!(!harness.get_by_label("Ready").accesskit_node().is_disabled());
}

#[test]
fn keyboard_focus_activates_with_enter_and_space() {
    let clicks = Rc::new(Cell::new(0));
    let counter = clicks.clone();
    let mut harness = harness(move |ui| {
        if Button::secondary("Refresh").show(ui).clicked() {
            counter.set(counter.get() + 1);
        }
    });
    harness.key_press(egui::Key::Tab);
    harness.run();
    assert!(harness.get_by_label("Refresh").is_focused());
    let theme = harness.ctx.tokens();
    assert!(
        strokes_with(&harness, theme.focus.ring_neutral),
        "keyboard focus shows the neutral focus ring"
    );
    harness.key_press(egui::Key::Enter);
    harness.run();
    harness.key_press(egui::Key::Space);
    harness.run();
    assert_eq!(clicks.get(), 2);
}

#[test]
fn hover_and_press_step_the_fill() {
    let mut harness = harness(|ui| {
        Button::primary("Apply changes").show(ui);
    });
    let theme = harness.ctx.tokens();
    assert!(rect_fills(&harness).contains(&theme.accent.fill));
    harness.get_by_label("Apply changes").hover();
    harness.run();
    assert!(rect_fills(&harness).contains(&theme.accent.fill_hover));
    let center = harness.get_by_label("Apply changes").rect().center();
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: center,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.step();
    assert!(rect_fills(&harness).contains(&theme.accent.fill_active));
}

#[test]
fn variants_paint_their_roles() {
    let theme = crate::test_support::distinct_role_theme();
    let rest = Interaction::default();
    let hovered = Interaction {
        hovered: true,
        ..rest
    };
    let pressed = Interaction {
        pressed: true,
        ..rest
    };
    let colors = |variant: ButtonVariant, state| variant.colors(&theme, state);
    assert_eq!(
        colors(ButtonVariant::Secondary, rest),
        (theme.components.btn_secondary_bg, theme.text.primary)
    );
    assert_eq!(
        colors(ButtonVariant::Soft, pressed),
        (theme.accent.tint_active, theme.accent.text)
    );
    assert_eq!(
        colors(ButtonVariant::Ghost, rest),
        (Color32::TRANSPARENT, theme.text.secondary)
    );
    assert_eq!(
        colors(ButtonVariant::Ghost, hovered),
        (theme.control.fill, theme.text.primary)
    );
    assert_eq!(
        colors(ButtonVariant::Danger, hovered),
        (theme.danger.fill_hover, theme.text.on_fill)
    );
    assert_eq!(
        colors(ButtonVariant::DangerQuiet, hovered),
        (theme.danger.tint, theme.danger.text)
    );
    assert_eq!(ButtonVariant::Primary.focus_ring(&theme), theme.focus.ring);
    assert_eq!(
        ButtonVariant::Ghost.focus_ring(&theme),
        theme.focus.ring_neutral
    );
    assert_eq!(
        ButtonVariant::DangerQuiet.focus_ring(&theme),
        theme.focus.ring_danger
    );
}

#[test]
fn disabled_labels_on_a_fill_fade_as_a_group() {
    let harness = harness(|ui| {
        Button::primary("Apply changes").enabled(false).show(ui);
        Button::ghost("Cancel").enabled(false).show(ui);
    });
    let theme = harness.ctx.tokens();
    let alpha = harness.ctx.global_style().visuals.disabled_alpha();
    let colors = text_colors(&harness);
    assert!(
        colors.contains(
            &theme
                .surface
                .canvas
                .lerp_to_gamma(theme.text.on_fill, alpha)
        ),
        "the label on a fill is painted as the faded group shows it: {colors:?}"
    );
    assert!(
        colors.iter().any(|color| color.a() < u8::MAX),
        "a label without a fill fades with the painter: {colors:?}"
    );
}

#[test]
fn icon_only_and_unbuilt_buttons_explain_themselves() {
    let mut harness = harness(|ui| {
        Button::icon(Icon::Trash, "Delete pod").show(ui);
        Button::secondary("Export").not_implemented().show(ui);
    });
    harness.get_by_label("Delete pod").hover();
    harness.run();
    harness.ui_harness("buttons/icon_tooltip");
    harness.get_by_label("Export").hover();
    harness.run();
    assert!(
        harness.query_by_label(crate::NOT_IMPLEMENTED).is_some(),
        "an unbuilt action says so"
    );
}

#[test]
fn hovered_and_pressed_buttons_by_real_interaction() {
    for variant in [
        ButtonVariant::Primary,
        ButtonVariant::Secondary,
        ButtonVariant::Ghost,
        ButtonVariant::DangerQuiet,
    ] {
        let mut harness = Harness::builder().build_ui(move |ui| {
            ui.horizontal(|ui| {
                Button::new("Rest").variant(variant).show(ui);
                Button::new("Hovered").variant(variant).show(ui);
                Button::new("Pressed").variant(variant).show(ui);
                Button::link("Link").show(ui);
            });
        });
        setup_egui(&mut harness);
        harness.run();
        let pressed = harness.get_by_label("Pressed").rect().center();
        harness.get_by_label("Hovered").hover();
        harness.run();
        harness.ui_harness(format!("buttons/interaction/{variant:?}_hovered").to_lowercase());
        harness
            .input_mut()
            .events
            .push(egui::Event::PointerMoved(pressed));
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos: pressed,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
        harness.step();
        harness.ui_harness(format!("buttons/interaction/{variant:?}_pressed").to_lowercase());
    }
    let mut harness = Harness::builder().build_ui(|ui| {
        Button::link("Manage license").show(ui);
    });
    setup_egui(&mut harness);
    harness.run();
    harness.get_by_label("Manage license").hover();
    harness.run();
    harness.ui_harness("buttons/interaction/link_hovered");
}

#[test]
fn disabled_buttons_explain_why_on_hover() {
    let mut harness = harness(|ui| {
        Button::primary("Apply changes")
            .enabled(false)
            .disabled_reason("No changes to apply")
            .show(ui);
        Button::secondary("Refresh")
            .tooltip("Reload the license")
            .disabled_reason("Only shown while disabled")
            .show(ui);
    });
    harness.get_by_label("Apply changes").hover();
    harness.run();
    harness.ui_harness("buttons/disabled_reason_tooltip");
    assert!(harness.query_by_label("No changes to apply").is_some());
    harness.get_by_label("Refresh").hover();
    harness.run();
    assert!(harness.query_by_label("Reload the license").is_some());
    assert!(
        harness
            .query_by_label("Only shown while disabled")
            .is_none(),
        "an enabled button shows its tooltip, not its disabled reason"
    );
}

#[test]
fn loading_buttons_are_busy_and_show_no_hover() {
    let mut harness = harness(|ui| {
        Button::primary("Saving").loading(true).show(ui);
    });
    assert!(harness.get_by_label("Saving").accesskit_node().is_busy());
    let theme = harness.ctx.tokens();
    harness.get_by_label("Saving").hover();
    harness.run();
    assert!(!rect_fills(&harness).contains(&theme.accent.fill_hover));
}

#[test]
fn buttons_in_a_disabled_ui_do_not_click() {
    let clicks = Rc::new(Cell::new(0));
    let counter = clicks.clone();
    let mut harness = harness(move |ui| {
        ui.add_enabled_ui(false, |ui| {
            if Button::secondary("Cancel").show(ui).clicked() {
                counter.set(counter.get() + 1);
            }
        });
    });
    harness.get_by_label("Cancel").click();
    harness.run();
    assert_eq!(clicks.get(), 0);
    assert!(
        harness
            .get_by_label("Cancel")
            .accesskit_node()
            .is_disabled()
    );
}
