//! Grouped tables (C17): the C08 table with a [`GroupHeader`] row before
//! each group of rows. Group headers are virtual rows like any other, so
//! long lists stay virtualised.

use std::collections::HashSet;
use std::hash::Hash;

use egui::{Rect, Ui, Vec2, pos2, vec2};

use super::body::{HeaderOptions, Selection, layout_widths, show_header_band, show_row};
use super::{SortState, Table};
use crate::design::size;
use crate::design::theme::Tone;
use crate::group_header::GroupHeader;
use crate::problem_ticks::ProblemTick;

/// One group of a grouped table: its header and the rows it shows (all of
/// them, or a window such as the first 20 of a large group).
pub struct TableGroup<'h, 'a, T> {
    /// The group's header, which also says whether the group is expanded.
    pub header: GroupHeader<'h>,
    pub rows: &'a [T],
}

impl<'h, 'a, T> TableGroup<'h, 'a, T> {
    /// An expanded group.
    pub fn new(header: GroupHeader<'h>, rows: &'a [T]) -> Self {
        Self { header, rows }
    }

    /// Collapse or expand the group ([`GroupHeader::expanded`]); a
    /// collapsed group shows only its header.
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.header = self.header.expanded(expanded);
        self
    }

    /// The rows the group shows: none while it is collapsed.
    fn shown_rows(&self) -> &'a [T] {
        if self.header.is_expanded() {
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
    /// Where the problem rows sit in the list, for [`crate::ProblemTicks`].
    pub ticks: Vec<ProblemTick>,
    /// The visible part of the list as `(start, length)` fractions, the
    /// ticks' thumb.
    pub visible: (f32, f32),
    /// The list's full height, header rows included.
    pub content_height: f32,
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
        let header = Entry::Header { group };
        let height = header.height();
        list.push((top, header));
        top += height;
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
        debug_assert!(
            selection.is_none() || self.is_selectable,
            "selectable tables must call .selectable()"
        );
        let original_spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let mut selection = selection.map(|set| Selection {
            set,
            key: &key,
            group: 0,
        });
        let widths = layout_widths(&self, selection.is_some(), ui.available_width());
        let (layout, width, _) = show_header_band(
            &self,
            ui,
            &widths,
            // "Select all" selects the rows shown, not those of collapsed groups.
            &mut groups.iter().flat_map(|group| group.shown_rows().iter()),
            selection.as_mut(),
            &mut HeaderOptions {
                sort,
                ..HeaderOptions::default()
            },
        );

        let (list, content_height) = entries(&groups);
        let mut response = GroupedTableResponse {
            content_height,
            ..GroupedTableResponse::default()
        };
        if content_height > 0.0 {
            response.ticks = list
                .iter()
                .filter_map(|(top, entry)| match entry {
                    Entry::Row { group, row, .. } => {
                        problem(&groups[*group].rows[*row]).map(|tone| ProblemTick {
                            position: top / content_height,
                            tone,
                        })
                    }
                    Entry::Header { .. } => None,
                })
                .collect();
        }

        let body_height = ui.available_height().max(0.0);
        let offset = usize::from(selection.is_some());
        let (mut headers, rows): (Vec<Option<GroupHeader<'_>>>, Vec<_>) = groups
            .into_iter()
            .map(|group| (Some(group.header), group.rows))
            .unzip();
        let output = crate::scroll::vertical()
            .id_salt(self.id.with("grouped-body"))
            .max_height(body_height)
            .auto_shrink([false, true])
            .show_viewport(ui, |ui, viewport| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.set_height(content_height);
                let origin = ui.min_rect().min;
                for (top, entry) in &list {
                    let height = entry.height();
                    if top + height < viewport.top() || *top > viewport.bottom() {
                        continue;
                    }
                    let rect =
                        Rect::from_min_size(pos2(origin.x, origin.y + top), vec2(width, height));
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
                            if let Some(selection) = selection.as_mut() {
                                selection.group = *group;
                            }
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
                                &mut render_cell,
                                &mut |_, _, _| {},
                            );
                        }
                    }
                }
            });
        if content_height > 0.0 {
            let viewport = output.inner_rect.height();
            response.visible = (
                output.state.offset.y / content_height,
                (viewport / content_height).min(1.0),
            );
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
