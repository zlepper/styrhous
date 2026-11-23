use super::*;
use crate::test_support::UiHarnessSnapshot;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::cell::RefCell;
use std::rc::Rc;

fn create_harness<'a>(app: impl FnMut(&mut Ui) + 'a) -> Harness<'a> {
    let mut harness = Harness::new_ui(app);
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness
}

use crate::icons::Icon;

#[test]
fn test_sidebar_wide_mode() {
    let mut harness = create_harness(|ui| {
        WideSidebar::new().show(ui, |sidebar| {
            sidebar.item("Dashboard", Icon::Home.image(), true);
            sidebar.item("Team", Icon::Users.image(), false);
            sidebar.item("Projects", Icon::Folder.image(), false);
            sidebar.item("Calendar", Icon::Calendar.image(), false);
            sidebar.item("Documents", Icon::Document.image(), false);
            sidebar.item("Reports", Icon::ChartBar.image(), false);

            sidebar.section_header("Your teams");

            sidebar.avatar_item("Heroicons", "H", false);
            sidebar.avatar_item("Tailwind Labs", "T", false);
            sidebar.avatar_item("Workcation", "W", false);
        });
    });

    harness.ui_harness("sidebars/test_sidebar_wide_mode/wide");
}

#[test]
fn test_sidebar_narrow_mode() {
    let mut harness = create_harness(|ui| {
        NarrowSidebar::new().show(ui, |sidebar| {
            sidebar.item("Dashboard", Icon::Home.image(), true);
            sidebar.item("Team", Icon::Users.image(), false);
            sidebar.item("Projects", Icon::Folder.image(), false);
            sidebar.item("Calendar", Icon::Calendar.image(), false);
            sidebar.item("Documents", Icon::Document.image(), false);
            sidebar.item("Reports", Icon::ChartBar.image(), false);
        });
    });

    harness.ui_harness("sidebars/test_sidebar_narrow_mode/narrow");
}

#[test]
fn test_sidebar_narrow_avatars() {
    let mut harness = create_harness(|ui| {
        NarrowSidebar::new().show(ui, |sidebar| {
            sidebar.avatar_item("Production", "P", true);
            sidebar.avatar_item("Development", "D", false);
            sidebar.avatar_item("Staging", "S", false);
        });
    });

    harness.ui_harness("sidebars/test_sidebar_narrow_avatars/narrow_avatars");
}

#[test]
fn test_sidebar_dark_mode() {
    let mut harness = create_harness(|ui| {
        WideSidebar::new().show(ui, |sidebar| {
            sidebar.section_header("Resources");
            sidebar.expandable("core", Icon::Folder.image(), true, |sidebar| {
                sidebar.child_item("pods", true);
                sidebar.child_item("services", false);
            });
            sidebar.expandable("apps", Icon::Folder.image(), false, |_sidebar| {});
        });
    });

    harness.ui_harness("sidebars/test_sidebar_dark_mode/dark");
}

#[test]
fn test_sidebar_primary_text_item() {
    let mut harness = create_harness(|ui| {
        WideSidebar::new().show(ui, |sidebar| {
            sidebar.primary_text_item("nodes", true);
            sidebar.primary_text_item("namespaces", false);
        });
    });

    harness.ui_harness("sidebars/test_sidebar_primary_text_item/primary_text_item");
}

#[test]
fn test_sidebar_expandable_sections() {
    let mut harness = create_harness(|ui| {
        WideSidebar::new().show(ui, |sidebar| {
            sidebar.item("Dashboard", Icon::Home.image(), true);

            sidebar.expandable("Teams", Icon::Users.image(), true, |sidebar| {
                sidebar.child_item("Engineering", false);
                sidebar.child_item("Human Resources", false);
                sidebar.child_item("Customer Success", false);
            });

            sidebar.expandable("Projects", Icon::Folder.image(), false, |sidebar| {
                sidebar.child_item("Alpha", false);
                sidebar.child_item("Beta", false);
            });

            sidebar.item("Calendar", Icon::Calendar.image(), false);
            sidebar.item("Documents", Icon::Document.image(), false);
            sidebar.item("Reports", Icon::ChartBar.image(), false);
        });
    });

    harness.ui_harness("sidebars/test_sidebar_expandable_sections/expandable");
}

