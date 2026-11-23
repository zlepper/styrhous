//! The public ways to show a [`Table`]: plain, selectable, sortable and
//! configurable (resizable columns with header callbacks). All of them
//! render through [`super::body::show`].

use std::collections::HashSet;
use std::hash::Hash;

use egui::{Response, Ui};

use super::body::{self, HeaderOptions, Selection};
use super::{SortState, Table};

/// The key type of a table without selection.
type NoKey = ();

impl Table {
    /// Show the table with the given items
    ///
    /// The `render_cell` closure is called for each cell with (ui, item, column_index).
    pub fn show<'a, T>(
        self,
        ui: &mut Ui,
        items: &'a [T],
        mut render_cell: impl FnMut(&mut Ui, &'a T, usize),
    ) {
        self.show_with_row_response(ui, items, &mut render_cell, |_, _, _| {});
    }

    /// Show the table and receive an interactive response for every cell.
    ///
    /// Callers receive every cell response with its column index, so they can
    /// attach a context menu across the row or keep a primary action scoped to
    /// a specific cell while preserving egui's per-widget interaction model.
    pub fn show_with_row_response<'a, T>(
        self,
        ui: &mut Ui,
        items: &'a [T],
        mut render_cell: impl FnMut(&mut Ui, &'a T, usize),
        mut render_row: impl FnMut(&Response, &'a T, usize),
    ) {
        body::show::<T, NoKey>(
            self,
            ui,
            items,
            None,
            HeaderOptions::default(),
            &mut render_cell,
            &mut render_row,
        );
    }

    /// Show the table with selection support
    ///
    /// The `selection` set contains the keys of selected items.
    /// The `key_fn` extracts a unique key from each item.
    /// The `render_cell` closure renders each cell.
    pub fn show_selectable<'a, T, K>(
        self,
        ui: &mut Ui,
        items: &'a [T],
        selection: &mut HashSet<K>,
        key_fn: impl Fn(&T) -> K,
        render_cell: impl FnMut(&mut Ui, &'a T, usize),
    ) where
        K: Eq + Hash + Clone,
    {
        self.show_selectable_with_row_response(
            ui,
            items,
            selection,
            |item| Some(key_fn(item)),
            render_cell,
            |_, _, _| {},
        );
    }

    /// Show a selectable table and receive an interactive response for every
    /// data cell.
    ///
    /// This is the selectable counterpart to [`Self::show_with_row_response`].
    /// Checkbox interactions update `selection` directly (Shift-click selects
    /// a range); returning `None` from `key_fn` renders an unselectable row
    /// while preserving the regular row callback for callers that need
    /// contextual row interactions.
    pub fn show_selectable_with_row_response<'a, T, K>(
        self,
        ui: &mut Ui,
        items: &'a [T],
        selection: &mut HashSet<K>,
        key_fn: impl Fn(&T) -> Option<K>,
        mut render_cell: impl FnMut(&mut Ui, &'a T, usize),
        mut render_row: impl FnMut(&Response, &'a T, usize),
    ) where
        K: Eq + Hash + Clone,
    {
        debug_assert!(
            self.is_selectable,
            "selectable tables must call .selectable()"
        );
        body::show(
            self,
            ui,
            items,
            Some(Selection {
                set: selection,
                key: &key_fn,
                group: 0,
            }),
            HeaderOptions::default(),
            &mut render_cell,
            &mut render_row,
        );
    }

    /// Show the table with sortable columns
    ///
    /// Clicking a sortable header sorts by it ascending, then descending,
    /// then returns to the default order ([`SortState::after_click`]).
    ///
    /// Returns `true` if the sort state changed (caller should re-sort their data).
    pub fn show_sortable<'a, T>(
        self,
        ui: &mut Ui,
        items: &'a [T],
        sort_state: &mut Option<SortState>,
        mut render_cell: impl FnMut(&mut Ui, &'a T, usize),
    ) -> bool {
        let clicked = body::show::<T, NoKey>(
            self,
            ui,
            items,
            None,
            HeaderOptions {
                sort: sort_state.as_ref(),
                click_to_sort: true,
                ..HeaderOptions::default()
            },
            &mut render_cell,
            &mut |_, _, _| {},
        );
        match clicked {
            Some(column_id) => {
                *sort_state = SortState::after_click(sort_state.as_ref(), &column_id);
                true
            }
            None => false,
        }
    }

    /// Show a table with resizable columns (persisted by `resized`) and a
    /// callback for every header cell, under a horizontal scroll area for
    /// columns that no longer fit.
    ///
    /// Returns the id of a sortable column whose header was clicked: sort by
    /// it with [`SortState::after_click`].
    #[allow(clippy::too_many_arguments)]
    pub fn show_configurable_with_row_response<'a, T>(
        self,
        ui: &mut Ui,
        items: &'a [T],
        sort_state: Option<&SortState>,
        mut render_header: impl FnMut(&Response, &str, &str, bool),
        mut resized: impl FnMut(&str, f32),
        mut render_cell: impl FnMut(&mut Ui, &'a T, usize),
        mut render_row: impl FnMut(&Response, &'a T, usize),
    ) -> Option<String> {
        body::show::<T, NoKey>(
            self,
            ui,
            items,
            None,
            HeaderOptions {
                sort: sort_state,
                click_to_sort: true,
                resize: Some(&mut resized),
                on_header: Some(&mut render_header),
                horizontal_scroll: true,
            },
            &mut render_cell,
            &mut render_row,
        )
    }

    /// The selectable counterpart of
    /// [`Self::show_configurable_with_row_response`]; returns the id of a
    /// sortable column whose header was clicked.
    #[allow(clippy::too_many_arguments)]
    pub fn show_selectable_configurable_with_row_response<'a, T, K>(
        self,
        ui: &mut Ui,
        items: &'a [T],
        selection: &mut HashSet<K>,
        key_fn: impl for<'item> Fn(&'item T) -> Option<&'item K>,
        sort_state: Option<&SortState>,
        mut render_header: impl FnMut(&Response, &str, &str, bool),
        mut resized: impl FnMut(&str, f32),
        mut render_cell: impl FnMut(&mut Ui, &'a T, usize),
        mut render_row: impl FnMut(&Response, &'a T, usize),
    ) -> Option<String>
    where
        K: Eq + Hash + Clone,
    {
        debug_assert!(
            self.is_selectable,
            "selectable tables must call .selectable()"
        );
        let key = |item: &T| key_fn(item).cloned();
        body::show(
            self,
            ui,
            items,
            Some(Selection {
                set: selection,
                key: &key,
                group: 0,
            }),
            HeaderOptions {
                sort: sort_state,
                click_to_sort: true,
                resize: Some(&mut resized),
                on_header: Some(&mut render_header),
                horizontal_scroll: true,
            },
            &mut render_cell,
            &mut render_row,
        )
    }
}
