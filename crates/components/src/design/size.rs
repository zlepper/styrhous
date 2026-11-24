//! Fixed component and layout dimensions.

/// Mini control height.
pub const CONTROL_MINI: f32 = 24.0;
/// Small control height.
pub const CONTROL_SM: f32 = 28.0;
/// Medium (default) control height.
pub const CONTROL_MD: f32 = 32.0;
/// Large control height.
pub const CONTROL_LG: f32 = 36.0;

/// Padding of stock egui buttons, set as egui's `button_padding` by the
/// theme: the small button (C02 `.btn.sm`, `padding: 0 11px`), 28 points
/// tall around a 16 point UI text row.
pub const BUTTON_PADDING_SM: egui::Vec2 = egui::vec2(11.0, (CONTROL_SM - 16.0) / 2.0);

/// Horizontal padding of a medium field (C04 `.inp`, C05 `.sel` left side).
pub const FIELD_PADDING_X: f32 = 14.0;
/// Horizontal padding of a small field (`.inp.sm`).
pub const FIELD_PADDING_X_SM: f32 = 12.0;
/// Width of a focused or invalid field's inset ring (`inset 0 0 0 1.5px`),
/// a box-shadow and so never snapped to device pixels.
pub const FIELD_RING: f32 = CONTROL_STROKE;
/// Width of the soft halo outside a focused or invalid field (`0 0 0 3px`).
pub const FIELD_HALO: f32 = 3.0;
/// Height of the divider between groups inside a field (`.inp .ivr`).
pub const INLINE_DIVIDER: f32 = 16.0;
/// Vertical padding of a text area (`.ta`).
pub const TEXT_AREA_PADDING_Y: f32 = 10.0;
/// Width of a number field's text part (C04 Replicas).
pub const NUMBER_FIELD_INPUT: f32 = 72.0;
/// The text caret the design draws in a focused field (`.inp .caret`).
pub const CARET: egui::Vec2 = egui::vec2(2.0, 16.0);

/// Horizontal inset of a small button's content (C02 `.btn.sm`: 11 points of
/// padding inside a 1 point transparent border).
pub const BUTTON_INSET_SM: f32 = 12.0;
/// Horizontal inset of a medium button's content (`.btn`: 14 + 1).
pub const BUTTON_INSET_MD: f32 = 15.0;
/// Horizontal inset of a large button's content (`.btn.lg`: 16 + 1).
pub const BUTTON_INSET_LG: f32 = 17.0;
/// Gap between a small button's icon and its label.
pub const BUTTON_GAP_SM: f32 = 4.0;
/// Gap between a medium or large button's icon and its label.
pub const BUTTON_GAP: f32 = 6.0;
/// The spinner that leads a loading button's label (`.btn .spin`).
pub const BUTTON_SPINNER: f32 = 14.0;
/// Width of the surface-coloured gap between a control and its focus ring
/// (`0 0 0 2px surface-canvas`).
pub const FOCUS_RING_GAP: f32 = 2.0;
/// Width of a focus ring (`0 0 0 4px focus-ring` outside the gap).
pub const FOCUS_RING: f32 = 2.0;
/// How far a focus ring reaches outside its control (gap plus ring).
pub const FOCUS_RING_EXTENT: f32 = FOCUS_RING_GAP + FOCUS_RING;
/// Thickness of a hovered link button's underline.
pub const LINK_UNDERLINE: f32 = 2.0;
/// Distance from a link button's baseline to its underline.
pub const LINK_UNDERLINE_OFFSET: f32 = 3.0;

