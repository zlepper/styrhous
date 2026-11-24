//! Buttons (C02): one pill-shaped button in six variants plus links.
//!
//! ```ignore
//! if Button::primary("Apply changes").show(ui).clicked() { … }
//! Button::secondary("Refresh").leading_icon(Icon::ArrowPath).show(ui);
//! Button::icon(Icon::Trash, "Delete").variant(ButtonVariant::DangerQuiet).show(ui);
//! Button::secondary("Export").not_implemented().show(ui);
//! ```
//!
//! Primary appears at most once per surface (blade, dialog, window header);
//! solid [`ButtonVariant::Danger`] only confirms a destructive dialog, and
//! every other destructive action is [`ButtonVariant::DangerQuiet`]. In
//! footers buttons are right-aligned 8 points apart, secondary before
//! primary.

use egui::{
    Color32, CornerRadius, Response, Sense, Ui, Widget, WidgetInfo, WidgetType, pos2, vec2,
};

use crate::control::{self, Content, Leading, Look, Trailing};
use crate::design::theme::{Theme, ThemeExt};
use crate::design::type_scale::{self, TextRole};
use crate::design::{radius, size};
use crate::icons::Icon;
use crate::interaction::{ControlState, Feedback, Interaction};

/// The colour scheme of a [`Button`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    /// The one action a surface exists for: accent fill.
    Primary,
    /// Every other button: the strong neutral fill.
    #[default]
    Secondary,
    /// A low-emphasis positive action inside content: accent tint.
    Soft,
    /// Toolbar and header buttons, Cancel in dense rows: no fill at rest.
    Ghost,
    /// Confirms a destructive dialog, and nothing else: danger fill.
    Danger,
    /// A destructive action that opens a confirmation: danger text on the
    /// neutral fill.
    DangerQuiet,
    /// Inline navigation: accent text without a fill, underlined on hover.
    Link,
}

impl ButtonVariant {
    /// The fill (transparent for none) and content colour in `state`.
    fn colors(self, theme: &Theme, state: Interaction) -> (Color32, Color32) {
        let accent = &theme.accent;
        let danger = &theme.danger;
        let components = &theme.components;
        match self {
            Self::Primary => (
                state.pick(accent.fill, accent.fill_hover, accent.fill_active),
                theme.text.on_fill,
            ),
            Self::Secondary => (
                state.pick(
                    components.btn_secondary_bg,
                    components.btn_secondary_hover,
                    components.btn_secondary_press,
                ),
                theme.text.primary,
            ),
            Self::Soft => (
                state.pick(accent.tint, accent.tint_hover, accent.tint_active),
                accent.text,
            ),
            Self::Ghost => (
                state.pick(
                    Color32::TRANSPARENT,
                    theme.control.fill,
                    theme.control.fill_hover,
                ),
                state.pick(theme.text.secondary, theme.text.primary, theme.text.primary),
            ),
            Self::Danger => (
                state.pick(danger.fill, danger.fill_hover, danger.fill_active),
                theme.text.on_fill,
            ),
            Self::DangerQuiet => (
                state.pick(components.btn_secondary_bg, danger.tint, danger.tint_hover),
                danger.text,
            ),
            Self::Link => (Color32::TRANSPARENT, accent.text),
        }
    }

    /// The focus ring, in the button's own colour family.
    fn focus_ring(self, theme: &Theme) -> Color32 {
        match self {
            Self::Primary | Self::Soft => theme.focus.ring,
            Self::Secondary | Self::Ghost | Self::Link => theme.focus.ring_neutral,
            Self::Danger | Self::DangerQuiet => theme.focus.ring_danger,
        }
    }

    /// Whether the variant has a solid fill that its spinner sits on.
    fn is_solid(self) -> bool {
        matches!(self, Self::Primary | Self::Danger)
    }

    fn corner_radius(self) -> CornerRadius {
        match self {
            Self::Link => CornerRadius::same(radius::LINK),
            _ => radius::FULL.into(),
        }
    }
}

/// The size of a [`Button`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonSize {
    /// 28 points, 12/16 labels: table rows, card headers, dense toolbars.
    Sm,
    /// 32 points, 13/20 labels: the default everywhere.
    #[default]
    Md,
    /// 36 points, 14/20 labels: empty-state calls to action only.
    Lg,
}

impl ButtonSize {
    /// The height of a button of this size, for rows that must line up with one.
    pub const fn height(self) -> f32 {
        match self {
            Self::Sm => size::CONTROL_SM,
            Self::Md => size::CONTROL_MD,
            Self::Lg => size::CONTROL_LG,
        }
    }

    const fn inset(self) -> f32 {
        match self {
            Self::Sm => size::BUTTON_INSET_SM,
            Self::Md => size::BUTTON_INSET_MD,
            Self::Lg => size::BUTTON_INSET_LG,
        }
    }

