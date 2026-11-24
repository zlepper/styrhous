//! Empty, error and loading states (`.empty`): what is missing and what to
//! do next, centred in the area they replace.

use egui::{Align, Layout, Rect, Response, Sense, Ui, UiBuilder, Vec2, pos2};

use crate::design::theme::{ThemeExt, Tone};
use crate::design::type_scale::{self, TextRole};
use crate::design::{size, space};
use crate::icons::Icon;
use crate::loading::Spinner;
use crate::tile::{Tile, TileSize};
use crate::{Button, ButtonSize};

/// The widest an empty state's text runs before wrapping.
pub const EMPTY_STATE_MAX_WIDTH: f32 = 360.0;
/// Gap between the parts of an empty state (`.empty` gap 4).
const GAP: f32 = space::S4;
/// Extra space above the title and above the action (`margin-top: 8px`).
const SPACING: f32 = space::S8;
/// The title (`.w6` at 13/20).
const TITLE: TextRole = type_scale::UI_STRONG;
/// Extra space under a tile and under a spinner on a code surface
/// (`margin-top` 12 and 10).
const CODE_TILE_SPACING: f32 = space::S12;
const CODE_SPINNER_SPACING: f32 = 10.0;
/// The widest an empty state's text runs on a code surface.
const CODE_MAX_WIDTH: f32 = 440.0;

/// What leads an empty state.
enum Media {
    Tile(Icon, Tone),
    Spinner(Spinner),
    /// A bare muted glyph (the filtered-out state's magnifying glass).
    Glyph(Icon),
}

impl Media {
    fn height(&self, code_surface: bool) -> f32 {
        match self {
            Self::Tile(..) if code_surface => size::TILE_LG,
            Self::Tile(..) => size::TILE,
            Self::Spinner(_) => size::SPINNER_LG,
            Self::Glyph(_) => size::ICON_LG,
        }
    }

    /// Extra space between the media and the title: 8 points under a tile
    /// or spinner, 4 under a bare glyph.
    fn title_spacing(&self, code_surface: bool) -> f32 {
        match self {
            Self::Glyph(_) => GAP,
            Self::Tile(..) if code_surface => CODE_TILE_SPACING,
            Self::Spinner(_) if code_surface => CODE_SPINNER_SPACING,
            Self::Tile(..) | Self::Spinner(_) => SPACING,
        }
    }
}

/// A centred empty, error or loading state: a tile (neutral for nothing
/// selected or no data, danger for errors) or a spinner, a semibold title,
/// a muted description and an optional small action.
///
/// The cases (S12): nothing selected ("Choose a cluster"), no data ("No pods
/// in kube-system"), filtered out (search icon, Clear filter), an error
/// (danger tile, the server message, Retry; keep previous data if any) and
/// loading (a spinner after 300 ms; skeletons for tables).
#[must_use = "call `show` to add the empty state"]
pub struct EmptyState<'a> {
    media: Media,
    title: &'a str,
    description: Option<&'a str>,
    action: Option<(Option<Icon>, &'a str)>,
    code_surface: Option<f32>,
}

impl<'a> EmptyState<'a> {
    /// A state titled `title`, led by a neutral `icon` tile.
    pub fn new(icon: Icon, title: &'a str) -> Self {
        Self::with_media(Media::Tile(icon, Tone::Neutral), title)
    }

    fn with_media(media: Media, title: &'a str) -> Self {
        Self {
            media,
            title,
            description: None,
            action: None,
            code_surface: None,
        }
    }

    /// An error state: a danger tile with the exclamation circle.
    pub fn error(title: &'a str) -> Self {
        Self::with_media(Media::Tile(Icon::ExclamationCircle, Tone::Danger), title)
    }

    /// A loading state led by a large spinner.
    pub fn loading(title: &'a str) -> Self {
        Self::with_media(Media::Spinner(Spinner::new().size(size::SPINNER_LG)), title)
    }

    /// Lead with `spinner` (shown [`size::SPINNER_LG`] wide), for example one
    /// held still for a snapshot.
    pub fn spinner(mut self, spinner: Spinner) -> Self {
        self.media = Media::Spinner(spinner.size(size::SPINNER_LG));
        self
    }

    /// The muted line under the title.
    pub fn description(mut self, description: &'a str) -> Self {
        self.description = Some(description);
        self
    }

    /// A filtered-out state ("No pods match"): a muted magnifying glass;
    /// add Clear filter as its [`Self::action`].
    pub fn filtered(title: &'a str) -> Self {
        Self::with_media(Media::Glyph(Icon::MagnifyingGlass), title)
    }

    /// A small secondary action button ("Clear filter").
    pub fn action(mut self, label: &'a str) -> Self {
        self.action = Some((None, label));
        self
    }

    /// A small secondary action button led by `icon` ("Retry").
    pub fn action_with_icon(mut self, icon: Icon, label: &'a str) -> Self {
        self.action = Some((Some(icon), label));
        self
    }