/// Padding of a pill tray (C03 `.grp`, `.seg`) around its children.
pub const TRAY_PADDING: f32 = 2.0;
/// Gap between the children of a pill tray.
pub const TRAY_GAP: f32 = 2.0;
/// Height of the separator between sub-groups of a button group (`.grp .sep`).
pub const TRAY_SEPARATOR: f32 = 16.0;
/// Horizontal margin on each side of a button group separator.
pub const TRAY_SEPARATOR_MARGIN: f32 = 2.0;
/// Horizontal padding of a toggle with a label inside a group (`.tgl.txt`).
pub const TOGGLE_PADDING_X: f32 = 10.0;
/// Horizontal padding of a standalone toggle (`.tgl-solo`).
pub const TOGGLE_SOLO_PADDING_X: f32 = 12.0;
/// Horizontal padding of a medium segment (`.seg-i`).
pub const SEGMENT_PADDING_X: f32 = 12.0;
/// Horizontal padding of a small segment (`.seg.sm .seg-i`).
pub const SEGMENT_PADDING_X_SM: f32 = 10.0;

/// Right padding of a select trigger, before its chevron (C05 `.sel`).
pub const SELECT_PADDING_RIGHT: f32 = 10.0;
/// Horizontal padding of the `+N` chip after a select's values (`.sel .more`).
pub const MORE_CHIP_PADDING_X: f32 = 7.0;
/// Gap between a select trigger and its popover.
pub const POPOVER_GAP: f32 = 4.0;
/// An option row of a select or menu (`.opt`).
pub const OPTION: f32 = 32.0;
/// An option row with a secondary line (`.opt.two`).
pub const OPTION_TWO_LINE: f32 = 44.0;
/// Horizontal padding of an option row.
pub const OPTION_PADDING_X: f32 = 10.0;
/// A divider between groups of options, with its margins (`.msep`: 4 + 1 + 4).
pub const OPTION_SEPARATOR: f32 = 9.0;
/// The search row on top of a popover (`.pop-search`).
pub const POPOVER_SEARCH: f32 = 36.0;
/// Horizontal padding of a popover's search row.
pub const POPOVER_SEARCH_PADDING_X: f32 = 12.0;
/// The key-hint footer of a popover (`.pop-foot`).
pub const POPOVER_FOOTER: f32 = 32.0;

// C07
/// Narrowest a menu grows (S07: as wide as its content, 200 to 320 points).
pub const MENU_MIN_WIDTH: f32 = 200.0;
/// Widest a menu grows before its labels elide.
pub const MENU_MAX_WIDTH: f32 = 320.0;
/// The narrowest a resource table's row menu is (A04).
pub const MENU_ROW_WIDTH: f32 = 220.0;
/// The gap between a row's "…" button and its menu (A04).
pub const MENU_ROW_GAP: f32 = 12.0;
/// The section heading of a menu (`.mlabel`): 8 points above and 4 below
/// a 16 point caption line.
pub const MENU_HEADING: f32 = 28.0;
/// Padding of a tooltip (`.tip`).
pub const TOOLTIP_PADDING: egui::Vec2 = egui::vec2(10.0, 6.0);
/// Widest a tooltip grows before it wraps, padding included.
pub const TOOLTIP_MAX_WIDTH: f32 = 280.0;
/// Gap between a tooltip and the control it explains (S07).
pub const TOOLTIP_GAP: f32 = 6.0;

/// Table header height.
pub const TABLE_HEADER: f32 = 36.0;
/// Height of a table row tile.
pub const ROW_TILE: f32 = 36.0;
/// Gap between table row tiles.
pub const ROW_GAP: f32 = 4.0;
/// Vertical distance between consecutive table rows (tile plus gap).
pub const ROW_PITCH: f32 = ROW_TILE + ROW_GAP;
/// Width of a table's selection checkbox column.
pub const TABLE_CHECKBOX_COLUMN: f32 = 40.0;
/// Width of a table's row actions column: a ghost sm "⋯" button, centred (S08).
pub const TABLE_ACTIONS_COLUMN: f32 = 48.0;
/// The narrowest a table column can be resized to (S08).
pub const TABLE_MIN_COLUMN: f32 = 48.0;

