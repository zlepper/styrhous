//! The geometry and painting shared by [`super::Table`] and
//! [`crate::ReorderableTable`]: column positions, header cells, row tiles
//! and the child `Ui` a cell's contents are laid out in.

use egui::{Color32, CornerRadius, Pos2, Rect, Response, Shape, Stroke, Ui, pos2, vec2};

pub(crate) use super::CellAlign;
use super::SortDirection;
use crate::design::theme::{Theme, ThemeExt};
use crate::design::type_scale::{self, TextRole};
use crate::design::{radius, size, space};
use crate::icons::Icon;

/// Column headers (`.th`): Plex Sans Condensed 11/16 · 500, caps, +0.08 em.
pub(crate) const HEADER_ROLE: TextRole = type_scale::CAPTION.with_letter_spacing(0.08);
/// Horizontal padding of a cell (`.td`).
pub(crate) const CELL_PADDING_X: f32 = space::S12;
/// Gap between the items of a cell (`.td` gap 6).
pub(crate) const CELL_GAP: f32 = space::S6;
/// The sort chevron in a header (`.ic.s12`).
const SORT_ICON: f32 = size::ICON_XS;
/// Inset of a column separator from the top and bottom of the header.
const SEPARATOR_INSET: f32 = space::S12;
/// Inset of the accent line of a column being resized.
const RESIZING_INSET: f32 = space::S4;
/// Width of the accent line of a column being resized.
const RESIZING_WIDTH: f32 = 2.0;
/// The vertical radius of a row tile's bottom corners: the tile is clipped
/// to its padding box, inside the 4 point gap (`border-bottom`) that shares
/// the row's 12 point corner radius.
const TILE_BOTTOM_RADIUS_Y: f32 = radius::ROW as f32 - size::ROW_GAP;
/// The corners of outlines drawn on a row tile (`.tr.is-gap::before`).
const TILE_OUTLINE_RADIUS: CornerRadius = CornerRadius {
    nw: radius::ROW,
    ne: radius::ROW,
    sw: radius::ROW - size::ROW_GAP as u8,
    se: radius::ROW - size::ROW_GAP as u8,
};
/// Segments per corner of a row tile's outline.
const CORNER_SEGMENTS: usize = 8;
/// Dash and gap length of a drop placeholder's dashed outline, in border
/// widths (Chromium's `dashed`).
const DASH_WIDTHS: f32 = 3.0;

/// The horizontal extent of every column of a table, left to right.
#[derive(Clone, Debug, Default)]
pub(crate) struct ColumnLayout {
    spans: Vec<(f32, f32)>,
}

impl ColumnLayout {
    /// Columns of `widths` starting at `left`.
    pub fn new(left: f32, widths: impl IntoIterator<Item = f32>) -> Self {
        let mut x = left;
        let spans = widths
            .into_iter()
            .map(|width| {
                let span = (x, width);
                x += width;
                span
            })
            .collect();
        Self { spans }
    }

    /// The rect of column `index` between `top` and `top + height`.
    pub fn cell(&self, index: usize, top: f32, height: f32) -> Rect {
        let (left, width) = self.spans[index];
        Rect::from_min_size(pos2(left, top), vec2(width, height))
    }
}

/// How a header's column separator (`.rs::after`) is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Separator {
    None,
    /// The 1 point divider between labelled columns.
    Rest,
    /// The 2 point accent line of a column being resized.
    Resizing,
}

/// A header cell to paint.
pub(crate) struct HeaderCell<'a> {
    pub label: &'a str,
    pub align: CellAlign,
    pub sort: Option<SortDirection>,
    pub hovered: bool,
    pub separator: Separator,
}