#[test]
fn test_sidebar_expandable_toggle() {
    let mut harness = Harness::new_ui(|ui| {
        WideSidebar::new().show(ui, |sidebar| {
            sidebar.item("Dashboard", Icon::Home.image(), true);
            sidebar.expandable("Teams", Icon::Users.image(), false, |sidebar| {
                sidebar.child_item("Engineering", false);
                sidebar.child_item("Design", false);
            });
        });
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();

    harness.ui_harness("sidebars/test_sidebar_expandable_toggle/expandable_toggle_collapsed");

    let teams_node = harness.get_by_label("Teams");
    let center = teams_node.rect().center();

    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(center));
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: center,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();

    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: center,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();

    harness.ui_harness("sidebars/test_sidebar_expandable_toggle/expandable_toggle_expanded");
}

#[test]
fn test_sidebar_parent_hover_is_a_full_rounded_row() {
    let mut harness = create_harness(|ui| {
        WideSidebar::new().show(ui, |sidebar| {
            sidebar.expandable_text("Apps & Containers", true, |sidebar| {
                sidebar.child_item("pods", false);
                sidebar.child_item("deployments", false);
            });
        });
    });

    harness.get_by_label("Apps & Containers").hover();
    harness.run();

    harness.ui_harness(
        "sidebars/test_sidebar_parent_hover_is_a_full_rounded_row/resource_parent_hover",
    );
}

#[test]
fn test_sidebar_open_resource_parent_keeps_the_closed_row_height() {
    let heights = Rc::new(RefCell::new((0.0, 0.0)));
    let heights_for_ui = heights.clone();
    let _harness = create_harness(move |ui| {
        WideSidebar::new().show(ui, |sidebar| {
            let open = sidebar.expandable_text("Open resources", true, |_sidebar| {});
            heights_for_ui.borrow_mut().0 = open.header.rect.height();

            let closed = sidebar.expandable_text("Closed resources", false, |_sidebar| {});
            heights_for_ui.borrow_mut().1 = closed.header.rect.height();
        });
    });

    let (open_height, closed_height) = *heights.borrow();
    assert_eq!(open_height, closed_height);
    assert_eq!(open_height, WIDE_GROUP_HEIGHT);
}

#[test]
fn test_sidebar_full_text_tooltips_only_appear_when_truncated() {
    let mut visible_label = create_harness(|ui| {
        WideSidebar::new().show(ui, |sidebar| {
            sidebar.child_item("pods", false);
        });
    });
    visible_label.get_by_label("pods").hover();
    visible_label.run();
    visible_label.ui_harness(
        "sidebars/test_sidebar_full_text_tooltips_only_appear_when_truncated/tooltip_visible_label",
    );

    let mut truncated_label = create_harness(|ui| {
        WideSidebar::new().width(160.0).show(ui, |sidebar| {
            sidebar.child_item("very-long-resource-name-that-needs-truncation", false);
        });
    });
    truncated_label
        .get_by_label("very-long-resource-name-that-needs-truncation")
        .hover();
    truncated_label.run();
    truncated_label.ui_harness("sidebars/test_sidebar_full_text_tooltips_only_appear_when_truncated/tooltip_truncated_label");
}

#[test]
fn test_sidebar_child_item_click() {
    let clicked: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let clicked_clone = clicked.clone();

    let mut harness = Harness::new_ui(move |ui| {
        WideSidebar::new().show(ui, |sidebar| {
            sidebar.item("Dashboard", Icon::Home.image(), true);
            sidebar.expandable("Teams", Icon::Users.image(), true, |sidebar| {
                if sidebar.child_item("Engineering", false).clicked() {
                    *clicked_clone.borrow_mut() = Some("Engineering".to_string());
                }
                if sidebar.child_item("Design", false).clicked() {
                    *clicked_clone.borrow_mut() = Some("Design".to_string());
                }
            });
        });
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();

    // Click on Engineering using accessibility
    harness.get_by_label("Engineering").click();
    harness.run();

    assert_eq!(
        *clicked.borrow(),
        Some("Engineering".to_string()),
        "Engineering should be clicked via accessibility"
    );
}

#[test]
fn test_sidebar_loading_item_lines_up_with_section_labels() {
    {
        let mut harness = Harness::new_ui(move |ui| {
            WideSidebar::new().width(240.0).show(ui, |sidebar| {
                sidebar.expandable_text("Section", false, |_| ());
                sidebar.loading_item("Discovering API groups...");
            });
        });
        crate::test_support::setup_egui(&mut harness);
        harness.set_size(Vec2::new(280.0, 140.0));
        // The spinner keeps repainting.
        harness.run_steps(3);

        let section = harness.get_by_label("Section").rect();
        let loading = harness.get_by_label("Discovering API groups...").rect();
        let label_left = text_pos_after_chevron(
            section.shrink2(Vec2::new(ITEM_PADDING_X, 0.0)),
            WIDE_GROUP_HEIGHT,
        )
        .x;
        assert_eq!(
            loading.left(),
            label_left,
            "the text starts where section labels do"
        );
        harness.ui_harness("sidebars/test_sidebar_loading_item/dark");
    }
}