/// Page and window header bars.
pub const HEADER_BAR: f32 = 56.0;
/// Toolbars.
pub const TOOLBAR: f32 = 48.0;
/// Status bars.
pub const STATUS_BAR: f32 = 28.0;

/// Width of the workspace navigation rail.
pub const RAIL_WIDTH: f32 = 64.0;
/// Rail buttons and avatars (`.rbtn`, `.av`).
pub const RAIL_BUTTON: f32 = 40.0;
/// Width of the workspace navigator panel.
pub const NAVIGATOR_WIDTH: f32 = 240.0;

/// Hairline strokes: dividers and borders.
pub const HAIRLINE: f32 = 1.0;
/// Control strokes: checkbox and radio borders, focus and selection rings.
pub const CONTROL_STROKE: f32 = 1.5;
/// Checkbox and radio size.
pub const CHECKBOX: f32 = 16.0;
/// Dot of a selected radio.
pub const RADIO_DOT: f32 = 6.0;
/// Switch track (C06 `.sw`).
pub const SWITCH: egui::Vec2 = egui::vec2(32.0, 18.0);
/// The rounded-square knob of a switch.
pub const SWITCH_KNOB: f32 = 12.0;
/// Inset of a switch's knob from its track's edges.
pub const SWITCH_KNOB_INSET: f32 = 3.0;
/// Gap between a switch and its label (`label.row` gap 10).
pub const SWITCH_LABEL_GAP: f32 = 10.0;
/// Horizontal padding of a radio card (C06 `.rcard`).
pub const RADIO_CARD_PADDING_X: f32 = 16.0;
/// Vertical padding of a radio card.
pub const RADIO_CARD_PADDING_Y: f32 = 14.0;
/// How far a radio card's radio sits below the top of its title's line
/// box (`.rcard .rd { margin-top: 2px }`).
pub const RADIO_CARD_RADIO_OFFSET: f32 = 2.0;
/// Keycap height (`.kbd`).
pub const KEY_HEIGHT: f32 = 18.0;
/// Horizontal keycap padding.
pub const KEY_PADDING_X: f32 = 5.0;
/// Icon tile (`.tile`).
pub const TILE: f32 = 36.0;
/// Extra-small icon size (`.ic.s12`): chevrons in headers.
pub const ICON_XS: f32 = 12.0;
/// Small icon size (`.ic.s14`): icons in 24 point mini buttons.
pub const ICON_SM: f32 = 14.0;
/// Default icon size, in a 20 point line box next to 13 point text.
pub const ICON: f32 = 16.0;
/// Large icon size (`.ic.s20`): toasts, rail buttons and tiles.
pub const ICON_LG: f32 = 20.0;
/// Extra-large icon size (`.ic.s24`).
pub const ICON_XL: f32 = 24.0;

