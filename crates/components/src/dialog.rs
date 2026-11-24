//! The modal dialog (C11 `.dlg`): the one frame every confirmation, typed
//! confirmation and error dialog shares.
//!
//! A dialog is [`size::DIALOG_WIDTH`] wide ([`size::DIALOG_WIDTH_WIDE`] when
//! it holds logs or a table), on `surface-panel` with the overlay elevation
//! and radius, centred over the scrim. Its body opens with a header row: a
//! large icon tile carrying the tone, then the title (a question) with its
//! description under it. Everything else (callouts, code blocks, fields)
//! runs the full content width, 16 points below the header; the body sets the
//! gap between its own blocks (12 in a typed confirmation). The footer
//! sits on `surface-sunken` under a divider, its buttons on the right.

use egui::{
    Align, Context, Frame, Id, Key, Layout, Modal, Modifiers, Rect, Sense, Ui, UiBuilder, Vec2,
    pos2, vec2,
};

use crate::a11y::{Name, describe_container};
use crate::design::theme::{self, ThemeExt, ThemeScope, Tone};
use crate::design::{radius, size, space, type_scale};
use crate::icons::Icon;
use crate::prose::Prose;
use crate::tile::{Tile, TileSize};

/// Padding of the dialog body (`.dlg-b`).
const BODY_PADDING: f32 = space::S24;
/// Gap between the blocks of the body, and between the tile and the title.
const BODY_GAP: f32 = space::S16;
/// Gap between the title and its description.
const TITLE_TO_DESCRIPTION: f32 = space::S4;
/// Vertical padding of the footer (`.dlg-f`).
const FOOTER_PADDING_Y: f32 = space::S16;
/// Horizontal padding of the footer.
const FOOTER_PADDING_X: f32 = space::S24;
/// Gap between footer buttons.
const FOOTER_GAP: f32 = space::S8;

/// The frame and header of a modal dialog. Show it in place with
/// [`Self::show_inline`] (design boards) or as a modal with [`Self::show`].
#[must_use = "call `show` to add the dialog"]
pub struct Dialog<'a> {
    id: Id,
    icon: Icon,
    tone: Tone,
    title: &'a str,
    description: Prose<'a>,
    width: f32,
}

/// The result of showing a modal [`Dialog`].
pub struct DialogResponse<R> {
    /// What the footer returned.
    pub inner: R,
    /// Escape was pressed or the scrim clicked while the dialog was on top:
    /// the user asked to cancel.
    pub dismissed: bool,
}

impl<'a> Dialog<'a> {
    /// A dialog titled `title` whose header tile shows `icon` in `tone`
    /// (accent for neutral actions, danger for destructive ones and errors).
    pub fn new(id: Id, icon: Icon, tone: Tone, title: &'a str) -> Self {
        Self {
            id,
            icon,
            tone,
            title,
            description: Prose::new(),
            width: size::DIALOG_WIDTH,
        }
    }

    /// The sentence under the title; name the resource in bold.
    pub fn description(mut self, description: impl Into<Prose<'a>>) -> Self {
        self.description = description.into();
        self
    }

    /// The dialog's width; [`size::DIALOG_WIDTH`] by default.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Show the dialog as a modal over the scrim. `body` adds the blocks
    /// below the header (nothing for a plain confirmation); `footer` adds the
    /// buttons from the right, so add the confirming action first.
    pub fn show<R>(
        self,
        ctx: &Context,
        body: impl FnOnce(&mut Ui),
        footer: impl FnOnce(&mut Ui) -> R,
    ) -> DialogResponse<R> {
        let tokens = ctx.tokens();
        let id = self.id;
        let response = Modal::new(id)
            .area(
                Modal::default_area(id)
                    .default_width(self.width)
                    .fade_in(false),
            )
            .backdrop_color(tokens.shade.scrim)
            .frame(Frame::NONE)
            .show(ctx, |ui| self.show_inline(ui, body, footer));
        let on_top = response.is_top_modal && !response.any_popup_open;
        // Esc in a text field of the dialog only leaves the field.
        let dismissed = on_top
            && (response.backdrop_response.clicked() || {
                let left_field = crate::interaction::escape_left_text_field(ctx);
                ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
                    && !left_field
            });
        DialogResponse {
            inner: response.inner,
            dismissed,
        }
    }

    /// Paint the dialog at the cursor, as its modal shows it.
    pub fn show_inline<R>(
        self,
        ui: &mut Ui,
        body: impl FnOnce(&mut Ui),
        footer: impl FnOnce(&mut Ui) -> R,
    ) -> R {
        let outer = Rect::from_min_size(ui.cursor().min, vec2(self.width, 0.0));
        let mut dialog_ui = ui.new_child(
            UiBuilder::new()
                .id_salt(self.id)
                .max_rect(outer)
                .layout(Layout::top_down(Align::Min)),
        );
        let (rect, inner, (title, description)) =
            theme::scope(&mut dialog_ui, ThemeScope::Dialog, |ui| {
                self.paint(ui, outer, body, footer)
            });
        // The title names the dialog (by reference, so it is not read twice).
        describe_container(
            &dialog_ui,
            egui::accesskit::Role::AlertDialog,
            Name::LabelledBy(title),
            Some(rect),
        );
        // The description paragraph describes it, again by reference.
        if let Some(description) = description {
            dialog_ui
                .ctx()
                .accesskit_node_builder(dialog_ui.unique_id(), |node| {
                    node.set_described_by(vec![description.accesskit_id()]);
                });
        }
        ui.advance_cursor_after_rect(rect);
        inner
    }

