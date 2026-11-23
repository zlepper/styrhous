//! The painting shared by pill-shaped controls: buttons, toggle buttons and
//! segments (C02, C03).
//!
//! Each of them is a fill (possibly none) with a focus ring outside it and a
//! row of content inside: a leading icon or spinner, a label and a trailing
//! icon, `gap` points apart and vertically centred. Disabled controls are
//! painted at the theme's disabled opacity as one group, the way CSS
//! `opacity` fades an element and its contents together.

use std::sync::Arc;

use egui::{Color32, CornerRadius, Galley, Pos2, Rect, Ui, Vec2, pos2, vec2};

use crate::design::theme::{ThemeExt, Tone};
use crate::design::{radius, size, stroke, type_scale};
use crate::icons::Icon;
use crate::keycap::Keycap;
use crate::loading::Spinner;

/// What leads a control's label.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Leading {
    /// An icon [`size::ICON`] square.
    Icon(Icon),
    /// A [`size::BUTTON_SPINNER`] spinner: on a filled control's colours, or
    /// the label colour on `divider-strong`.
    Spinner { on_fill: bool },
    // C16
    /// An icon [`size::ICON_SM`] square (`.ic.s14`).
    SmallIcon(Icon),
    /// A [`size::STATUS_DOT`] status lamp in its tone (`.dot`), with its glow.
    Dot(Tone),
}

impl Leading {
    fn extent(self) -> f32 {
        match self {
            Self::Icon(_) => size::ICON,
            Self::Spinner { .. } => size::BUTTON_SPINNER,
            Self::SmallIcon(_) => size::ICON_SM,
            Self::Dot(_) => size::STATUS_DOT,
        }
    }

    fn paint(self, ui: &Ui, rect: Rect, color: Color32) {
        match self {
            Self::Icon(icon) | Self::SmallIcon(icon) => icon.paint(ui, rect, color),
            Self::Dot(tone) => {
                crate::status::paint_status_dot(ui.painter(), &ui.tokens(), rect, tone)
            }
            Self::Spinner { on_fill } => {
                let spinner = Spinner::new().size(size::BUTTON_SPINNER);
                let spinner = if on_fill {
                    spinner.on_fill()
                } else {
                    spinner.neutral(color)
                };
                spinner.paint(ui, rect);
            }
        }
    }

    /// Whether the item paints in its own theme colours rather than the
    /// content colour, so a disabled control must fade it with the painter.
    fn has_own_colors(self) -> bool {
        matches!(self, Self::Spinner { on_fill: true } | Self::Dot(_))
    }
}

// C19
/// What ends a control's label.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Trailing {
    /// An icon [`size::ICON_SM`] square (a chevron).
    Icon(Icon),
    /// The keycap of the control's shortcut (`.btn .kbd`); on a solid fill
    /// it sits on `key-on-fill-bg` in the content colour.
    Key {
        key: String,
        on_fill: bool,
        width: f32,
    },
}

impl Trailing {
    /// The keycap of `key`, measured.
    pub fn key(ui: &Ui, key: &str, on_fill: bool) -> Self {
        Self::Key {
            key: key.to_owned(),
            on_fill,
            width: Keycap::new(key).size(ui).x,
        }
    }

    fn extent(&self) -> f32 {
        match self {
            Self::Icon(_) => size::ICON_SM,
            Self::Key { width, .. } => *width,
        }
    }
}

/// The content row of a pill control.
#[derive(Clone, Debug)]
pub(crate) struct Content {
    pub leading: Option<Leading>,
    /// The label, laid out in its role's CSS line box with a placeholder
    /// colour so it can be painted in any state's colour.
    pub text: Option<Arc<Galley>>,
    pub trailing: Option<Trailing>,
    /// Gap between the items.
    pub gap: f32,
}

/// Where [`paint`] put the label: the top-left of its line box and its width.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TextPlacement {
    pub pos: Pos2,
    pub width: f32,
}

/// How a control places its content row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Align {
    /// From the left edge, this many points in.
    Inset(f32),
    /// Centred (icon-only controls).
    Centred,
}

impl Content {
    /// `text` laid out in `role`, ready to paint in any colour.
    pub fn label(ui: &Ui, role: type_scale::TextRole, text: &str) -> Arc<Galley> {
        role.galley(ui.ctx(), text, Color32::PLACEHOLDER, f32::INFINITY)
    }

    /// Whether the content is a lone icon (or spinner).
    pub fn is_icon_only(&self) -> bool {
        self.text.is_none() && self.trailing.is_none()
    }

    /// The width of the items and the gaps between them.
    pub fn width(&self) -> f32 {
        let (sum, count) = [
            self.leading.map(Leading::extent),
            self.text.as_ref().map(|text| text.rect.width()),
            self.trailing.as_ref().map(Trailing::extent),
        ]
        .into_iter()
        .flatten()
        .fold((0.0, 0), |(sum, count), width| (sum + width, count + 1));
        sum + self.gap * (count.max(1) - 1) as f32
    }

    /// The size of a control `height` points tall holding this content
    /// placed by `align`; square for a lone icon.
    pub fn control_size(&self, height: f32, align: Align) -> Vec2 {
        match align {
            Align::Centred => Vec2::splat(height),
            Align::Inset(inset) => vec2(self.width() + 2.0 * inset, height),
        }
    }