/// Status dot (`.dot`), a small rounded square lamp.
pub const STATUS_DOT: f32 = 8.0;
/// Gap between a status dot and its words (`.st`, `.pill`).
pub const STATUS_DOT_GAP: f32 = 6.0;
/// Pills, kind badges and chips (`.pill`, `.kind`, `.chip`).
pub const PILL_HEIGHT: f32 = 22.0;
/// Horizontal padding of pills, kind badges and chips.
pub const PILL_PADDING_X: f32 = 9.0;
/// Minimum width of a pill whose words change over time (Following,
/// Reconnecting…), so nothing to its right jumps.
pub const LIVE_PILL_MIN_WIDTH: f32 = 96.0;
/// Count badge (`.count`): height and minimum width.
pub const COUNT_BADGE: f32 = 16.0;
/// Horizontal padding of a count badge.
pub const COUNT_BADGE_PADDING_X: f32 = 5.0;
/// The surface-coloured ring that cuts a count badge out of the control it
/// sits on.
pub const COUNT_BADGE_RING: f32 = 2.0;
/// Container state square (`.cs`).
pub const CONTAINER_SQUARE: f32 = 8.0;
/// Gap between container state squares.
pub const CONTAINER_SQUARE_GAP: f32 = 3.0;
// A04 · usage cells
/// The bar of a table's used / allocatable cell: its width.
pub const USAGE_BAR_WIDTH: f32 = 48.0;
/// The bar of a table's used / allocatable cell: its height.
pub const USAGE_BAR_HEIGHT: f32 = 4.0;
/// Gap between a usage cell's figures and its bar.
pub const USAGE_BAR_GAP: f32 = 8.0;
/// Small spinner diameter, beside 12 point status text.
pub const SPINNER_SM: f32 = 12.0;
/// Spinner (`.spin`) diameter.
pub const SPINNER: f32 = 16.0;
/// Large spinner diameter, in empty and loading states.
pub const SPINNER_LG: f32 = 20.0;
/// Spinner ring width.
pub const SPINNER_STROKE: f32 = 2.0;
/// Skeleton bar (`.skel`) height.
pub const SKELETON_BAR: f32 = 10.0;
/// Cluster avatar (`.av`), as large as a rail button.
pub const AVATAR: f32 = RAIL_BUTTON;
/// The status square on an avatar (`.av .sd`), including its border.
pub const AVATAR_STATUS: f32 = 11.0;
/// The surface-coloured border that cuts the status square out of the avatar.
pub const AVATAR_STATUS_BORDER: f32 = 2.0;
/// How far the status square hangs past the avatar's bottom-right corner.
pub const AVATAR_STATUS_OVERHANG: f32 = 3.0;
/// Gap between a selected avatar and its ring.
pub const AVATAR_RING_GAP: f32 = FOCUS_RING_GAP;
/// Width of the selected avatar ring.
pub const AVATAR_RING: f32 = FOCUS_RING;

// A09
/// A colour swatch's side (`Swatches`).
pub const SWATCH: f32 = 24.0;

// C11
/// Width of a dialog (`.dlg`).
pub const DIALOG_WIDTH: f32 = 440.0;
/// Width of a dialog that holds logs or a table.
pub const DIALOG_WIDTH_WIDE: f32 = 560.0;

// C13 · data display
/// The header row of a card or disclosure (`.card-h`).
pub const CARD_HEADER: f32 = 44.0;
/// Widest a chip grows before it truncates with a tooltip.
pub const CHIP_MAX_WIDTH: f32 = 320.0;
/// The caption before a row of chips ([`crate::labelled_chips`]).
pub const CHIP_ROW_LABEL: f32 = 72.0;
/// How far a hovered copyable value's pill reaches past the value's left
/// and right edges (`margin-left: -4px; padding: 0 4px`).
pub const COPYABLE_INSET: f32 = 4.0;
/// Gap between a copyable value and its copy icon.
pub const COPYABLE_ICON_GAP: f32 = 6.0;
/// A property grid uses three columns from this width (`.pg`).
pub const PROPERTY_GRID_THREE_COLUMNS: f32 = 600.0;
/// A property grid uses two columns from this width, one below it.
pub const PROPERTY_GRID_TWO_COLUMNS: f32 = 400.0;
/// The plot of a usage chart.
pub const CHART_PLOT: f32 = 72.0;
/// Height of the plot of a compact usage chart (A05).
pub const CHART_PLOT_COMPACT: f32 = 56.0;
/// Inset of a usage chart's limit line from the top of its plot.
pub const CHART_LIMIT_INSET: f32 = 8.0;
/// Inset of a usage chart's baseline from the bottom of its plot.
pub const CHART_BASELINE_INSET: f32 = 8.0;
/// Width of a usage chart's series line.
pub const CHART_LINE: f32 = 1.5;
/// Dash and gap length of a usage chart's limit and request lines.
pub const CHART_DASH: f32 = 3.0;
/// The plot width at which a dash is [`CHART_DASH`] long; dashes stretch
/// with the plot (the design draws charts in a 300 unit wide `viewBox`).
pub const CHART_DASH_SCALE_WIDTH: f32 = 300.0;
/// A navigation list row (`.lrow`).
pub const LIST_ROW: f32 = 64.0;
/// A small icon tile (`.tile.sm`), in window headers.
pub const TILE_SM: f32 = 28.0;
/// A large icon tile (`.tile.lg`).
pub const TILE_LG: f32 = 40.0;
/// The row of a tab strip (`.tabs`).
pub const TABS: f32 = 40.0;
/// A tab pill (`.tab`).
pub const TAB: f32 = 30.0;
/// Horizontal padding of a tab pill.
pub const TAB_PADDING_X: f32 = 12.0;

