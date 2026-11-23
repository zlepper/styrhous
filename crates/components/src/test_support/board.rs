//! The chrome of a Harbour component board (`docs/design-oracles/harbour/src/C*.html`),
//! so a board scene test only lays out its own components.
//!
//! Every component board is 1536 × 1024 on `surface-canvas` with the same
//! header — a caption eyebrow in `accent-text` (`.t-cap`), the display title
//! 4 points below it (`.t-display`) and a description paragraph 6 points
//! below that (`.t-body.c2`, at most 900 points wide) — at 48 points from the
//! left and 40 from the top. The board's own content starts 168 points from
//! the top, inset 48 points left and right and 40 from the bottom.
//!
//! Content is laid out with flex-box-like helpers that control every gap
//! explicitly ([`section`], [`row`], [`column`]); inside them egui's item
//! spacing is zero, so positions match the CSS.
//!
//! ```ignore
//! fn scene(ui: &mut egui::Ui) {
//!     board(ui, BoardHeader { eyebrow: "C01 · …", title: "Icons & status", description: "…" }, |ui| {
//!         section(ui, "Sizes", |ui| { /* components */ });
//!     });
//! }
//! ```

use egui::{Align, Layout, Rect, Response, Ui, UiBuilder, Vec2, pos2, vec2};

use crate::design::theme::ThemeExt;
use crate::design::type_scale;

use super::EGUI_TEST_SIZE;

/// Horizontal inset of the header and content.
const BOARD_INSET_X: f32 = 48.0;
/// Top of the header.
const HEADER_TOP: f32 = 40.0;
/// Gap between the eyebrow and the title.
const EYEBROW_TO_TITLE: f32 = 4.0;
/// Gap between the title and the description.
const TITLE_TO_DESCRIPTION: f32 = 6.0;
/// Widest a description runs before wrapping.
const DESCRIPTION_MAX_WIDTH: f32 = 900.0;
/// Top of the board's content.
const CONTENT_TOP: f32 = 168.0;
/// Bottom inset of the board's content.
const CONTENT_BOTTOM_INSET: f32 = 40.0;
/// Gap between a section label and its contents.
const SECTION_LABEL_GAP: f32 = 12.0;

/// The header of a component board.
#[derive(Clone, Copy, Debug)]
pub struct BoardHeader<'a> {
    /// The caption above the title, for example
    /// `"C01 · components::icons, components::status"` (upper-cased when drawn).
    pub eyebrow: &'a str,
    /// The board title.
    pub title: &'a str,
    /// The description paragraph under the title.
    pub description: &'a str,
}

/// Paint a board: the canvas, `header`, and `add_contents` laid out top-down
/// in the content area with zero item spacing.
pub fn board<R>(
    ui: &mut Ui,
    header: BoardHeader<'_>,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    board_with_description_width(ui, header, DESCRIPTION_MAX_WIDTH, add_contents)
}

// C09
/// A [`board`] whose description wraps at `description_width` instead of
/// 900 points (C09 sets `max-width: 960px`).
pub fn board_with_description_width<R>(
    ui: &mut Ui,
    header: BoardHeader<'_>,
    description_width: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    let theme = ui.tokens();
    let board_rect = Rect::from_min_size(pos2(0.0, 0.0), EGUI_TEST_SIZE);
    ui.ctx()
        .layer_painter(egui::LayerId::background())
        .rect_filled(board_rect, 0, theme.surface.canvas);

    let header_rect = Rect::from_min_max(
        pos2(BOARD_INSET_X, HEADER_TOP),
        pos2(board_rect.right() - BOARD_INSET_X, CONTENT_TOP),
    );
    let mut header_ui = child(ui, header_rect, Layout::top_down(Align::Min));
    type_scale::CAPTION.label(&mut header_ui, header.eyebrow, theme.accent.text);
    header_ui.add_space(EYEBROW_TO_TITLE);
    type_scale::DISPLAY.label(&mut header_ui, header.title, theme.text.primary);
    header_ui.add_space(TITLE_TO_DESCRIPTION);
    header_ui.set_max_width(description_width);
    type_scale::BODY.label(&mut header_ui, header.description, theme.text.secondary);

    let content_rect = Rect::from_min_max(
        pos2(BOARD_INSET_X, CONTENT_TOP),
        board_rect.max - vec2(BOARD_INSET_X, CONTENT_BOTTOM_INSET),
    );
    let mut content_ui = child(ui, content_rect, Layout::top_down(Align::Min));
    add_contents(&mut content_ui)
}

