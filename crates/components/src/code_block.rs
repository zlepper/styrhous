//! Code blocks (`.codeblk`, C13): a short, read-only file or command on
//! the code surface, optionally under a header with its file name and a
//! Copy button.

use egui::{Align, Layout, Margin, Sense, Ui, UiBuilder, pos2, vec2};

use crate::button::{Button, ButtonSize};
use crate::design::theme::ThemeExt;
use crate::design::type_scale::{self, TextRole};
use crate::design::{radius, space};
use crate::icons::Icon;

/// The file name above a code block (`.w5.mono`, 12/20 · 500).
const FILENAME: TextRole = type_scale::MONO_ROW_STRONG;
/// Gap between the header and the block.
const HEADER_GAP: f32 = space::S6;
/// Padding inside the block (`padding: 10px 14px`).
const PADDING: Margin = Margin::symmetric(14, 10);
const COMPACT_PADDING: Margin = Margin::symmetric(10, 6);

/// The response of a [`CodeBlock`].
pub struct CodeBlockResponse {
    /// Whether the Copy button copied the code this frame.
    pub copied: bool,
}

/// A code block: `code` in mono 12/18 on `code-surface`, whitespace kept,
/// filling the available width. Lines too long for it are clipped.
#[must_use = "call `show` to add the block"]
pub struct CodeBlock<'a> {
    code: &'a str,
    filename: Option<&'a str>,
    wrap: bool,
    compact: bool,
}

impl<'a> CodeBlock<'a> {
    pub fn new(code: &'a str) -> Self {
        Self {
            code,
            filename: None,
            wrap: false,
            compact: false,
        }
    }

    // C11
    /// Wrap long lines (CSS `white-space: pre-wrap`) and let the text be
    /// selected, for technical detail such as an error dialog's.
    pub fn wrap(mut self) -> Self {
        self.wrap = true;
        self
    }

    /// Pad the block 10 by 6 points instead of 14 by 10, for a single line
    /// of code beside a button (A09).
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    /// Show a header with `filename` and a Copy button above the block.
    pub fn filename(mut self, filename: &'a str) -> Self {
        self.filename = Some(filename);
        self
    }

    pub fn show(self, ui: &mut Ui) -> CodeBlockResponse {
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            self.show_rows(ui)
        })
        .inner
    }

    fn show_rows(self, ui: &mut Ui) -> CodeBlockResponse {
        let theme = ui.tokens();
        let mut copied = false;
        if let Some(filename) = self.filename {
            let height = ButtonSize::Sm.height();
            let (row, row_response) =
                ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
            let galley = FILENAME.galley(ui.ctx(), filename, theme.text.primary, f32::INFINITY);
            let pos = pos2(row.left(), row.center().y - galley.rect.height() / 2.0);
            let id = row_response.id.with("filename");
            type_scale::text_at(ui, id, pos, galley, filename, theme.text.primary);
            let mut actions = ui.new_child(
                UiBuilder::new()
                    .max_rect(row)
                    .layout(Layout::right_to_left(Align::Center)),
            );
            let copy = Button::ghost("Copy")
                .size(ButtonSize::Sm)
                .leading_icon(Icon::ClipboardDocument)
                .accessibility_label(format!("Copy {filename}"))
                .show(&mut actions);
            if copy.clicked() {
                ui.ctx().copy_text(self.code.to_owned());
                copied = true;
            }
            ui.add_space(HEADER_GAP);
        }
        let role = type_scale::MONO_SMALL;
        egui::Frame::new()
            .fill(theme.code.surface)
            .corner_radius(radius::CODE_BLOCK)
            .inner_margin(if self.compact {
                COMPACT_PADDING
            } else {
                PADDING
            })
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                if self.wrap {
                    let galley =
                        role.galley(ui.ctx(), self.code, theme.code.text, ui.available_width());
                    ui.add(egui::Label::new(galley).selectable(true));
                    return;
                }
                let galley = role.galley(ui.ctx(), self.code, theme.code.text, f32::INFINITY);
                let width = galley.rect.width().min(ui.available_width());
                let (rect, response) =
                    type_scale::allocate_text(ui, vec2(width, galley.rect.height()), self.code);
                let clip = ui.clip_rect().intersect(ui.max_rect());
                let painter_ui = &mut ui.new_child(UiBuilder::new().max_rect(rect));
                painter_ui.set_clip_rect(clip);
                type_scale::paint_text(painter_ui, &response, rect.min, galley, theme.code.text);
            });
        CodeBlockResponse { copied }
    }
}
