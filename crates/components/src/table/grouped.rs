//! Grouped tables (C17): the C08 table with a [`GroupHeader`] row before
//! each group of rows. Group headers are virtual rows like any other, so
//! long lists stay virtualised.

use std::collections::HashSet;
use std::hash::Hash;

use egui::{Align, Rect, Response, Ui, Vec2, pos2, vec2};

use super::body::{HeaderOptions, Selection, layout_widths, show_header_band, show_row};
use super::{SortState, Table};
use crate::design::size;
use crate::design::theme::Tone;
use crate::group_header::GroupHeader;
use crate::problem_ticks::ProblemTick;

/// One group of a grouped table: its header and the rows it shows (all of
/// them, or a window such as the first 20 of a large group).
pub struct TableGroup<'h, 'a, T> {
    /// The group's header, which also says whether the group is expanded;
    /// `None` for rows shown without a header (an ungrouped list).
    pub header: Option<GroupHeader<'h>>,
    pub rows: &'a [T],
}

impl<'h, 'a, T> TableGroup<'h, 'a, T> {
    /// An expanded group.
    pub fn new(header: GroupHeader<'h>, rows: &'a [T]) -> Self {
        Self {
            header: Some(header),
            rows,
        }
    }

    /// Rows without a group header: an ungrouped list, which still reports
    /// problem ticks.
    pub fn ungrouped(rows: &'a [T]) -> Self {
        Self { header: None, rows }
    }

    /// Collapse or expand the group ([`GroupHeader::expanded`]); a
    /// collapsed group shows only its header.
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.header = self.header.map(|header| header.expanded(expanded));
        self
    }

    /// The rows the group shows: none while it is collapsed.
    fn shown_rows(&self) -> &'a [T] {
        if self.header.as_ref().is_none_or(GroupHeader::is_expanded) {
            self.rows
        } else {
            &[]
        }
    }
}

/// What a grouped table did this frame.
#[derive(Debug, Default)]
pub struct GroupedTableResponse {
    /// The group whose chip was clicked: collapse or expand it.
    pub toggled: Option<usize>,
    /// Where the problem rows sit, for [`crate::ProblemTicks`]: as fractions of the
    /// list, or of the viewport when the list is shorter than that.
    pub ticks: Vec<ProblemTick>,
    /// The visible part of the list as `(start, length)` fractions, the
    /// ticks' thumb.
    pub visible: (f32, f32),
    /// The list's full height, header rows included.
    pub content_height: f32,
    /// The sortable column whose header was clicked
    /// ([`Table::show_grouped_configurable`]).
    pub sort_clicked: Option<String>,
}

/// A row of a grouped table's body.
enum Entry {
    /// The header of group `group`.
    Header { group: usize },
    /// Row `row` of group `group`, the `display`-th data row of the table.
    Row {
        group: usize,
        row: usize,
        display: usize,
    },
}

impl Entry {
    fn height(&self) -> f32 {
        match self {
            Self::Header { .. } => size::GROUP_HEADER,
            Self::Row { .. } => size::ROW_PITCH,
        }
    }
}

/// The entries of `groups`, top to bottom, with each entry's top.
fn entries<T>(groups: &[TableGroup<'_, '_, T>]) -> (Vec<(f32, Entry)>, f32) {
    let mut list = Vec::new();
    let mut top = 0.0;
    let mut display = 0;
    for (group, table_group) in groups.iter().enumerate() {
        if table_group.header.is_some() {
            let header = Entry::Header { group };
            let height = header.height();
            list.push((top, header));
            top += height;
        }
        for row in 0..table_group.shown_rows().len() {
            list.push((
                top,
                Entry::Row {
                    group,
                    row,
                    display,
                },
            ));
            top += size::ROW_PITCH;
            display += 1;
        }
    }
    (list, top)
}