/// Paint a header cell (`.th .td`) into `rect`, exposing its label as the
/// text of `response`.
pub(crate) fn paint_header_cell(ui: &Ui, rect: Rect, response: &Response, cell: &HeaderCell<'_>) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let theme = ui.tokens();
    let painter = ui.painter();
    if cell.hovered {
        painter.rect_filled(rect, radius::MENU_ITEM, theme.control.fill);
    }
    match cell.separator {
        Separator::None => {}
        Separator::Rest => {
            let line = Rect::from_min_max(
                pos2(rect.right() - size::HAIRLINE, rect.top() + SEPARATOR_INSET),
                pos2(rect.right(), rect.bottom() - SEPARATOR_INSET),
            );
            painter.rect_filled(line, 0, theme.divider.base);
        }
        Separator::Resizing => {
            let line = Rect::from_min_max(
                pos2(rect.right() - RESIZING_WIDTH, rect.top() + RESIZING_INSET),
                pos2(rect.right(), rect.bottom() - RESIZING_INSET),
            );
            painter.rect_filled(line, 1, theme.accent.fill);
        }
    }
    if cell.label.is_empty() {
        return;
    }
    let color = if cell.hovered || cell.sort.is_some() {
        theme.text.primary
    } else {
        theme.text.muted
    };
    let padding = if cell.align == CellAlign::Center {
        0.0
    } else {
        CELL_PADDING_X
    };
    let sort_width = cell.sort.map_or(0.0, |_| CELL_GAP + SORT_ICON);
    let max_text = (rect.width() - 2.0 * padding - sort_width).max(0.0);
    let mut job = HEADER_ROLE.layout_job(ui.ctx(), cell.label, color, max_text);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = HEADER_ROLE.galley_from_job(ui.ctx(), job);
    let content = galley.rect.width() + sort_width;
    let left = match cell.align {
        CellAlign::Start => rect.left() + padding,
        CellAlign::End => rect.right() - padding - content,
        CellAlign::Center => rect.center().x - content / 2.0,
    };
    let top = rect.center().y - HEADER_ROLE.line_height() / 2.0;
    let text_right = left + galley.rect.width();
    let pos = pos2(left, top);
    // Read as a column header named by its response (not by its caps).
    type_scale::expose_text_runs_as(
        ui,
        response,
        pos,
        &galley,
        egui::accesskit::Role::ColumnHeader,
    );
    type_scale::paint_galley(ui.painter(), pos, galley, color);
    if let Some(direction) = cell.sort {
        let icon = match direction {
            SortDirection::Ascending => Icon::ChevronUp,
            SortDirection::Descending => Icon::ChevronDown,
        };
        let chevron = Rect::from_min_size(
            pos2(text_right + CELL_GAP, rect.center().y - SORT_ICON / 2.0),
            egui::Vec2::splat(SORT_ICON),
        );
        icon.paint(ui, chevron, theme.accent.text);
    }
}

/// The look of a row tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowLook {
    Rest,
    Hovered,
    Selected,
    SelectedHovered,
    /// The row whose blade is open.
    Inspected,
    /// Where a dragged row will land: a dashed outline.
    Gap,
    /// A row being dragged: raised with a drop shadow.
    Dragged,
}

impl RowLook {
    fn fill(self, theme: &Theme) -> Color32 {
        match self {
            Self::Rest => theme.components.row_bg,
            Self::Hovered => theme.components.row_bg_hover,
            Self::Selected => theme.accent.tint,
            Self::SelectedHovered => theme.accent.tint_hover,
            Self::Inspected => theme.item.selected,
            Self::Gap => Color32::TRANSPARENT,
            Self::Dragged => theme.surface.raised,
        }
    }
}

/// The tile of a row whose pitch is `row` (tile plus gap).
pub(crate) fn tile_rect(row: Rect) -> Rect {
    Rect::from_min_size(row.min, vec2(row.width(), size::ROW_TILE))
}

/// Paint the tile (`.tr`) of a row whose pitch is `row` in `look`.
pub(crate) fn paint_row_tile(ui: &Ui, row: Rect, look: RowLook) {
    paint_tile(ui, tile_rect(row), look);
}

/// Paint a row tile filling `tile` (normally [`tile_rect`]) in `look`.
pub(crate) fn paint_tile(ui: &Ui, tile: Rect, look: RowLook) {
    // The row's border box: the tile and the gap under it.
    let row = tile.with_max_y(tile.bottom() + size::ROW_GAP);
    if !ui.is_rect_visible(row) {
        return;
    }
    let theme = ui.tokens();
    let painter = ui.painter();
    if look == RowLook::Dragged {
        // The shadow follows the row's border box, its gap included.
        painter.add(theme.shadow.drag.shadow.as_shape(row, radius::ROW));
    }
    let fill = look.fill(&theme);
    if fill != Color32::TRANSPARENT {
        painter.add(Shape::convex_polygon(
            tile_outline(tile),
            fill,
            Stroke::NONE,
        ));
    }
    match look {
        RowLook::Inspected => {
            painter.rect_stroke(
                tile,
                TILE_OUTLINE_RADIUS,
                Stroke::new(size::CONTROL_STROKE, theme.accent.fill),
                egui::StrokeKind::Inside,
            );
        }
        RowLook::Gap => {
            // A CSS border: snapped to device pixels, dashes three widths long.
            let width = size::css_border_width(size::CONTROL_STROKE, ui.pixels_per_point());
            let inset = tile.shrink(width / 2.0);
            let mut points = rounded_outline(inset, TILE_OUTLINE_RADIUS);
            points.push(points[0]);
            painter.extend(Shape::dashed_line(
                &points,
                Stroke::new(width, theme.divider.strong),
                DASH_WIDTHS * width,
                DASH_WIDTHS * width,
            ));
        }
        _ => {}
    }
}

