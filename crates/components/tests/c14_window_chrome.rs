//! The C14 "Document window chrome" component board
//! (`docs/design-oracles/harbour/src/C14.html`) rebuilt from the real
//! components, compared with its oracle by `scripts/design-oracles/compare.sh C14`.

use components::design::theme::{ThemeExt, Tone};
use components::design::{radius, size, stroke, type_scale};
use components::icons::Icon;
use components::test_support::board::{BoardHeader, board, column, row, section, snapshot_board};
use components::{
    Button, ButtonGroup, SearchField, SearchStatus, Segmented, SegmentedSize, StatusBar,
    ToggleButton, Toolbar, WindowHeader, problems_toggle, status_icon, status_number, status_text,
    toolbar_activity, toolbar_divider,
};
use egui::{Align, CornerRadius, Layout, Rect, Sense, StrokeKind, Ui, UiBuilder, Vec2, vec2};

const HEADER: BoardHeader<'static> = BoardHeader {
    eyebrow: "C14 · components::{WindowHeader, Toolbar, StatusBar}",
    title: "Document window chrome",
    description: "Log viewer and YAML editor are the same window: a title header with state, \
                  a single toolbar row, the code surface and a status bar. Primary actions live \
                  in the header; tools live in the toolbar.",
};

/// Gap between the board's rows of windows.
const SECTION_GAP: f32 = 24.0;
/// The full-width windows.
const WIDE: Vec2 = vec2(1440.0, 168.0);
/// The narrow window: a header and a toolbar inside the border.
const NARROW: Vec2 = vec2(760.0, 106.0);
const NARROW_GAP: f32 = 40.0;
/// The toolbar order note.
const NOTE_WIDTH: f32 = 600.0;
const NOTE_GAP: f32 = 8.0;
/// Search field widths.
const NARROW_SEARCH_WIDTH: f32 = 220.0;

/// The window's inner corner radius, inside its border.
fn inner_radius() -> u8 {
    radius::CARD - size::HAIRLINE as u8
}

fn top_corners() -> CornerRadius {
    CornerRadius {
        sw: 0,
        se: 0,
        ..CornerRadius::same(inner_radius())
    }
}

fn bottom_corners() -> CornerRadius {
    CornerRadius {
        nw: 0,
        ne: 0,
        ..CornerRadius::same(inner_radius())
    }
}

/// A window of `size` with a divider border and card corners, its chrome
/// stacked inside by `add_contents`.
fn window(ui: &mut Ui, size: Vec2, add_contents: impl FnOnce(&mut Ui)) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let inner = rect.shrink(size::HAIRLINE);
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::top_down(Align::Min)),
    );
    child.spacing_mut().item_spacing = Vec2::ZERO;
    child.set_clip_rect(inner);
    add_contents(&mut child);
    ui.painter().rect_stroke(
        rect,
        radius::CARD,
        stroke::divider(&ui.tokens()),
        StrokeKind::Inside,
    );
}

/// The empty code surface between the toolbar and the status bar.
fn code_surface(ui: &mut Ui, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    ui.painter().rect_filled(rect, 0, ui.tokens().code.surface);
}

fn search(ui: &mut Ui, initial: &str, width: f32, status: SearchStatus) {
    let mut query = initial.to_owned();
    let mut regex = false;
    SearchField::new(&mut query)
        .regex(&mut regex)
        .id_salt(("search", width as u32, initial))
        .placeholder("Search logs…")
        .accessibility_label("Search")
        .width(width)
        .status(status)
        .show(ui);
}

fn instance(ui: &mut Ui, size: SegmentedSize) {
    let mut instance = 0;
    Segmented::new(&mut instance)
        .option(0, "Current")
        .option(1, "Previous")
        .size(size)
        .label("Container instance")
        .show(ui);
}

