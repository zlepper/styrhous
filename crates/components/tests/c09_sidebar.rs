//! The C09 "Sidebar" component board (`docs/design-oracles/harbour/src/C09.html`)
//! rebuilt from the real components, compared with its oracle by
//! `scripts/design-oracles/compare.sh C09`.

use components::design::theme::{ThemeExt, Tone};
use components::design::type_scale;
use components::design::{radius, size, space};
use components::icons::Icon;
use components::test_support::board::{
    BoardHeader, Run, WIDE_DESCRIPTION, board_with_description_width, cell, column, row,
    rules_list, snapshot_board,
};
use components::{
    ClusterCard, ControlState, FloatingSidebar, SideItem, kind_filter, side_caption, side_footer,
};
use egui::{Align, Layout, Rect, Ui, pos2, vec2};

const HEADER: BoardHeader<'static> = BoardHeader {
    eyebrow: "C09 · components::{Sidebar, ClusterCard, KindList}",
    title: "Sidebar",
    description: "One floating panel replaces the cluster rail and navigator: the cluster card \
                  switches clusters (opens C18), the kind list switches resource types. Kind \
                  colours match the kind badges on blades.",
};

const COLUMN_GAP: f32 = 40.0;
/// Gap under a column's caption (`margin-bottom: 10px`).
const CAPTION_GAP: f32 = 10.0;
/// Gap between the demos of a column.
const DEMO_GAP: f32 = 14.0;
/// Gap between a demo and its caption.
const DEMO_CAPTION_GAP: f32 = 6.0;
const SIDEBAR_HEIGHT: f32 = 800.0;
const RULES_WIDTH: f32 = 300.0;
/// The boxes around the kind list item demos.
const ITEM_BOX_PADDING: f32 = space::S8;

/// A column's caption and the gap under it.
fn caption(ui: &mut Ui, text: &str) {
    type_scale::CAPTION.label(ui, text, ui.tokens().text.muted);
    ui.add_space(CAPTION_GAP);
}

fn category(ui: &Ui, index: usize) -> egui::Color32 {
    ui.tokens().category.nth(index)
}

fn sidebar(ui: &mut Ui) {
    cell(
        ui,
        vec2(size::SIDEBAR, 0.0),
        Layout::top_down(Align::Min),
        0.0,
        |ui| {
            caption(ui, "Sidebar");
            let rect = Rect::from_min_size(ui.cursor().min, vec2(size::SIDEBAR, SIDEBAR_HEIGHT));
            FloatingSidebar::new(rect).show(
                ui,
                |ui| {
                    let _ =
                        ClusterCard::new("kind-kind", "2 of 4", "KK", 1, Tone::Success).show(ui);
                    kind_filter(ui, &mut String::new(), "Filter 54 kinds");
                    side_caption(ui, "Pinned", None);
                    let kinds = [
                        ("Pods", 3, 18),
                        ("Deployments", 1, 4),
                        ("Nodes", 4, 3),
                        ("Helm releases", 0, 2),
                    ];
                    for (index, (label, colour, count)) in kinds.into_iter().enumerate() {
                        let _ = SideItem::kind(label, category(ui, colour))
                            .count(count)
                            .selected(index == 0)
                            .show(ui);
                    }
                    side_caption(ui, "Kubernetes", Some("54"));
                    for (label, count) in [
                        ("Workloads", 6),
                        ("Network", 7),
                        ("Config & secrets", 4),
                        ("Storage", 5),
                        ("Cluster", 14),
                        ("Access control", 8),
                    ] {
                        let _ = SideItem::group(label, false).count(count).show(ui);
                    }
                    for (label, colour, count) in [("Namespaces", 5, 5), ("Events", 2, 42)] {
                        let _ = SideItem::kind(label, category(ui, colour))
                            .count(count)
                            .show(ui);
                    }
                    side_caption(ui, "Custom resources", Some("6 groups"));
                    for group in ["helm.sh", "metrics.k8s.io"] {
                        let _ = SideItem::api_group(group, false).count(2).show(ui);
                    }
                },
                |ui| {
                    let _ = side_footer(
                        ui,
                        SideItem::action(Icon::Bell, "Notifications").alert_count(1),
                        SideItem::action(Icon::Cog6Tooth, "Settings"),
                    );
                },
            );
        },
    );
}

fn demo(ui: &mut Ui, caption: &str, add: impl FnOnce(&mut Ui)) {
    column(ui, DEMO_CAPTION_GAP, |ui| {
        ui.set_width(size::SIDEBAR);
        add(ui);
        type_scale::META.label(ui, caption, ui.tokens().text.muted);
    });
}

