//! The key-hint footer of a popover (`.pop-foot`): a select's "↑ ↓ move ·
//! Space toggle · Enter done", the completion popover's "↑ ↓ move · Enter
//! apply · Esc dismiss".

use egui::{CornerRadius, Rect, Sense, Ui, Vec2, pos2, vec2};

use crate::design::theme::ThemeExt;
use crate::design::type_scale::{self, TextRole};
use crate::design::{radius, size, space, stroke};
use crate::keycap::Keycap;

/// The hints' words (12 px in 20 point line boxes).
const FOOTER_ROLE: TextRole = type_scale::FIELD_SM;

/// One hint: the keys, then what they do.
pub(crate) type KeyHint<'a> = (&'a [&'a str], &'a str);

/// Add the footer across the whole popover, its padding included, on
/// `surface-sunken` below a divider, [`crate::frames::POPOVER_PADDING`]
/// below the content above it.
pub(crate) fn popover_footer(ui: &mut Ui, hints: &[KeyHint<'_>]) {
    let padding = crate::frames::POPOVER_PADDING;
    ui.add_space(padding);
    let (slot, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), size::POPOVER_FOOTER - padding),
        Sense::hover(),
    );
    // The hints are painted; read them as one line ("↑ ↓ move · Enter apply").
    let words = hints
        .iter()
        .map(|(keys, action)| format!("{} {action}", keys.join(" ")))
        .collect::<Vec<_>>()
        .join(" · ");
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &words));
    let rect = Rect::from_min_max(
        slot.min - vec2(padding, 0.0),
        slot.max + Vec2::splat(padding),
    );
    let theme = ui.tokens();
    let corner = radius::POPOVER;
    ui.painter().rect_filled(
        rect,
        CornerRadius {
            nw: 0,
            ne: 0,
            ..CornerRadius::same(corner)
        },
        theme.surface.sunken,
    );
    ui.painter().hline(
        rect.x_range(),
        rect.top() + size::HAIRLINE / 2.0,
        stroke::divider(&theme),
    );
    // Centred in the box below the divider (`border-top: 1px`).
    let center_y = (rect.top() + size::HAIRLINE + rect.bottom()) / 2.0;
    let mut x = rect.left() + size::OPTION_PADDING_X;
    for (keys, action) in hints {
        for key in *keys {
            let key = Keycap::new(key);
            let key_size = key.size(ui);
            key.paint(
                ui,
                Rect::from_min_size(pos2(x, center_y - key_size.y / 2.0), key_size),
            );
            x += key_size.x + space::S4;
        }
        let galley = FOOTER_ROLE.galley(ui.ctx(), action, theme.text.muted, f32::INFINITY);
        let width = galley.rect.width();
        let pos = pos2(x, center_y - galley.rect.height() / 2.0);
        type_scale::paint_galley(ui.painter(), pos, galley, theme.text.muted);
        x += width + space::S8;
    }
}