// C14 · document window chrome
/// A vertical divider between toolbar groups (`.vr`).
pub const TOOLBAR_DIVIDER: f32 = 20.0;
/// The narrowest the leading side of a window header, toolbar or status bar
/// gets (a tile, an elided title and a pill), however wide its trailing
/// side grows.
pub const BAR_MIN_LEADING: f32 = 240.0;

// C15 · code surface
/// The line-number gutter of a code surface, its padding included.
pub const CODE_GUTTER: f32 = 56.0;
/// Space between a line number and the code (`.ln .g` right padding).
pub const CODE_GUTTER_PADDING: f32 = 16.0;
/// Where the editor's gutter rule sits, from the surface's left edge.
pub const CODE_GUTTER_RULE: f32 = 48.0;
/// Space above the first line of a code surface.
pub const CODE_PADDING_TOP: f32 = 12.0;
/// The text caret of a code surface (`.cr`): 2 points wide, 18 tall.
pub const CODE_CARET: egui::Vec2 = egui::vec2(2.0, 18.0);
/// Gap between the caret's line box top and the caret.
pub const CODE_CARET_INSET: f32 = 1.0;
/// Width of a code surface's source-label column padding (the space after
/// the longest label).
pub const CODE_SOURCE_PADDING: f32 = 16.0;
/// Half the height of a diagnostic's wavy underline.
pub const SQUIGGLE_AMPLITUDE: f32 = 1.0;
/// Length of one wave of a diagnostic's underline.
pub const SQUIGGLE_WAVELENGTH: f32 = 7.0;
/// Distance from the text baseline to the middle of a squiggle (CSS
/// `text-underline-offset: 4px` plus Chromium's wave placement).
pub const SQUIGGLE_OFFSET: f32 = 6.5;
/// The completion popover (`.pop` on the code surface).
pub const COMPLETION_WIDTH: f32 = 264.0;
/// The completion documentation popover beside it.
pub const COMPLETION_DOC_WIDTH: f32 = 216.0;
/// The documentation popover beside the YAML editor's completion (Y02).
pub const COMPLETION_DOC_WIDTH_EDITOR: f32 = 260.0;
/// Gap between the completion popover and its documentation popover.
pub const COMPLETION_DOC_GAP: f32 = 8.0;
/// A completion option row.
pub const COMPLETION_ROW: f32 = 28.0;
/// The resource-kind badge of a completion's documentation.
pub const COMPLETION_KIND_BADGE: f32 = 20.0;
/// The header of the Problems panel.
pub const PROBLEMS_HEADER: f32 = 32.0;
/// A row of the Problems panel.
pub const PROBLEMS_ROW: f32 = 28.0;
/// Tallest the Problems panel's rows grow before they scroll.
pub const PROBLEMS_MAX_HEIGHT: f32 = 160.0;
/// The "Ln n" column of a Problems row.
pub const PROBLEMS_LINE_COLUMN: f32 = 44.0;
/// The YAML editor's Problems panel is roomier (Y02): its horizontal padding.
pub const PROBLEMS_PADDING_EDITOR: f32 = 16.0;
/// The width of the "Ln n" column of the YAML editor's Problems panel.
pub const PROBLEMS_LINE_COLUMN_EDITOR: f32 = 48.0;