fn cluster_cards(ui: &mut Ui) {
    cell(
        ui,
        vec2(size::SIDEBAR, 0.0),
        Layout::top_down(Align::Min),
        0.0,
        |ui| {
            caption(ui, "Cluster card");
            ui.add_space(DEMO_GAP);
            column(ui, DEMO_GAP, |ui| {
                demo(ui, "Connected", |ui| {
                    let _ =
                        ClusterCard::new("kind-kind", "2 OF 4", "KK", 1, Tone::Success).show(ui);
                });
                demo(ui, "Connecting — lamp warn", |ui| {
                    let _ =
                        ClusterCard::new("dev-eu", "CONNECTING", "DE", 0, Tone::Warning).show(ui);
                });
                demo(ui, "Unreachable — lamp bad", |ui| {
                    let _ = ClusterCard::new("prod-eu", "UNREACHABLE", "PR", 3, Tone::Danger)
                        .force_state(ControlState::Hovered)
                        .show(ui);
                });
                demo(ui, "No cluster", |ui| {
                    let _ = ClusterCard::empty("Choose a cluster", "4 contexts", "⌘K").show(ui);
                });
            });
        },
    );
}

/// A demo box of kind list items on `nav-surface` (`.n-surface`).
fn item_box(ui: &mut Ui, caption: &str, add: impl FnOnce(&mut Ui)) {
    demo(ui, caption, |ui| {
        let theme = ui.tokens();
        let top = ui.cursor().min;
        let background = ui.painter().add(egui::Shape::Noop);
        let inner = Rect::from_min_size(
            top + vec2(ITEM_BOX_PADDING, ITEM_BOX_PADDING),
            vec2(size::SIDEBAR - 2.0 * ITEM_BOX_PADDING, 0.0),
        );
        let mut items = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(
                    inner.min,
                    pos2(inner.right(), f32::INFINITY),
                ))
                .layout(Layout::top_down(Align::Min)),
        );
        items.spacing_mut().item_spacing = egui::Vec2::ZERO;
        add(&mut items);
        let rect = Rect::from_min_max(
            top,
            pos2(
                top.x + size::SIDEBAR,
                items.min_rect().bottom() + ITEM_BOX_PADDING,
            ),
        );
        ui.painter().set(
            background,
            egui::epaint::RectShape::filled(rect, radius::CLUSTER_CARD, theme.nav.surface),
        );
        ui.allocate_rect(rect, egui::Sense::hover());
    });
}

fn kind_items(ui: &mut Ui) {
    cell(
        ui,
        vec2(size::SIDEBAR, 0.0),
        Layout::top_down(Align::Min),
        0.0,
        |ui| {
            caption(ui, "Kind list items");
            ui.add_space(DEMO_GAP);
            column(ui, DEMO_GAP, |ui| {
                item_box(ui, "Rest · kind colour + count", |ui| {
                    let _ = SideItem::kind("Deployments", category(ui, 1))
                        .count(4)
                        .show(ui);
                });
                item_box(ui, "Hover", |ui| {
                    let _ = SideItem::kind("Deployments", category(ui, 1))
                        .count(4)
                        .force_state(ControlState::Hovered)
                        .show(ui);
                });
                item_box(ui, "Selected · count in nav-accent", |ui| {
                    let _ = SideItem::kind("Pods", category(ui, 3))
                        .count(18)
                        .selected(true)
                        .show(ui);
                });
                item_box(ui, "Group collapsed · expanded · child", |ui| {
                    let _ = SideItem::group("Network", false).count(7).show(ui);
                    let _ = SideItem::group("Workloads", true).count(6).show(ui);
                    let _ = SideItem::child("StatefulSets").count(1).show(ui);
                });
                item_box(ui, "Custom resource group (API group in mono)", |ui| {
                    let _ = SideItem::api_group("cert-manager.io", false)
                        .count(6)
                        .show(ui);
                });
                item_box(ui, "Footer · Settings blade open", |ui| {
                    ui.horizontal(|ui| {
                        let _ = side_footer(
                            ui,
                            SideItem::action(Icon::Bell, "Notifications").alert_count(3),
                            SideItem::action(Icon::Cog6Tooth, "Settings").selected(true),
                        );
                    });
                });
            });
        },
    );
}
const RULES: [&[Run]; 5] = [
    &[Run::Text(
        "Floating: 12 px inset, radius 20, nav-surface on surface-canvas.",
    )],
    &[
        Run::Text("Order: cluster card · kind filter ("),
        Run::Key("G"),
        Run::Text(") · Pinned · Kubernetes groups · Custom resources by API group · footer."),
    ],
    &[Run::Text(
        "Counts are live and in mono; a kind with problems may show its count in danger-text.",
    )],
    &[Run::Text(
        "Pinned kinds are user-chosen (drag from the list); the first four kinds are pinned by default.",
    )],
    &[Run::Text(
        "Selection is item-selected with the count in nav-accent — never a solid accent fill.",
    )],
];