/// The outline of a row tile, clockwise: circular 12 point top corners and
/// elliptical bottom corners, 12 points wide and 8 tall.
fn tile_outline(tile: Rect) -> Vec<Pos2> {
    let r = f32::from(radius::ROW);
    let corners = [
        (pos2(tile.left() + r, tile.top() + r), vec2(r, r), 180.0),
        (pos2(tile.right() - r, tile.top() + r), vec2(r, r), 270.0),
        (
            pos2(tile.right() - r, tile.bottom() - TILE_BOTTOM_RADIUS_Y),
            vec2(r, TILE_BOTTOM_RADIUS_Y),
            0.0,
        ),
        (
            pos2(tile.left() + r, tile.bottom() - TILE_BOTTOM_RADIUS_Y),
            vec2(r, TILE_BOTTOM_RADIUS_Y),
            90.0,
        ),
    ];
    arcs(&corners)
}

/// The outline of `rect` with circular `corner_radius`, clockwise.
fn rounded_outline(rect: Rect, corner_radius: CornerRadius) -> Vec<Pos2> {
    let [nw, ne, se, sw] = [
        corner_radius.nw,
        corner_radius.ne,
        corner_radius.se,
        corner_radius.sw,
    ]
    .map(f32::from);
    let corners = [
        (pos2(rect.left() + nw, rect.top() + nw), vec2(nw, nw), 180.0),
        (
            pos2(rect.right() - ne, rect.top() + ne),
            vec2(ne, ne),
            270.0,
        ),
        (
            pos2(rect.right() - se, rect.bottom() - se),
            vec2(se, se),
            0.0,
        ),
        (
            pos2(rect.left() + sw, rect.bottom() - sw),
            vec2(sw, sw),
            90.0,
        ),
    ];
    arcs(&corners)
}

/// Quarter arcs around `(centre, radii, start angle in degrees)`, joined.
fn arcs(corners: &[(Pos2, egui::Vec2, f32); 4]) -> Vec<Pos2> {
    let mut points = Vec::with_capacity(4 * (CORNER_SEGMENTS + 1));
    for (center, radii, start) in corners {
        for step in 0..=CORNER_SEGMENTS {
            let angle = (start + 90.0 * step as f32 / CORNER_SEGMENTS as f32).to_radians();
            points.push(*center + vec2(radii.x * angle.cos(), radii.y * angle.sin()));
        }
    }
    points
}

/// The tag of a cell whose numbers are set in tabular figures.
const TABULAR_CELL: &str = "components::table::tabular";

/// Whether `ui` lays out (part of) a cell of a
/// [`super::TableColumnBuilder::tabular`] column.
pub(crate) fn in_tabular_cell(ui: &Ui) -> bool {
    ui.stack()
        .iter()
        .any(|frame| frame.info.tags.contains(TABULAR_CELL))
}

/// Lay a cell's contents out with `add_contents` in `rect` (the cell across
/// its tile), aligned as `align`, `inset` in from its padded edge, and
/// clipped to the cell.
///
/// egui places widgets before it knows their width, so a centred cell
/// remembers its contents' width under `width_id` (shared by the cells of a
/// column) and centres that width; it starts from a small control's width.
pub(crate) fn show_cell<R>(
    ui: &mut Ui,
    rect: Rect,
    align: CellAlign,
    inset: f32,
    tabular: bool,
    width_id: egui::Id,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    let (content, layout) = match align {
        CellAlign::Start => (
            rect.shrink2(vec2(inset, 0.0)),
            egui::Layout::left_to_right(egui::Align::Center),
        ),
        CellAlign::End => (
            rect.shrink2(vec2(inset, 0.0)),
            egui::Layout::right_to_left(egui::Align::Center),
        ),
        CellAlign::Center => {
            let width = ui
                .data(|data| data.get_temp::<f32>(width_id))
                .unwrap_or(size::CONTROL_SM)
                .min(rect.width());
            (
                Rect::from_center_size(rect.center(), vec2(width, rect.height())),
                egui::Layout::left_to_right(egui::Align::Center),
            )
        }
    };
    let mut info = egui::UiStackInfo::default();
    if tabular {
        info = info.with_tag(TABULAR_CELL);
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content)
            .layout(layout)
            .ui_stack_info(info),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    child.spacing_mut().item_spacing = vec2(CELL_GAP, 0.0);
    let inner = add_contents(&mut child);
    if align == CellAlign::Center {
        let width = child.min_rect().width();
        let previous = ui.data(|data| data.get_temp::<f32>(width_id));
        if width > 0.0 && previous != Some(width) {
            ui.data_mut(|data| data.insert_temp(width_id, width));
            ui.ctx()
                .request_discard("a centred table cell changed width");
        }
    }
    inner
}