/// The width Chromium paints a CSS `border` of `width` points at
/// `pixels_per_point`: border widths are snapped down to whole device pixels
/// (at least one), so the design's 1.5 px control borders paint 1 px wide on
/// a 1× display and 3 px on a 2× display. Box-shadow rings (`inset 0 0 0
/// 1.5px`) are not snapped; draw those at their design width.
pub fn css_border_width(width: f32, pixels_per_point: f32) -> f32 {
    (width * pixels_per_point).floor().max(1.0) / pixels_per_point
}

// C12
/// Height of a banner (`.banner`), its bottom border included.
pub const BANNER: f32 = 36.0;

// C10
/// Preferred width of a blade (`.blade`).
pub const BLADE: f32 = 720.0;
/// Narrowest a blade gets before it gives up its share of the window.
pub const BLADE_MIN: f32 = 560.0;
/// Blade header (`.blade-h`), its bottom divider included.
pub const BLADE_HEADER: f32 = 56.0;
/// Blade footer (`.blade-f`), its top divider included.
pub const BLADE_FOOTER: f32 = 64.0;
/// How much of a history blade peeks out from behind the next one.
pub const BLADE_PEEK: f32 = 96.0;
/// How far each history peek steps down (and up from the bottom).
pub const BLADE_PEEK_STEP: f32 = 16.0;
/// The compact header of a history peek.
pub const BLADE_PEEK_HEADER: f32 = 72.0;
/// Widest a history peek's name runs before it is truncated.
pub const BLADE_PEEK_NAME: f32 = 72.0;
/// The vertical rule between a header's navigation and its title (`.vr`).
pub const VERTICAL_RULE: f32 = 20.0;

// C09
/// Width of the floating workspace sidebar (`.side`).
pub const SIDEBAR: f32 = 268.0;
/// Padding of the sidebar.
pub const SIDEBAR_PADDING: f32 = 14.0;
/// A kind, footer or pinned item of the sidebar (`.sv`).
pub const SIDE_ITEM: f32 = 34.0;
/// A group or child item of the sidebar (`.sv.sgrp`, `.sv.sub`).
pub const SIDE_ITEM_SM: f32 = 30.0;
/// Horizontal padding of a sidebar item.
pub const SIDE_ITEM_PADDING_X: f32 = 8.0;
/// Left padding of a child item, under its group's label.
pub const SIDE_CHILD_INDENT: f32 = 30.0;
/// The kind colour square of a sidebar item (`.kc`).
pub const KIND_SWATCH: f32 = 10.0;
/// Height of the sidebar's kind filter.
pub const SIDEBAR_FILTER: f32 = 34.0;
/// Padding of the cluster card (`.ccard`).
pub const CLUSTER_CARD_PADDING: f32 = 10.0;
/// Height of the fade over the sidebar's last items above its footer.
pub const SIDEBAR_FADE: f32 = 48.0;

// A02 · workspace shell
/// How far the floating sidebar stays off the window's edges (`.side`).
pub const SIDEBAR_INSET: f32 = 12.0;
/// Gap between the floating sidebar and the workspace (`.main` left 304).
pub const WORKSPACE_GAP: f32 = 24.0;
/// The band left of the workspace: the sidebar with its inset and gap.
pub const WORKSPACE_LEFT: f32 = SIDEBAR_INSET + SIDEBAR + WORKSPACE_GAP;
/// The gutter after the workspace that holds a list's scroll track
/// (`.main` ends 28 points short of the window).
pub const WORKSPACE_END_GUTTER: f32 = 28.0;

