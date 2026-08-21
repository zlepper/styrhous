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

macro_rules! test_icon {
    ($name:ident, $path:literal) => {
        fn $name() -> Image<'static> {
            Image::new(egui::include_image!($path))
        }
    };
}

test_icon!(home_icon, "../icons/home.svg");
test_icon!(users_icon, "../icons/users.svg");
test_icon!(folder_icon, "../icons/folder.svg");
test_icon!(calendar_icon, "../icons/calendar.svg");
test_icon!(document_icon, "../icons/document.svg");
test_icon!(chart_icon, "../icons/chart-bar.svg");

#[test]
fn test_sidebar_wide_mode() {
    let mut harness = create_harness(|ui| {
        WideSidebar::new().show(ui, |sidebar| {
            sidebar.item("Dashboard", home_icon(), true);
            sidebar.item("Team", users_icon(), false);
            sidebar.item("Projects", folder_icon(), false);
            sidebar.item("Calendar", calendar_icon(), false);
            sidebar.item("Documents", document_icon(), false);
            sidebar.item("Reports", chart_icon(), false);

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
            sidebar.item("Dashboard", home_icon(), true);
            sidebar.item("Team", users_icon(), false);
            sidebar.item("Projects", folder_icon(), false);
            sidebar.item("Calendar", calendar_icon(), false);
            sidebar.item("Documents", document_icon(), false);
            sidebar.item("Reports", chart_icon(), false);
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
        WideSidebar::new().dark().show(ui, |sidebar| {
            sidebar.section_header("Resources");
            sidebar.expandable("core", folder_icon(), true, |sidebar| {
                sidebar.child_item("pods", true);
                sidebar.child_item("services", false);
            });
            sidebar.expandable("apps", folder_icon(), false, |_sidebar| {});
        });
    });

    harness.ui_harness("sidebars/test_sidebar_dark_mode/dark");
}

#[test]
fn test_sidebar_primary_text_item() {
    let mut harness = create_harness(|ui| {
        WideSidebar::new().dark().show(ui, |sidebar| {
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
            sidebar.item("Dashboard", home_icon(), true);

            sidebar.expandable("Teams", users_icon(), true, |sidebar| {
                sidebar.child_item("Engineering", false);
                sidebar.child_item("Human Resources", false);
                sidebar.child_item("Customer Success", false);
            });

            sidebar.expandable("Projects", folder_icon(), false, |sidebar| {
                sidebar.child_item("Alpha", false);
                sidebar.child_item("Beta", false);
            });

            sidebar.item("Calendar", calendar_icon(), false);
            sidebar.item("Documents", document_icon(), false);
            sidebar.item("Reports", chart_icon(), false);
        });
    });

    harness.ui_harness("sidebars/test_sidebar_expandable_sections/expandable");
}

#[test]
fn test_sidebar_expandable_toggle() {
    let mut harness = Harness::new_ui(|ui| {
        WideSidebar::new().show(ui, |sidebar| {
            sidebar.item("Dashboard", home_icon(), true);
            sidebar.expandable("Teams", users_icon(), false, |sidebar| {
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
        WideSidebar::new().dark().show(ui, |sidebar| {
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
        WideSidebar::new().dark().show(ui, |sidebar| {
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
        WideSidebar::new().dark().show(ui, |sidebar| {
            sidebar.child_item("pods", false);
        });
    });
    visible_label.get_by_label("pods").hover();
    visible_label.run();
    visible_label.ui_harness(
        "sidebars/test_sidebar_full_text_tooltips_only_appear_when_truncated/tooltip_visible_label",
    );

    let mut truncated_label = create_harness(|ui| {
        WideSidebar::new().width(160.0).dark().show(ui, |sidebar| {
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
            sidebar.item("Dashboard", home_icon(), true);
            sidebar.expandable("Teams", users_icon(), true, |sidebar| {
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