    /// Set the state for a code surface (the log viewer, L03 and L04): a
    /// large tinted tile or spinner, a Bricolage title, code text colours
    /// and a medium action, starting `top` points below the area's top
    /// instead of being centred in it.
    pub fn on_code_surface(mut self, top: f32) -> Self {
        self.code_surface = Some(top);
        self
    }

    /// Fill the available space and centre the state in it. Returns the
    /// action button's response, if there is one.
    pub fn show(self, ui: &mut Ui) -> Option<Response> {
        let area = ui.available_rect_before_wrap();
        let code = self.code_surface.is_some();
        let width = area.width().min(if code {
            CODE_MAX_WIDTH
        } else {
            EMPTY_STATE_MAX_WIDTH
        });
        let theme = ui.tokens();
        let ctx = ui.ctx().clone();
        // A single line is laid out as is and centred by its width, as CSS
        // centres its line box; only wrapped text centres row by row.
        let centred = |role: TextRole, text: &str, color| {
            let galley = role.galley(&ctx, text, color, width);
            if galley.rows.len() <= 1 {
                return galley;
            }
            let mut job = role.layout_job(&ctx, text, color, width);
            job.halign = Align::Center;
            role.galley_from_job(&ctx, job)
        };
        let (title_role, title_color, description_role, description_color) = if code {
            let description_role = match self.media {
                Media::Tile(..) => type_scale::BODY,
                _ => type_scale::META,
            };
            (
                type_scale::TITLE,
                theme.code.text,
                description_role,
                theme.code.text_muted,
            )
        } else {
            (
                TITLE,
                theme.text.primary,
                type_scale::META,
                theme.text.muted,
            )
        };
        let title = centred(title_role, self.title, title_color);
        let description = self
            .description
            .map(|text| centred(description_role, text, description_color));
        let media_height = self.media.height(code);
        let title_spacing = self.media.title_spacing(code);
        let button_size = if code { ButtonSize::Md } else { ButtonSize::Sm };
        let action_spacing = if code { CODE_TILE_SPACING } else { SPACING };
        let action_height = button_size.height();
        let height = media_height
            + GAP
            + title_spacing
            + title.rect.height()
            + description
                .as_ref()
                .map_or(0.0, |galley| GAP + galley.rect.height())
            + self
                .action
                .map_or(0.0, |_| GAP + action_spacing + action_height);

        // Centred, but never above the area when the text is tall.
        let top = match self.code_surface {
            Some(inset) => area.top() + inset,
            None => (area.center().y - height / 2.0).max(area.top()),
        };
        let centre_x = area.center().x;
        let media_rect = Rect::from_center_size(
            pos2(centre_x, top + media_height / 2.0),
            Vec2::splat(media_height),
        );
        match self.media {
            Media::Tile(icon, tone) if code => Tile::new(icon)
                .tone(tone)
                .size(TileSize::Lg)
                .tinted()
                .paint(ui, media_rect),
            Media::Tile(icon, tone) => Tile::new(icon).tone(tone).paint(ui, media_rect),
            Media::Glyph(icon) => icon.paint(ui, media_rect, theme.text.muted),
            Media::Spinner(spinner) => {
                spinner.paint(ui, media_rect);
                let response =
                    ui.interact(media_rect, ui.id().with("empty-spinner"), Sense::hover());
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::ProgressIndicator, true, "Loading")
                });
            }
        }

        let mut y = media_rect.bottom() + GAP + title_spacing;
        let mut text = |ui: &mut Ui, galley: std::sync::Arc<egui::Galley>, color| {
            // A row-centred galley's rect straddles x = 0.
            let left = if galley.rows.len() <= 1 {
                centre_x - galley.rect.width() / 2.0
            } else {
                centre_x
            };
            let pos = pos2(left, y);
            let rect = galley.rect.translate(pos.to_vec2());
            let response = type_scale::allocate_text_at(ui, rect, galley.text());
            type_scale::paint_text(ui, &response, pos, galley, color);
            y = rect.bottom() + GAP;
        };
        text(ui, title, title_color);
        if let Some(description) = description {
            text(ui, description, description_color);
        }
        let action = self.action.map(|(icon, label)| {
            let mut button = Button::secondary(label).size(button_size);
            if let Some(icon) = icon {
                button = button.leading_icon(icon);
            }
            let mut row = ui.new_child(
                UiBuilder::new()
                    .max_rect(Rect::from_min_max(
                        pos2(area.left(), y + action_spacing),
                        pos2(area.right(), y + action_spacing + action_height),
                    ))
                    .layout(Layout::top_down(Align::Center)),
            );
            button.show(&mut row)
        });
        ui.allocate_rect(area, Sense::hover());
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_states_use_a_danger_tile() {
        let state = EmptyState::error("Couldn’t watch pods");
        assert!(matches!(
            state.media,
            Media::Tile(Icon::ExclamationCircle, Tone::Danger)
        ));
        assert!(matches!(
            EmptyState::loading("Loading").media,
            Media::Spinner(_)
        ));
    }
}