// C16 · top bar
/// The band above a resource list holding its top bar (`.topbar`).
pub const TOP_BAR: f32 = 78.0;
/// The search and scope pills of a top bar (`.inp.lg`, `.sel.lg`).
pub const TOP_BAR_CONTROL: f32 = 38.0;
/// Left padding of a large select (`.sel.lg`).
pub const SELECT_PADDING_LEFT_LG: f32 = 16.0;
/// Right padding of a large select, after its chevron.
pub const SELECT_PADDING_RIGHT_LG: f32 = 12.0;
/// The search pill of a top bar.
pub const SEARCH_PILL: f32 = 300.0;
/// The search pill of a narrow window's top bar.
pub const SEARCH_PILL_NARROW: f32 = 220.0;
/// The scope pill of a top bar.
pub const SCOPE_PILL: f32 = 240.0;
/// The scope pill of a narrow window's top bar.
pub const SCOPE_PILL_NARROW: f32 = 180.0;
/// Below this window width the top bar is narrow (S16): the grouping chip
/// and the live meta hide.
pub const TOP_BAR_NARROW_WINDOW: f32 = 1100.0;

// C17 · grouped table
/// A group header row of a grouped table (`.gh`).
pub const GROUP_HEADER: f32 = 34.0;
/// Horizontal padding of a group header.
pub const GROUP_HEADER_PADDING_X: f32 = 6.0;
/// The chip holding a group's chevron and name (`.gchip`).
pub const GROUP_CHIP: f32 = 26.0;
/// Left padding of a group chip, before its chevron.
pub const GROUP_CHIP_PADDING_LEFT: f32 = 8.0;
/// Right padding of a group chip, after its name.
pub const GROUP_CHIP_PADDING_RIGHT: f32 = 10.0;
/// Width of the problem ticks track (`.sticks`).
pub const PROBLEM_TRACK: f32 = 6.0;
/// Height of a problem tick (`.tk`).
pub const PROBLEM_TICK: f32 = 3.0;
// A02 / A03 · resource tables
/// The Namespace column of a resource table.
pub const TABLE_NAMESPACE_COLUMN: f32 = 132.0;
/// The Owner column of a resource table.
pub const TABLE_OWNER_COLUMN: f32 = 160.0;
/// The Ready column of a resource table in a narrow window.
pub const TABLE_READY_COLUMN_COMPACT: f32 = 64.0;
/// The popover of the namespace scope pill, which its search row and key
/// hints need more room for than the pill.
pub const NAMESPACE_POPOVER: f32 = 300.0;
/// The gap between a list's rows and its problem track (A02 `.main` to `.sticks`).
pub const PROBLEM_TRACK_GAP: f32 = 6.0;
/// How far below its list's header band a problem track starts.
pub const PROBLEM_TRACK_TOP_INSET: f32 = 14.0;
/// How far above the window's bottom edge a problem track ends.
pub const PROBLEM_TRACK_BOTTOM_INSET: f32 = 12.0;
/// How far a problem tick reaches past each side of its track.
pub const PROBLEM_TICK_OVERHANG: f32 = 1.0;

// C19 · readouts
/// Padding of a readout tile (`.ro`: 9 points top and bottom, 12 at the sides).
pub const READOUT_PADDING: egui::Vec2 = egui::vec2(12.0, 9.0);
/// Width of an inspector's section rail (`.srail`).
pub const SECTION_RAIL: f32 = 176.0;
/// Padding of a section rail (14 points top and bottom, 10 at the sides).
pub const SECTION_RAIL_PADDING: egui::Vec2 = egui::vec2(10.0, 14.0);
/// An item of a section rail (`.si`).
pub const SECTION_ITEM: f32 = 32.0;
/// Horizontal padding of a section rail item.
pub const SECTION_ITEM_PADDING_X: f32 = 10.0;
/// Padding of an inspector header (`padding: 14px 16px 16px`).
pub const INSPECTOR_HEADER_PADDING: egui::Margin = egui::Margin {
    left: 16,
    right: 16,
    top: 14,
    bottom: 16,
};
/// The title and status line row of an inspector header.
pub const INSPECTOR_HEADER_ROW: f32 = 40.0;