    const fn gap(self) -> f32 {
        match self {
            Self::Sm => size::BUTTON_GAP_SM,
            Self::Md | Self::Lg => size::BUTTON_GAP,
        }
    }

    const fn role(self) -> TextRole {
        match self {
            Self::Sm => type_scale::BUTTON_SM,
            Self::Md => type_scale::BUTTON,
            Self::Lg => type_scale::BUTTON_LG,
        }
    }
}

/// A button (`.btn`): a pill with a label, an optional leading and trailing
/// icon, or just an icon.
///
/// Enter and Space activate a focused button. A disabled button fades to
/// the theme's disabled opacity; say why with [`Self::disabled_reason`] when
/// it is not obvious. A loading button shows a spinner in place of its
/// leading icon and ignores clicks.
#[must_use = "call `show` to add the button"]
pub struct Button {
    label: Option<String>,
    accessibility_label: Option<String>,
    icon: Option<Icon>,
    small_icon: bool,
    trailing_icon: Option<Icon>,
    shortcut: Option<String>,
    variant: ButtonVariant,
    size: ButtonSize,
    loading: bool,
    enabled: bool,
    disabled_reason: Option<String>,
    tooltip: Option<String>,
    forced_state: Option<ControlState>,
    not_implemented: bool,
    corner_radius: Option<CornerRadius>,
}

impl Button {
    /// A [`ButtonVariant::Secondary`] button labelled `label`.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: Some(label.into()),
            accessibility_label: None,
            icon: None,
            small_icon: false,
            trailing_icon: None,
            shortcut: None,
            variant: ButtonVariant::default(),
            size: ButtonSize::default(),
            loading: false,
            enabled: true,
            disabled_reason: None,
            tooltip: None,
            forced_state: None,
            not_implemented: false,
            corner_radius: None,
        }
    }

    /// A square, icon-only [`ButtonVariant::Secondary`] button. `label` is
    /// its accessible name and its tooltip.
    pub fn icon(icon: Icon, label: impl Into<String>) -> Self {
        let label = label.into();
        Self {
            label: None,
            icon: Some(icon),
            tooltip: Some(label.clone()),
            accessibility_label: Some(label),
            ..Self::new("")
        }
    }

    // C18
    /// The ghost small × that closes a header, palette or panel, named
    /// `label` ("Close blade").
    pub fn close(label: impl Into<String>) -> Self {
        Self::icon(Icon::XMark, label)
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Sm)
    }

    /// A [`ButtonVariant::Primary`] button.
    pub fn primary(label: impl Into<String>) -> Self {
        Self::new(label).variant(ButtonVariant::Primary)
    }

    /// A [`ButtonVariant::Secondary`] button.
    pub fn secondary(label: impl Into<String>) -> Self {
        Self::new(label)
    }

    /// A [`ButtonVariant::Soft`] button.
    pub fn soft(label: impl Into<String>) -> Self {
        Self::new(label).variant(ButtonVariant::Soft)
    }

    /// A [`ButtonVariant::Ghost`] button.
    pub fn ghost(label: impl Into<String>) -> Self {
        Self::new(label).variant(ButtonVariant::Ghost)
    }

    /// A [`ButtonVariant::Danger`] button.
    pub fn danger(label: impl Into<String>) -> Self {
        Self::new(label).variant(ButtonVariant::Danger)
    }

    /// A [`ButtonVariant::DangerQuiet`] button.
    pub fn danger_quiet(label: impl Into<String>) -> Self {
        Self::new(label).variant(ButtonVariant::DangerQuiet)
    }

    /// A [`ButtonVariant::Link`] button.
    pub fn link(label: impl Into<String>) -> Self {
        Self::new(label).variant(ButtonVariant::Link)
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// Lead the label with `icon`, when it adds recognition (Refresh, Edit).
    pub fn leading_icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// [`Self::leading_icon`] at [`size::ICON_SM`] (`.ic.s14`), as the inspector's primary
    /// actions have it.
    pub fn leading_small_icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self.small_icon = true;
        self
    }

    /// Follow the label with a small `icon`, for "opens elsewhere" or a
    /// dropdown.
    pub fn trailing_icon(mut self, icon: Icon) -> Self {
        self.trailing_icon = Some(icon);
        self
    }

    // C19
    /// Follow the label with the keycap of the button's shortcut (`L` for
    /// Logs): the primary header action shows its key. The caller handles the
    /// key itself.
    pub fn shortcut(mut self, key: impl Into<String>) -> Self {
        self.shortcut = Some(key.into());
        self
    }

    /// Show a spinner in place of the leading icon (before the label when
    /// there is none) and ignore clicks.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// Enable or disable the button. A button in a disabled [`Ui`] is
    /// disabled too.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The tooltip of the button while it is disabled, saying why
    /// ("No changes to apply").
    pub fn disabled_reason(mut self, reason: impl Into<String>) -> Self {
        self.disabled_reason = Some(reason.into());
        self
    }

    /// The tooltip of the enabled button. Icon-only buttons show their
    /// label by default.
    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// Name the button for assistive technologies and UI tests differently
    /// from its label ("Remove CPU column" for a "Remove" button).
    pub fn accessibility_label(mut self, label: impl Into<String>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }

    /// Paint `state` whatever the pointer and keyboard do (design boards and
    /// snapshots). The button still responds to real input.
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    /// Square off corners where the button joins other controls (the
    /// steppers of a [`crate::NumberField`]); the focus ring follows them.
    pub fn corner_radius(mut self, corner_radius: impl Into<CornerRadius>) -> Self {
        self.corner_radius = Some(corner_radius.into());
        self
    }

    /// Show the button as designed for an action that is not built yet:
    /// it never acts and its tooltip says "Not yet implemented"
    /// ([`crate::interaction::not_implemented`]).
    pub fn not_implemented(mut self) -> Self {
        self.not_implemented = true;
        self
    }

    /// Add the button. Check [`Response::clicked`] for activation.
    pub fn show(self, ui: &mut Ui) -> Response {
        let enabled = self.enabled;
        ui.add_enabled(enabled, Placed(self))
    }

    fn content(&self, ui: &Ui) -> Content {
        let leading = if self.loading {
            Some(Leading::Spinner {
                on_fill: self.variant.is_solid(),
            })
        } else {
            self.icon.map(|icon| {
                if self.small_icon {
                    Leading::SmallIcon(icon)
                } else {
                    Leading::Icon(icon)
                }
            })
        };
        Content {
            leading,
            text: self
                .label
                .as_deref()
                .map(|label| Content::label(ui, self.size.role(), label)),
            trailing: match &self.shortcut {
                Some(key) => Some(Trailing::key(ui, key, self.variant.is_solid())),
                None => self.trailing_icon.map(Trailing::Icon),
            },
            gap: self.size.gap(),
        }
    }

    fn name(&self) -> &str {
        self.accessibility_label
            .as_deref()
            .or(self.label.as_deref())
            .unwrap_or_default()
    }
}