/// A child of `ui` filling `rect` with `layout` and zero item spacing.
fn child(ui: &mut Ui, rect: Rect, layout: Layout) -> Ui {
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect).layout(layout));
    child.spacing_mut().item_spacing = Vec2::ZERO;
    child
}

/// A labelled section (`.col` with a `.t-cap` label): the caption in
/// `text-muted`, then `add_contents` 12 points below it.
pub fn section<R>(ui: &mut Ui, label: &str, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    column(ui, 0.0, |ui| {
        type_scale::CAPTION.label(ui, label, ui.tokens().text.muted);
        ui.add_space(SECTION_LABEL_GAP);
        add_contents(ui)
    })
}

/// A flex row (`.row`) `height` points tall whose items are `gap` points
/// apart and aligned on the cross axis by `align` (`Min` = `flex-start`,
/// `Center`, `Max` = `flex-end`). The row is as wide as its items.
pub fn row<R>(
    ui: &mut Ui,
    height: f32,
    gap: f32,
    align: Align,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), height),
        Layout::left_to_right(align),
        |ui| {
            ui.spacing_mut().item_spacing = vec2(gap, 0.0);
            ui.set_min_height(height);
            add_contents(ui)
        },
    )
    .inner
}

/// A flex column (`.col` with `align-items: flex-start`) whose items are
/// `gap` points apart. The column is as wide and tall as its items.
pub fn column<R>(ui: &mut Ui, gap: f32, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    column_sized(ui, 0.0, gap, add_contents)
}

/// A [`column`] that is aligned in its row as if it were `height` points
/// tall. egui places a child before laying it out, so a column centred or
/// bottom-aligned in a [`row`] must state its height up front.
pub fn column_sized<R>(
    ui: &mut Ui,
    height: f32,
    gap: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), height),
        Layout::top_down(Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, gap);
            add_contents(ui)
        },
    )
    .inner
}

/// A box of exactly `size` (a grid cell, a fixed-width table column) laid
/// out with `layout` and items `gap` points apart along its main axis.
pub fn cell<R>(
    ui: &mut Ui,
    size: Vec2,
    layout: Layout,
    gap: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let mut cell_ui = child(ui, rect, layout);
    if layout.main_dir().is_horizontal() {
        cell_ui.spacing_mut().item_spacing.x = gap;
    } else {
        cell_ui.spacing_mut().item_spacing.y = gap;
    }
    add_contents(&mut cell_ui)
}

/// Gap between a demo and the caption under it (`.col` gap 8).
pub const DEMO_CAPTION_GAP: f32 = 8.0;

/// The height of a [`captioned`] demo `demo_height` points tall.
pub fn captioned_height(demo_height: f32) -> f32 {
    demo_height + DEMO_CAPTION_GAP + type_scale::META.line_height()
}

/// A demo above its muted caption (`.col` gap 8 with a `.t-meta.c3`),
/// aligned in its row as [`captioned_height`] points.
pub fn captioned(ui: &mut Ui, demo_height: f32, caption: &str, demo: impl FnOnce(&mut Ui)) {
    column_sized(ui, captioned_height(demo_height), DEMO_CAPTION_GAP, |ui| {
        demo(ui);
        type_scale::META.label(ui, caption, ui.tokens().text.muted);
    });
}

/// Render the board `scene` at the board size and compare it with its
/// snapshot `boards/{name}` (the file the board's manifest entry names).
#[track_caller]
pub fn snapshot_board(name: &str, scene: impl FnMut(&mut Ui) + 'static) {
    use super::UiHarnessSnapshot as _;
    let mut harness = egui_kittest::Harness::builder().build_ui(scene);
    super::setup_egui(&mut harness);
    harness.run();
    harness.ui_harness(format!("boards/{name}"));
}