fn rules(ui: &mut Ui) {
    cell(
        ui,
        vec2(RULES_WIDTH, 0.0),
        Layout::top_down(Align::Min),
        0.0,
        |ui| {
            caption(ui, "Rules");
            rules_list(ui, RULES_WIDTH, &RULES);
        },
    );
}

fn c09_scene(ui: &mut Ui) {
    board_with_description_width(ui, HEADER, WIDE_DESCRIPTION, |ui| {
        row(ui, 0.0, COLUMN_GAP, Align::Min, |ui| {
            sidebar(ui);
            cluster_cards(ui);
            kind_items(ui);
            rules(ui);
        });
    });
}

#[test]
fn c09_sidebar_board() {
    snapshot_board("c09_sidebar", c09_scene);
}

/// Sidebar items report clicks and carry their counts for assistive
/// technologies; the cluster card is read as the switcher with its cluster.
#[test]
fn sidebar_items_report_clicks_and_describe_their_counts() {
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};

    let mut harness = Harness::builder()
        .with_size(vec2(320.0, 240.0))
        .build_ui_state(
            |ui, clicked: &mut Vec<&'static str>| {
                ui.set_width(size::SIDEBAR);
                if ClusterCard::new("kind-kind", "2 of 4", "KK", 1, Tone::Success)
                    .show(ui)
                    .clicked()
                {
                    clicked.push("card");
                }
                if SideItem::kind("Pods", category(ui, 3))
                    .count(18)
                    .show(ui)
                    .clicked()
                {
                    clicked.push("Pods");
                }
                ui.horizontal(|ui| {
                    let (notifications, _) = side_footer(
                        ui,
                        SideItem::action(Icon::Bell, "Notifications").alert_count(2),
                        SideItem::action(Icon::Cog6Tooth, "Settings"),
                    );
                    if notifications.clicked() {
                        clicked.push("Notifications");
                    }
                });
            },
            Vec::new(),
        );
    components::test_support::setup_egui(&mut harness);
    harness.set_size(vec2(320.0, 240.0));
    harness.run();

    assert_eq!(
        harness
            .get_by_label("Pods")
            .accesskit_node()
            .description()
            .as_deref(),
        Some("18")
    );
    assert_eq!(
        harness
            .get_by_label("Notifications")
            .accesskit_node()
            .description()
            .as_deref(),
        Some("2 unread")
    );
    assert_eq!(
        harness
            .get_by_label("Switch cluster")
            .accesskit_node()
            .description()
            .as_deref(),
        Some("kind-kind, 2 of 4")
    );
    harness.get_by_label("Switch cluster").click();
    harness.get_by_label("Pods").click();
    harness.get_by_label("Notifications").click();
    harness.run();
    assert_eq!(*harness.state(), ["card", "Pods", "Notifications"]);
}

/// Group rows report whether they are expanded; a label too long for the
/// sidebar is cut and shows its full words on hover.
#[test]
fn sidebar_groups_report_expansion_and_long_labels_elide() {
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};

    const LONG: &str = "certificaterequests.cert-manager.io.example.internal";
    let mut harness = Harness::builder()
        .with_size(vec2(320.0, 200.0))
        .build_ui(|ui| {
            ui.set_width(size::SIDEBAR);
            let _ = SideItem::group("Workloads", true).count(6).show(ui);
            let _ = SideItem::group("Network", false).show(ui);
            let _ = SideItem::api_group(LONG, false).count(3).show(ui);
        });
    components::test_support::setup_egui(&mut harness);
    harness.set_size(vec2(320.0, 200.0));
    harness.run();

    assert_eq!(
        harness
            .get_by_label("Workloads")
            .accesskit_node()
            .data()
            .is_expanded(),
        Some(true)
    );
    assert_eq!(
        harness
            .get_by_label("Network")
            .accesskit_node()
            .data()
            .is_expanded(),
        Some(false)
    );
    let long = harness.get_by_label(LONG);
    assert!(long.rect().width() <= size::SIDEBAR);
    long.hover();
    harness.run_steps(30);
    assert!(
        harness.query_all_by_label(LONG).count() >= 2,
        "the cut label shows its full text in a tooltip"
    );
}

