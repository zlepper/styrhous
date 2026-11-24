//! A title over a one-line meta text, each elided to the room there is.

use egui::{Id, Rect, Sense, Ui, pos2, vec2};

use crate::design::theme::ThemeExt;
use crate::design::type_scale::{self, TextRole};
use crate::tooltip::TooltipExt;

/// The title (`primary` text) with an optional meta line under it (`muted`
/// text): a column as wide as its longer line and as tall as its lines. An
/// elided line carries a tooltip with its full text.
pub struct TitleStack<'a> {
    id: Id,
    title: &'a str,
    title_role: TextRole,
    meta: Option<(&'a str, TextRole)>,
}

impl<'a> TitleStack<'a> {
    /// A stack whose lines are identified by `id` (`id.with("title")`, `id.with("meta")`).
    pub fn new(id: Id, title: &'a str, title_role: TextRole) -> Self {
        Self {
            id,
            title,
            title_role,
            meta: None,
        }
    }

    /// The line under the title.
    pub fn meta(mut self, meta: &'a str, role: TextRole) -> Self {
        self.meta = Some((meta, role));
        self
    }

    /// Add the stack at the cursor, each line elided to `max_width`. Returns
    /// the column's rectangle.
    pub fn show(self, ui: &mut Ui, max_width: f32) -> Rect {
        let theme = ui.tokens();
        let title =
            self.title_role
                .galley_elided(ui.ctx(), self.title, theme.text.primary, max_width, 1);
        let meta = self.meta.map(|(text, role)| {
            (
                text,
                role,
                role.galley_elided(ui.ctx(), text, theme.text.muted, max_width, 1),
            )
        });
        let width = title.rect.width().max(
            meta.as_ref()
                .map_or(0.0, |(_, _, galley)| galley.rect.width()),
        );
        let height = self.title_role.line_height()
            + meta.as_ref().map_or(0.0, |(_, role, _)| role.line_height());
        let (column, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        let title_elided = title.elided;
        let title_label = type_scale::text_at(
            ui,
            self.id.with("title"),
            column.min,
            title,
            self.title,
            theme.text.primary,
        );
        if title_elided {
            title_label.tooltip(self.title);
        }
        if let Some((text, _, galley)) = meta {
            let elided = galley.elided;
            let label = type_scale::text_at(
                ui,
                self.id.with("meta"),
                pos2(column.left(), column.top() + self.title_role.line_height()),
                galley,
                text,
                theme.text.muted,
            );
            if elided {
                label.tooltip(text);
            }
        }
        column
    }
}