// C16
/// Gap between a section caption and what follows it on the C16–C20
/// boards (`.t-cap` with `margin-bottom: 10px`).
pub const SECTION_CAPTION_GAP: f32 = 10.0;

/// A section caption (`.t-cap`, `text-muted`) followed by
/// [`SECTION_CAPTION_GAP`].
pub fn section_caption(ui: &mut Ui, label: &str) {
    type_scale::CAPTION.label(ui, label, ui.tokens().text.muted);
    ui.add_space(SECTION_CAPTION_GAP);
}

/// Lay `add_contents` out top-down in a column `width` points wide whose
/// top-left corner is at `pos` on the board (an absolutely positioned
/// block), with zero item spacing.
pub fn placed<R>(
    ui: &mut Ui,
    pos: egui::Pos2,
    width: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    let rect = Rect::from_min_max(pos, pos2(pos.x + width, EGUI_TEST_SIZE.y));
    let mut block = child(ui, rect, Layout::top_down(Align::Min));
    add_contents(&mut block)
}

/// Gap between a state demo and its caption on the C16–C20 boards (`.col`
/// gap 6).
pub const STATE_CAPTION_GAP: f32 = 6.0;
/// Gap between the captioned states of a board's column (`.col` gap 14).
pub const STATE_GAP: f32 = 14.0;
/// The description width of the C09 and C16–C20 boards (`max-width: 960px`).
pub const WIDE_DESCRIPTION: f32 = 960.0;

/// A state demo above its muted caption, [`STATE_CAPTION_GAP`] apart
/// (`.col` gap 6 with a `.t-meta.c3`).
pub fn state(ui: &mut Ui, demo_height: f32, caption: &str, demo: impl FnOnce(&mut Ui)) {
    let height = demo_height + STATE_CAPTION_GAP + type_scale::META.line_height();
    column_sized(ui, height, STATE_CAPTION_GAP, |ui| {
        demo(ui);
        type_scale::META.label(ui, caption, ui.tokens().text.muted);
    });
}