/// A [`Button`] as an egui widget, so [`Button::show`] can disable it with
/// [`Ui::add_enabled`] (which never fades a disabled `Ui` twice).
struct Placed(Button);

impl Widget for Placed {
    fn ui(self, ui: &mut Ui) -> Response {
        self.0.paint(ui)
    }
}

impl Button {
    fn paint(self, ui: &mut Ui) -> Response {
        let content = self.content(ui);
        let is_link = self.variant == ButtonVariant::Link;
        let icon_only = self.label.is_none();
        let (height, align) = match (is_link, icon_only) {
            (true, true) => (size::ICON, control::Align::Centred),
            (true, false) => (self.size.role().line_height(), control::Align::Inset(0.0)),
            (false, true) => (self.size.height(), control::Align::Centred),
            (false, false) => (self.size.height(), control::Align::Inset(self.size.inset())),
        };
        // A loading button ignores clicks; an unbuilt one stays reachable
        // from the keyboard so its tooltip can be read.
        let sense = if self.loading {
            Sense::hover()
        } else if self.not_implemented {
            Sense::focusable_noninteractive()
        } else {
            Sense::click()
        };
        let (rect, response) = ui.allocate_exact_size(content.control_size(height, align), sense);
        let enabled = ui.is_enabled();
        response.widget_info(|| {
            WidgetInfo::labeled(
                WidgetType::Button,
                enabled && !self.not_implemented,
                self.name(),
            )
        });
        if self.loading {
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_busy());
        }

        let theme = ui.tokens();
        let passive = self.loading || self.not_implemented;
        let state = Interaction::of(&response, self.forced_state, passive);
        let (fill, content_color) = self.variant.colors(&theme, state);
        let look = Look {
            fill,
            content: content_color,
            corner_radius: self
                .corner_radius
                .unwrap_or_else(|| self.variant.corner_radius()),
            focus_ring: state.focused.then(|| self.variant.focus_ring(&theme)),
        };
        let text = control::paint(ui, rect, look, &content, align);
        if is_link
            && state.hovered
            && let Some(text) = text
        {
            let baseline = text.pos.y + self.size.role().css_baseline(ui.pixels_per_point());
            let underline = egui::Rect::from_min_size(
                pos2(text.pos.x, baseline + size::LINK_UNDERLINE_OFFSET),
                vec2(text.width, size::LINK_UNDERLINE),
            );
            ui.painter().rect_filled(underline, 0, look.content);
        }

        Feedback {
            tooltip: self.tooltip.as_deref(),
            disabled_reason: self.disabled_reason.as_deref(),
            clickable: !passive,
            not_implemented: self.not_implemented,
        }
        .apply(response)
    }
}

#[cfg(test)]
mod tests;