// C20 · log viewer
/// A log line (`.ll`).
pub const LOG_ROW: f32 = 22.0;
/// Horizontal padding of a log line.
pub const LOG_ROW_PADDING_X: f32 = 18.0;
/// The line-number column of a log line (`.ll .lno`).
pub const LOG_LINE_NUMBER: f32 = 40.0;
/// The source and level pills of a log line (`.ll .sp`, `.lv`).
pub const LOG_PILL: f32 = 18.0;
/// The narrowest source pill of a log line.
pub const LOG_SOURCE_MIN_WIDTH: f32 = 68.0;
/// Horizontal padding of a log line's source pill.
pub const LOG_SOURCE_PADDING_X: f32 = 8.0;
/// A log level badge (`.lv`).
pub const LEVEL_BADGE: f32 = 52.0;
/// A source pill of the volume strip (`.srcp`).
pub const SOURCE_PILL: f32 = 24.0;
/// Horizontal padding of a source pill.
pub const SOURCE_PILL_PADDING_X: f32 = 10.0;
/// The ring around a reconnecting source's lamp (`.dot.ring`, `0 0 0 3px`).
pub const LAMP_RING: f32 = FIELD_HALO;
/// The bars of a volume strip (`.vbars`).
pub const VOLUME_BARS: f32 = 36.0;
/// One volume bar.
pub const VOLUME_BAR: f32 = 5.0;
/// Gap between volume bars.
pub const VOLUME_BAR_GAP: f32 = 3.0;
/// How far the visible-window outline reaches above and below the bars.
pub const VOLUME_WINDOW_OUTSET: f32 = 4.0;
/// Padding of the volume strip (`.vstrip`: 14 top, 18 sides, 12 bottom).
pub const VOLUME_STRIP_PADDING: egui::Margin = egui::Margin {
    left: 18,
    right: 18,
    top: 14,
    bottom: 12,
};
/// Padding of a floating toolbar around its controls (`.ftool`).
pub const FLOATING_TOOLBAR_PADDING: f32 = 6.0;
/// How far a floating toolbar floats above the bottom of its window.
pub const FLOATING_TOOLBAR_INSET: f32 = 22.0;
/// The space a log panel reserves under its last line for the toolbar.
pub const FLOATING_TOOLBAR_RESERVE: f32 = 96.0;

// C18 · cluster switcher
/// A cluster tile (`.ctile`).
pub const CLUSTER_TILE: f32 = 60.0;
/// Horizontal padding of a cluster tile.
pub const CLUSTER_TILE_PADDING_X: f32 = 12.0;
/// Padding of the cluster switcher (`.palette`).
pub const CLUSTER_SWITCHER_PADDING: f32 = 24.0;
/// The widest the cluster switcher grows (S18).
pub const CLUSTER_SWITCHER_MAX_WIDTH: f32 = 1176.0;
/// Padding of the switcher's readout strip (16 at the sides, 12 above and below).
pub const CLUSTER_STRIP_PADDING: egui::Vec2 = egui::vec2(16.0, 12.0);
/// The name column of the switcher's readout strip.
pub const CLUSTER_STRIP_NAME: f32 = 180.0;
/// The latency history of the switcher's readout strip.
pub const CLUSTER_LATENCY: egui::Vec2 = egui::vec2(140.0, 28.0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borders_snap_to_whole_device_pixels() {
        assert_eq!(css_border_width(CONTROL_STROKE, 1.0), 1.0);
        assert_eq!(css_border_width(CONTROL_STROKE, 2.0), 1.5);
        assert_eq!(css_border_width(HAIRLINE, 1.0), 1.0);
        assert_eq!(css_border_width(0.5, 1.0), 1.0);
        assert_eq!(css_border_width(CONTROL_STROKE, 1.5), 4.0 / 3.0);
    }
}