fn log_window(ui: &mut Ui) {
    section(
        ui,
        "Log window — header 56 · toolbar 48 · status bar 28",
        |ui| {
            window(ui, WIDE, |ui| {
                WindowHeader::new("log", Icon::DocumentText, "api-0")
                    .meta("Pod logs · container api · kube-system")
                    .status("Following", Tone::Success)
                    .corner_radius(top_corners())
                    .show(ui, |ui| {
                        status_text_secondary(ui, "Container start");
                        instance(ui, SegmentedSize::Md);
                    });
                Toolbar::new("log").show(
                    ui,
                    |ui| {
                        search(
                            ui,
                            "http",
                            components::SEARCH_FIELD_WIDTH,
                            SearchStatus::Matches {
                                active: Some(2),
                                total: 12,
                            },
                        );
                        ToggleButton::text("Matches only", false)
                            .leading_icon(Icon::Funnel)
                            .show(ui);
                        toolbar_divider(ui);
                        ButtonGroup::new("View options")
                            .toggle(ToggleButton::icon(Icon::NumberedList, "Line numbers", true))
                            .toggle(ToggleButton::icon(Icon::Clock, "Timestamps", false))
                            .toggle(ToggleButton::icon(Icon::Swatch, "ANSI colours", true))
                            .toggle(ToggleButton::icon(Icon::Tag, "Source labels", false))
                            .show(ui);
                    },
                    |ui| {
                        ButtonGroup::new("Line navigation")
                            .toggle(ToggleButton::action(Icon::ArrowUp, "Previous line"))
                            .toggle(ToggleButton::action(Icon::ArrowDown, "Next line"))
                            .show(ui);
                        ToggleButton::text("Follow", true)
                            .leading_icon(Icon::ChevronDoubleDown)
                            .show(ui);
                    },
                );
                code_surface(ui, ui.available_height() - size::STATUS_BAR);
                StatusBar::new("log").corner_radius(bottom_corners()).show(
                    ui,
                    |ui| {
                        status_number(ui, "1,284 lines");
                        status_number(ui, "12 matches");
                        status_text(ui, "Backfill 2.1k");
                    },
                    |ui| {
                        status_text(ui, "Selection 2 lines");
                        status_text(ui, "UTC");
                    },
                );
            });
        },
    );
}

/// Muted meta words in the header ("Container start").
fn status_text_secondary(ui: &mut Ui, text: &str) {
    type_scale::META.label(ui, text, ui.tokens().text.muted);
}

fn editor_window(ui: &mut Ui) {
    section(
        ui,
        "YAML editor — same header, toolbar and status bar",
        |ui| {
            window(ui, WIDE, |ui| {
                WindowHeader::new("editor", Icon::PencilSquare, "settings")
                    .meta("ConfigMap · kube-system")
                    .status("Modified", Tone::Warning)
                    .corner_radius(top_corners())
                    .show(ui, |ui| {
                        Button::secondary("Discard").show(ui);
                        Button::primary("Apply changes").show(ui);
                    });
                Toolbar::new("editor").show(
                    ui,
                    |ui| {
                        search(
                            ui,
                            "map",
                            components::SEARCH_FIELD_WIDTH,
                            SearchStatus::Matches {
                                active: Some(0),
                                total: 2,
                            },
                        );
                    },
                    |ui| {
                        toolbar_activity(ui, "Validating…");
                    },
                );
                code_surface(ui, ui.available_height() - size::STATUS_BAR);
                StatusBar::new("editor")
                    .corner_radius(bottom_corners())
                    .show(
                        ui,
                        |ui| {
                            status_icon(ui, Icon::InformationCircle);
                            status_text(ui, "Changes apply directly to the cluster");
                        },
                        |ui| {
                            problems_toggle(ui, 1);
                            status_number(ui, "Ln 4, Col 12");
                            status_text(ui, "YAML");
                        },
                    );
            });
        },
    );
}

fn narrow_window(ui: &mut Ui) {
    section(
        ui,
        "Narrow window (< 960 px): titles truncate, view toggles collapse into a menu, labels drop",
        |ui| {
            window(ui, NARROW, |ui| {
                WindowHeader::new(
                    "narrow",
                    Icon::DocumentText,
                    "kube-controller-manager-kind-control-plane",
                )
                .meta("Pod logs · 2 sources")
                .status("Reconnecting…", Tone::Warning)
                .corner_radius(top_corners())
                .show(ui, |ui| instance(ui, SegmentedSize::Sm));
                Toolbar::new("narrow").show(
                    ui,
                    |ui| {
                        search(ui, "", NARROW_SEARCH_WIDTH, SearchStatus::Idle);
                        ToggleButton::icon(Icon::Funnel, "Matches only", false).show(ui);
                        toolbar_divider(ui);
                        Button::secondary("View options")
                            .leading_icon(Icon::AdjustmentsHorizontal)
                            .trailing_icon(Icon::ChevronDown)
                            .show(ui);
                    },
                    |ui| {
                        ToggleButton::icon(Icon::ChevronDoubleDown, "Follow", true).show(ui);
                    },
                );
            });
        },
    );
}