/// `G` focuses the kind filter while nothing else has focus, and types into a
/// focused field instead.
#[test]
fn kind_filter_takes_focus_on_g() {
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};

    let mut harness = Harness::builder()
        .with_size(vec2(320.0, 80.0))
        .build_ui_state(
            |ui, query: &mut String| {
                ui.set_width(size::SIDEBAR);
                let _ = kind_filter(ui, query, "Filter 54 kinds");
            },
            String::new(),
        );
    components::test_support::setup_egui(&mut harness);
    harness.set_size(vec2(320.0, 80.0));
    harness.run();
    let focused = |harness: &Harness<'_, String>| {
        harness
            .get_by_label("Filter kinds")
            .accesskit_node()
            .is_focused()
    };
    assert!(!focused(&harness));

    harness.key_press(egui::Key::G);
    harness.run();
    assert!(focused(&harness), "G focuses the filter");
    assert!(harness.state().is_empty(), "the shortcut is not typed");

    harness.event(egui::Event::Text("g".into()));
    harness.run();
    assert_eq!(harness.state(), "g", "a focused filter takes the letter");
}

/// A lamp marks an action item that needs attention (Settings with an update
/// waiting), on the top-right of its icon.
#[test]
fn side_item_lamp_marks_its_icon() {
    use components::test_support::UiHarnessSnapshot;
    use egui_kittest::Harness;

    let mut harness = Harness::builder()
        .with_size(vec2(size::SIDEBAR, 60.0))
        .build_ui(|ui| {
            components::design::theme::scope(
                ui,
                components::design::theme::ThemeScope::Sidebar,
                |ui| {
                    ui.horizontal(|ui| {
                        let _ = side_footer(
                            ui,
                            SideItem::action(Icon::Bell, "Notifications"),
                            SideItem::action(Icon::Cog6Tooth, "Settings").lamp(Tone::Warning),
                        );
                    });
                },
            );
        });
    components::test_support::setup_egui(&mut harness);
    harness.set_size(vec2(size::SIDEBAR, 60.0));
    harness.run();
    harness.ui_harness("c09_sidebar/side_item_lamp");
}

/// The cluster card opens its menu of clusters below it; choosing one
/// closes it.
#[test]
fn menu_below_a_trigger_opens_and_closes_on_click() {
    use components::MoreButton;
    use components::test_support::UiHarnessSnapshot;
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};

    let mut harness = Harness::builder()
        .with_size(vec2(320.0, 220.0))
        .build_ui_state(
            |ui, chosen: &mut Option<&'static str>| {
                ui.set_width(size::SIDEBAR);
                let card = ClusterCard::new("kind-kind", "2 OF 2", "KK", 1, Tone::Success).show(ui);
                MoreButton::show_menu_below(&card, "Clusters", |menu| {
                    for cluster in ["dev-eu", "kind-kind"] {
                        if menu
                            .item(components::MenuItem::new(cluster).icon(Icon::ServerStack))
                            .clicked()
                        {
                            *chosen = Some(cluster);
                        }
                    }
                });
            },
            None,
        );
    components::test_support::setup_egui(&mut harness);
    harness.set_size(vec2(320.0, 220.0));
    harness.run();
    let expanded = |harness: &Harness<'_, Option<&'static str>>| {
        harness
            .get_by_label("Switch cluster")
            .accesskit_node()
            .data()
            .is_expanded()
    };
    assert_eq!(expanded(&harness), Some(false));

    harness.get_by_label("Switch cluster").click();
    harness.run();
    assert_eq!(expanded(&harness), Some(true));
    let card = harness.get_by_label("Switch cluster").rect();
    let first = harness
        .get_by_role_and_label(egui::accesskit::Role::MenuItem, "dev-eu")
        .rect();
    assert!(
        first.top() > card.bottom(),
        "the menu opens below its trigger"
    );
    components::test_support::rest_pointer(&mut harness);
    harness.ui_harness("c09_sidebar/menu_below_cluster_card");

    harness
        .get_by_role_and_label(egui::accesskit::Role::MenuItem, "dev-eu")
        .click();
    harness.run();
    assert_eq!(*harness.state(), Some("dev-eu"));
    assert_eq!(expanded(&harness), Some(false));
}