    /// Paint the items left to right from `left`, centred on `center_y`.
    /// Items with their own colours are painted `own_color_opacity` times
    /// the current opacity.
    fn paint(
        &self,
        ui: &mut Ui,
        left: f32,
        center_y: f32,
        color: Color32,
        own_color_opacity: f32,
    ) -> Option<TextPlacement> {
        let square = |left: f32, extent: f32| {
            Rect::from_min_size(pos2(left, center_y - extent / 2.0), Vec2::splat(extent))
        };
        let mut x = left;
        if let Some(leading) = self.leading {
            let rect = square(x, leading.extent());
            if leading.has_own_colors() && own_color_opacity < 1.0 {
                with_opacity(ui, own_color_opacity, |ui| leading.paint(ui, rect, color));
            } else {
                leading.paint(ui, rect, color);
            }
            x += leading.extent() + self.gap;
        }
        let mut placement = None;
        if let Some(text) = &self.text {
            let pos = pos2(x, center_y - text.rect.height() / 2.0);
            let width = text.rect.width();
            type_scale::paint_galley(ui.painter(), pos, text.clone(), color);
            placement = Some(TextPlacement { pos, width });
            x += width + self.gap;
        }
        match &self.trailing {
            Some(Trailing::Icon(icon)) => icon.paint(ui, square(x, size::ICON_SM), color),
            Some(Trailing::Key { key, on_fill, .. }) => {
                let keycap = Keycap::new(key);
                let key_size = keycap.size(ui);
                let rect = Rect::from_min_size(pos2(x, center_y - key_size.y / 2.0), key_size);
                if *on_fill {
                    keycap.paint_in(ui, rect, ui.tokens().components.key_on_fill_bg, color);
                } else {
                    keycap.paint(ui, rect);
                }
            }
            None => {}
        }
        placement
    }
}

/// Run `paint` with `ui`'s opacity multiplied by `factor`, then restore it.
pub(crate) fn with_opacity<R>(ui: &mut Ui, factor: f32, paint: impl FnOnce(&mut Ui) -> R) -> R {
    let opacity = ui.opacity();
    // `set_opacity`, not `multiply_opacity`, which clamps factors above 1.
    ui.set_opacity(opacity * factor);
    let result = paint(ui);
    ui.set_opacity(opacity);
    result
}

/// The colours and shape of one frame of a pill control.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Look {
    /// The fill; transparent for none.
    pub fill: Color32,
    /// The colour of the label and icons.
    pub content: Color32,
    pub corner_radius: CornerRadius,
    /// The focus ring colour, when the control shows keyboard focus.
    pub focus_ring: Option<Color32>,
}

impl Look {
    /// A fully rounded pill.
    pub fn pill(fill: Color32, content: Color32, focus_ring: Option<Color32>) -> Self {
        Self {
            fill,
            content,
            corner_radius: radius::FULL.into(),
            focus_ring,
        }
    }
}

/// Paint a pill control into `rect`: its focus ring, fill and `content`,
/// placed by `align`.
///
/// In a disabled `ui` the whole control fades to the theme's disabled
/// opacity as a group: the fill is painted translucent, and content on a
/// fill is painted opaque in the colour the faded group would show over the
/// canvas, so the fill never shows through the label. (On another surface
/// the label is off by the difference between that surface and the canvas
/// times the transparency, a few levels at most in Harbour.)
pub(crate) fn paint(
    ui: &mut Ui,
    rect: Rect,
    look: Look,
    content: &Content,
    align: Align,
) -> Option<TextPlacement> {
    let ring_extent = size::FOCUS_RING_EXTENT;
    if !ui.is_rect_visible(rect.expand(ring_extent)) {
        return None;
    }
    let theme = ui.tokens();
    if let Some(ring) = look.focus_ring {
        stroke::paint_focus_ring(
            ui.painter(),
            rect,
            look.corner_radius,
            theme.surface.canvas,
            ring,
        );
    }
    let has_fill = look.fill.a() > 0;
    if has_fill {
        ui.painter()
            .rect_filled(rect, look.corner_radius, look.fill);
    }

    let left = match align {
        Align::Centred => rect.center().x - content.width() / 2.0,
        Align::Inset(inset) => rect.left() + inset,
    };
    if has_fill {
        paint_on_fill(ui, look.content, |ui, color, own_color_opacity| {
            content.paint(ui, left, rect.center().y, color, own_color_opacity)
        })
    } else {
        content.paint(ui, left, rect.center().y, look.content, 1.0)
    }
}

/// Paint content in `color` that sits on an opaque fill of a control.
///
/// In a disabled `ui` the control fades to the theme's disabled opacity as
/// one group (CSS `opacity`), so content on its fill is painted opaque in
/// the colour the faded group shows over the canvas, instead of fading on
/// top of the faded fill. `paint` gets that colour and the opacity factor
/// to apply to anything painted in its own colours.
/// The factor that undoes the fade a disabled `ui` applies to its painter
/// (1 in an enabled `ui`), for parts of a control the design keeps at full
/// opacity while disabled.
pub(crate) fn disabled_fade_compensation(ui: &Ui) -> f32 {
    let disabled_alpha = ui.visuals().disabled_alpha();
    if ui.is_enabled() || disabled_alpha <= 0.0 {
        1.0
    } else {
        1.0 / disabled_alpha
    }
}

pub(crate) fn paint_on_fill<R>(
    ui: &mut Ui,
    color: Color32,
    paint: impl FnOnce(&mut Ui, Color32, f32) -> R,
) -> R {
    let disabled_alpha = ui.visuals().disabled_alpha();
    if ui.is_enabled() || disabled_alpha <= 0.0 {
        return paint(ui, color, 1.0);
    }
    let color = ui
        .tokens()
        .surface
        .canvas
        .lerp_to_gamma(color, disabled_alpha);
    with_opacity(ui, 1.0 / disabled_alpha, |ui| {
        paint(ui, color, disabled_alpha)
    })
}