impl Table {
    /// Show the table as groups of rows, each led by its [`GroupHeader`],
    /// under one header. With `selection` (a selectable table) the
    /// checkboxes select by `key`, Shift-click selecting a range within a
    /// group. `problem` names the tone of each problem row, reported as
    /// [`GroupedTableResponse::ticks`]. [`Table::inspected_row`] and
    /// [`Table::force_row_hovered`] count data rows across groups.
    #[allow(clippy::too_many_arguments)]
    pub fn show_grouped<'a, T, K>(
        self,
        ui: &mut Ui,
        groups: Vec<TableGroup<'_, 'a, T>>,
        selection: Option<&mut HashSet<K>>,
        key: impl Fn(&T) -> Option<K>,
        sort: Option<&SortState>,
        problem: impl Fn(&T) -> Option<Tone>,
        mut render_cell: impl FnMut(&mut Ui, &'a T, usize),
    ) -> GroupedTableResponse
    where
        K: Eq + Hash + Clone,
    {
        self.show_grouped_with(
            ui,
            groups,
            selection,
            key,
            HeaderOptions {
                sort,
                ..HeaderOptions::default()
            },
            problem,
            &mut render_cell,
            &mut |_, _, _| {},
        )
    }

    /// [`Self::show_grouped`] with resizable columns (persisted by
    /// `resized`), a callback for every header cell and one for every
    /// cell's response, under a horizontal scroll area for columns that no
    /// longer fit. The sortable column whose header was clicked is reported
    /// as [`GroupedTableResponse::sort_clicked`].
    #[allow(clippy::too_many_arguments)]
    pub fn show_grouped_configurable<'a, T, K>(
        self,
        ui: &mut Ui,
        groups: Vec<TableGroup<'_, 'a, T>>,
        selection: Option<&mut HashSet<K>>,
        key: impl Fn(&T) -> Option<K>,
        sort: Option<&SortState>,
        problem: impl Fn(&T) -> Option<Tone>,
        mut render_header: impl FnMut(&Response, &str, &str, bool),
        mut resized: impl FnMut(&str, f32),
        mut render_cell: impl FnMut(&mut Ui, &'a T, usize),
        mut render_row: impl FnMut(&Response, &'a T, usize),
    ) -> GroupedTableResponse
    where
        K: Eq + Hash + Clone,
    {
        self.show_grouped_with(
            ui,
            groups,
            selection,
            key,
            HeaderOptions {
                sort,
                click_to_sort: true,
                resize: Some(&mut resized),
                on_header: Some(&mut render_header),
                horizontal_scroll: true,
            },
            problem,
            &mut render_cell,
            &mut render_row,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn show_grouped_with<'a, T, K>(
        self,
        ui: &mut Ui,
        groups: Vec<TableGroup<'_, 'a, T>>,
        selection: Option<&mut HashSet<K>>,
        key: impl Fn(&T) -> Option<K>,
        mut header_options: HeaderOptions<'_>,
        problem: impl Fn(&T) -> Option<Tone>,
        render_cell: &mut impl FnMut(&mut Ui, &'a T, usize),
        render_row: &mut impl FnMut(&Response, &'a T, usize),
    ) -> GroupedTableResponse
    where
        K: Eq + Hash + Clone,
    {
        debug_assert!(
            selection.is_none() || self.is_selectable,
            "selectable tables must call .selectable()"
        );
        let original_spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let mut selection = selection.map(|set| Selection { set, key: &key });
        let widths = layout_widths(&self, selection.is_some(), ui.available_width());
        let content_width = widths.iter().sum::<f32>();
        let available_height = ui.available_height();
        let horizontal_scroll = header_options.horizontal_scroll;

        let (list, content_height) = entries(&groups);
        let mut response = GroupedTableResponse {
            content_height,
            ..GroupedTableResponse::default()
        };
        // Where each problem row starts, for ticks once the viewport is known.
        let problem_tops: Vec<(f32, Tone)> = list
            .iter()
            .filter_map(|(top, entry)| match entry {
                Entry::Row { group, row, .. } => {
                    problem(&groups[*group].rows[*row]).map(|tone| (*top, tone))
                }
                Entry::Header { .. } => None,
            })
            .collect();
        // "Select all" selects the rows shown, not those of collapsed groups.
        let shown: Vec<&'a [T]> = groups.iter().map(TableGroup::shown_rows).collect();
        let (mut headers, rows): (Vec<Option<GroupHeader<'_>>>, Vec<_>) = groups
            .into_iter()
            .map(|group| (group.header, group.rows))
            .unzip();

        let mut draw = |ui: &mut Ui| {
            let (layout, width, sort_clicked) = show_header_band(
                &self,
                ui,
                &widths,
                &mut shown.iter().flat_map(|rows| rows.iter()),
                selection.as_mut(),
                &mut header_options,
            );
            response.sort_clicked = sort_clicked;
            let body_height = (available_height - size::TABLE_HEADER).max(0.0);
            let offset = usize::from(selection.is_some());
            let output = crate::scroll::vertical()
                .id_salt(self.id.with("grouped-body"))
                .max_height(body_height)
                .auto_shrink([false, !self.fill_available_height])
                .show_viewport(ui, |ui, viewport| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    ui.set_height(content_height);
                    let origin = ui.min_rect().min;
                    for (top, entry) in &list {
                        let height = entry.height();
                        let rect = Rect::from_min_size(
                            pos2(origin.x, origin.y + top),
                            vec2(width, height),
                        );
                        if let Entry::Row { display, .. } = entry
                            && self.scroll_to_row == Some(*display)
                        {
                            ui.scroll_to_rect(rect, Some(Align::Center));
                        }
                        if top + height < viewport.top() || *top > viewport.bottom() {
                            continue;
                        }
                        match entry {
                            Entry::Header { group } => {
                                if let Some(header) = headers[*group].take() {
                                    let id = self.id.with(("group", *group));
                                    if header.paint(ui, rect, id).clicked() {
                                        response.toggled = Some(*group);
                                    }
                                }
                            }
                            Entry::Row {
                                group,
                                row,
                                display,
                            } => {
                                let items = rows[*group];
                                show_row(
                                    &self,
                                    ui,
                                    &layout,
                                    offset,
                                    rect,
                                    *display,
                                    &items[*row],
                                    selection.as_mut(),
                                    items,
                                    *row,
                                    render_cell,
                                    render_row,
                                );
                            }
                        }
                    }
                });
            // A list shorter than its viewport is laid out against the viewport, so
            // its ticks sit beside their rows.
            let viewport = output.inner_rect.height();
            let extent = content_height.max(viewport);
            if extent > 0.0 {
                response.visible = (output.state.offset.y / extent, (viewport / extent).min(1.0));
                response.ticks = problem_tops
                    .iter()
                    .map(|(top, tone)| ProblemTick {
                        position: top / extent,
                        tone: *tone,
                    })
                    .collect();
            }
        };
        if horizontal_scroll {
            crate::scroll::horizontal()
                .id_salt(self.id.with("horizontal-scroll"))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_min_width(content_width);
                    draw(ui);
                });
        } else {
            draw(ui);
        }
        ui.spacing_mut().item_spacing = original_spacing;
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapsed_groups_keep_their_header_and_rows_count_across_groups() {
        let first = [1, 2];
        let second = [3];
        let groups = vec![
            TableGroup::new(GroupHeader::new("a", 2), &first[..]),
            TableGroup::new(GroupHeader::new("b", 1), &second[..]).expanded(false),
            TableGroup::new(GroupHeader::new("c", 1), &second[..]),
        ];
        let (list, height) = entries(&groups);
        let tops: Vec<f32> = list.iter().map(|(top, _)| *top).collect();
        let header = size::GROUP_HEADER;
        let pitch = size::ROW_PITCH;
        assert_eq!(
            tops,
            vec![
                0.0,
                header,
                header + pitch,
                header + 2.0 * pitch,
                2.0 * header + 2.0 * pitch,
                3.0 * header + 2.0 * pitch,
            ]
        );
        assert_eq!(height, 3.0 * header + 3.0 * pitch);
        assert!(matches!(list[5].1, Entry::Row { display: 2, .. }));
    }
}