    fn paint<R>(
        &self,
        ui: &mut Ui,
        outer: Rect,
        body: impl FnOnce(&mut Ui),
        footer: impl FnOnce(&mut Ui) -> R,
    ) -> (Rect, R, (egui::Id, Option<egui::Id>)) {
        let theme = ui.tokens();
        let painter = ui.painter().clone();
        let shadow = painter.add(egui::Shape::Noop);
        let panel = painter.add(egui::Shape::Noop);
        let sunken = painter.add(egui::Shape::Noop);

        let content = Rect::from_min_max(
            outer.min + Vec2::splat(BODY_PADDING),
            pos2(outer.right() - BODY_PADDING, f32::INFINITY),
        );
        let mut body_ui = child(ui, content, Layout::top_down(Align::Min));
        let title = self.header(&mut body_ui);
        let blocks_top = body_ui.min_rect().bottom() + BODY_GAP;
        let mut blocks_ui = child(
            &mut body_ui,
            Rect::from_min_max(pos2(content.left(), blocks_top), content.max),
            Layout::top_down(Align::Min),
        );
        body(&mut blocks_ui);
        let mut body_bottom = body_ui.min_rect().bottom();
        if blocks_ui.min_rect().height() > 0.0 {
            body_bottom = blocks_ui.min_rect().bottom();
        }

        let footer_top = body_bottom + BODY_PADDING;
        let footer_content = Rect::from_min_max(
            pos2(
                outer.left() + FOOTER_PADDING_X,
                footer_top + size::HAIRLINE + FOOTER_PADDING_Y,
            ),
            pos2(outer.right() - FOOTER_PADDING_X, f32::INFINITY),
        );
        let mut footer_ui = child(ui, footer_content, Layout::right_to_left(Align::Min));
        footer_ui.spacing_mut().item_spacing.x = FOOTER_GAP;
        let inner = footer(&mut footer_ui);
        let bottom = footer_ui.min_rect().bottom() + FOOTER_PADDING_Y;

        let rect = Rect::from_min_max(outer.min, pos2(outer.right(), bottom));
        let footer_rect = Rect::from_min_max(pos2(rect.left(), footer_top), rect.max);
        let corner = radius::OVERLAY;
        painter.set(
            shadow,
            theme
                .shadow
                .overlay
                .shape(ui.ctx(), ui.pixels_per_point(), rect, corner),
        );
        painter.set(
            panel,
            egui::epaint::RectShape::filled(rect, corner, theme.surface.panel),
        );
        painter.set(sunken, crate::frames::sunken_footer(&theme, footer_rect));
        ui.allocate_rect(rect, Sense::hover());
        (rect, inner, title)
    }

    /// The tile, title and description row (`.dlg-hd`). Returns the ids of
    /// the title and the description.
    fn header(&self, ui: &mut Ui) -> (egui::Id, Option<egui::Id>) {
        let theme = ui.tokens();
        let row = ui.cursor().min;
        let tile = TileSize::Lg;
        Tile::new(self.icon)
            .tone(self.tone)
            .size(tile)
            .paint(ui, Rect::from_min_size(row, Vec2::splat(tile.side())));
        let text_left = row.x + tile.side() + BODY_GAP;
        let mut text_ui = child(
            ui,
            Rect::from_min_max(
                pos2(text_left, row.y),
                pos2(ui.max_rect().right(), f32::INFINITY),
            ),
            Layout::top_down(Align::Min),
        );
        let title = type_scale::TITLE.label(&mut text_ui, self.title, theme.text.primary);
        let description = (!self.description.is_empty()).then(|| {
            text_ui.add_space(TITLE_TO_DESCRIPTION);
            self.description
                .label(&mut text_ui, type_scale::BODY, theme.text.secondary)
                .id
        });
        let bottom = text_ui.min_rect().bottom().max(row.y + tile.side());
        ui.allocate_rect(
            Rect::from_min_max(row, pos2(ui.max_rect().right(), bottom)),
            Sense::hover(),
        );
        (title.id, description)
    }
}

/// A child of `ui` filling `rect` with `layout` and zero item spacing.
fn child(ui: &mut Ui, rect: Rect, layout: Layout) -> Ui {
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect).layout(layout));
    child.spacing_mut().item_spacing = Vec2::ZERO;
    child
}

/// Moves keyboard focus into a modal dialog once after it opens.
///
/// A dialog is "opened" when it is shown in a frame after one it was not
/// shown in. Focus moves on the first pass that can take it: a modal's first
/// frame is an invisible sizing pass whose widgets cannot be focused.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FocusOnOpen {
    key: Option<Id>,
}

impl FocusOnOpen {
    /// Never moves focus (dialogs painted in place).
    pub(crate) const NEVER: Self = Self { key: None };

    /// Track the dialog `id`, shown this frame.
    pub(crate) fn new(ctx: &Context, id: Id) -> Self {
        let shown_key = id.with("dialog-last-shown");
        let pending_key = id.with("dialog-focus-pending");
        let frame = ctx.cumulative_frame_nr();
        ctx.data_mut(|data| {
            let reopened = data
                .get_temp::<u64>(shown_key)
                .is_none_or(|last| last + 1 < frame);
            data.insert_temp(shown_key, frame);
            if reopened {
                data.insert_temp(pending_key, true);
            }
            let pending = data.get_temp::<bool>(pending_key).unwrap_or(false);
            Self {
                key: pending.then_some(pending_key),
            }
        })
    }

    /// Focus `response` if focus is still to move and it can take it now.
    pub(crate) fn apply(self, ui: &Ui, response: &egui::Response) {
        if let Some(key) = self.key
            && !ui.is_sizing_pass()
            && response.enabled()
        {
            response.request_focus();
            ui.data_mut(|data| data.insert_temp(key, false));
        }
    }
}