fn toolbar_order(ui: &mut Ui) {
    column(ui, NOTE_GAP, |ui| {
        ui.set_width(NOTE_WIDTH);
        type_scale::CAPTION.label(ui, "Toolbar order", ui.tokens().text.muted);
        type_scale::UI.label(
            ui,
            "Search → filter → view options | flexible space | navigation → mode toggles. \
             Groups never overlap: when the toolbar runs out of room, the view-option group \
             collapses first, then button labels, then the search field shrinks to 200 px.",
            ui.tokens().text.secondary,
        );
    });
}

fn c14_scene(ui: &mut Ui) {
    board(ui, HEADER, |ui| {
        column(ui, SECTION_GAP, |ui| {
            log_window(ui);
            editor_window(ui);
            row(ui, 0.0, NARROW_GAP, Align::Min, |ui| {
                narrow_window(ui);
                toolbar_order(ui);
            });
        });
    });
}

#[test]
fn c14_window_chrome_board() {
    snapshot_board("c14_window_chrome", c14_scene);
}

/// The trailing side of a bar ends at its right padding and never overlaps
/// its leading side.
#[test]
fn bars_keep_their_sides_apart() {
    use egui_kittest::kittest::Queryable;
    let mut harness = egui_kittest::Harness::new_ui_state(
        |ui, bar: &mut Rect| {
            ui.allocate_ui_with_layout(vec2(400.0, 200.0), Layout::top_down(Align::Min), |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                *bar = ui.max_rect();
                Toolbar::new("tight").show(
                    ui,
                    |ui| {
                        type_scale::UI.label(ui, "leading", ui.tokens().text.primary);
                    },
                    |ui| {
                        Button::secondary("Trailing action").show(ui);
                    },
                );
            });
        },
        Rect::NOTHING,
    );
    components::test_support::setup_egui(&mut harness);
    harness.run();
    let bar = *harness.state();
    let leading = harness.get_by_label("leading").rect();
    let trailing = harness.get_by_label("Trailing action").rect();
    assert!(leading.right() <= trailing.left());
    assert_eq!(trailing.right(), bar.right() - 16.0);
    assert_eq!(
        trailing.center().y,
        bar.top() + (size::TOOLBAR - size::HAIRLINE) / 2.0
    );
}

/// When the trailing side changes width, the bar lays out again: it keeps
/// ending at the right padding, and an empty trailing side gives the
/// leading side the whole bar.
#[test]
fn bars_relayout_when_the_trailing_side_changes() {
    use egui_kittest::kittest::Queryable;
    let mut harness = egui_kittest::Harness::new_ui_state(
        |ui, (bar, trailing): &mut (Rect, Option<&'static str>)| {
            ui.allocate_ui_with_layout(vec2(600.0, 200.0), Layout::top_down(Align::Min), |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                *bar = ui.max_rect();
                let trailing = *trailing;
                Toolbar::new("relayout").show(
                    ui,
                    |ui| {
                        type_scale::UI.label(ui, "leading", ui.tokens().text.primary);
                    },
                    |ui| {
                        if let Some(text) = trailing {
                            Button::secondary(text).show(ui);
                        }
                    },
                );
            });
        },
        (Rect::NOTHING, Some("Short")),
    );
    components::test_support::setup_egui(&mut harness);
    harness.run();
    let bar = harness.state().0;
    assert_eq!(
        harness.get_by_label("Short").rect().right(),
        bar.right() - 16.0
    );

    harness.state_mut().1 = Some("A much longer trailing action");
    harness.run();
    let trailing = harness.get_by_label("A much longer trailing action").rect();
    assert_eq!(trailing.right(), bar.right() - 16.0);
    assert!(harness.get_by_label("leading").rect().right() <= trailing.left());

    harness.state_mut().1 = None;
    harness.run();
    assert!(
        harness
            .query_by_label("A much longer trailing action")
            .is_none()
    );
    assert_eq!(
        harness.get_by_label("leading").rect().left(),
        bar.left() + 16.0
    );
}