// C17
/// One run of a rules list item: text, or a keycap set inline.
#[derive(Clone, Copy, Debug)]
pub enum Run {
    Text(&'static str),
    Key(&'static str),
}

/// Holds the place of an inline keycap in a laid-out paragraph.
const ZERO_WIDTH: char = '\u{200B}';
/// Indent of a rules list's text (`.doc ul { padding-left: 16px }`).
const LIST_INDENT: f32 = crate::design::space::S16;
/// Gap between the items of a rules list (`.doc li { margin-bottom: 4px }`).
const LIST_GAP: f32 = crate::design::space::S4;
/// Distance from a rules list item's disc to its text.
const DISC_OFFSET: f32 = 13.0;

/// A rules list (`.doc ul`) `width` points wide: one disc item per entry
/// of `items`, its runs wrapped at the column width with keycaps laid out
/// inline, in UI 13/20 `text-secondary`.
pub fn rules_list(ui: &mut Ui, width: f32, items: &[&[Run]]) {
    column(ui, LIST_GAP, |ui| {
        for runs in items {
            rule(ui, width, runs);
        }
    });
}

fn rule(ui: &mut Ui, width: f32, runs: &[Run]) {
    let role = type_scale::UI;
    let color = ui.tokens().text.secondary;
    let top_left = ui.cursor().min;
    // Chromium's disc marker: a third of the font size across, centred
    // on the first line, its centre 13 points before the text.
    ui.painter().circle_filled(
        pos2(
            top_left.x + LIST_INDENT - DISC_OFFSET,
            top_left.y + role.line_height() / 2.0,
        ),
        role.size() / 6.0,
        color,
    );
    ui.horizontal(|ui| {
        ui.add_space(LIST_INDENT);
        keyed_paragraph(ui, role, color, width - LIST_INDENT, runs);
    });
}

/// A paragraph of `runs` in `role` and `color` wrapped at `width`, with
/// its keycaps set inline on the text's baseline (`.kbd` in running text).
pub fn keyed_paragraph(
    ui: &mut Ui,
    role: type_scale::TextRole,
    color: egui::Color32,
    width: f32,
    runs: &[Run],
) -> Response {
    use crate::keycap::Keycap;
    let format = role.text_format(ui.ctx(), color);
    let mut job = egui::text::LayoutJob::default();
    let mut keys = Vec::new();
    for run in runs {
        match run {
            Run::Text(text) => job.append(text, 0.0, format.clone()),
            Run::Key(key) => {
                // Reserve the keycap's width with a zero-width joiner.
                let key_width = Keycap::new(key).size(ui).x;
                keys.push(*key);
                job.append(&ZERO_WIDTH.to_string(), key_width, format.clone());
            }
        }
    }
    // CSS lets the space at the end of a line hang past its edge; egui
    // counts it, so allow one space more.
    let space = role
        .galley(ui.ctx(), " ", color, f32::INFINITY)
        .rect
        .width();
    job.wrap.max_width = width + space;
    let galley = role.galley_from_job(ui.ctx(), job);
    let (rect, response) =
        ui.allocate_exact_size(vec2(width, galley.rect.height()), egui::Sense::hover());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, galley.text()));
    let ppp = ui.pixels_per_point();
    // The keycap's text sits on the line's baseline.
    let key_baseline = (crate::design::size::KEY_HEIGHT - type_scale::KEY.line_height()) / 2.0
        + type_scale::KEY.css_baseline(ppp);
    // Lay each row out again as runs of text between keycaps, every run
    // and keycap at the exact width of what precedes it: egui rounds glyphs
    // to the pixel grid relative to the start of their paragraph, Chromium
    // keeps them where they fall, and `paint_galley` puts a single row at
    // its sub-pixel position.
    let mut keys = keys.into_iter();
    let first_row_top = galley.rows.first().map_or(0.0, |row| row.pos.y);
    for row in &galley.rows {
        let line_top = rect.top() + row.pos.y - first_row_top;
        let key_top = line_top + role.css_baseline(ppp) - key_baseline;
        let mut x = rect.left();
        let mut run = String::new();
        let flush = |run: &mut String, x: &mut f32| {
            if run.is_empty() {
                return;
            }
            let line = role.galley(ui.ctx(), run, color, f32::INFINITY);
            let width = line.rect.width();
            type_scale::paint_galley(ui.painter(), pos2(*x, line_top), line, color);
            *x += width;
            run.clear();
        };
        for glyph in &row.glyphs {
            if glyph.chr != ZERO_WIDTH {
                run.push(glyph.chr);
                continue;
            }
            flush(&mut run, &mut x);
            if let Some(key) = keys.next() {
                let keycap = Keycap::new(key);
                let key_size = keycap.size(ui);
                keycap.paint(ui, Rect::from_min_size(pos2(x, key_top), key_size));
                x += key_size.x;
            }
        }
        flush(&mut run, &mut x);
    }
    response
}

// C18
/// A [`board`] whose description has keycaps set inline (C18 "or ⌘K").
pub fn board_with_keyed_description<R>(
    ui: &mut Ui,
    eyebrow: &str,
    title: &str,
    description: &[Run],
    description_width: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    let header = BoardHeader {
        eyebrow,
        title,
        description: "",
    };
    board_with_description_width(ui, header, description_width, |ui| {
        let theme = ui.tokens();
        let top = HEADER_TOP
            + type_scale::CAPTION.line_height()
            + EYEBROW_TO_TITLE
            + type_scale::DISPLAY.line_height()
            + TITLE_TO_DESCRIPTION;
        let rect = Rect::from_min_size(pos2(BOARD_INSET_X, top), vec2(description_width, 100.0));
        let mut paragraph = child(ui, rect, Layout::top_down(Align::Min));
        keyed_paragraph(
            &mut paragraph,
            type_scale::BODY,
            theme.text.secondary,
            description_width,
            description,
        );
        add_contents(ui)
    })
}
