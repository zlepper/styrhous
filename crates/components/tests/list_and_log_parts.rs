//! Focused snapshots and behaviour of the C16–C20 parts: the top bar, the
//! grouped table and problem ticks, the cluster switcher, readouts and the
//! section rail, and the log viewer parts. The boards (`c16_*` … `c20_*`)
//! check the whole layouts against their oracles.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use components::design::theme::{ThemeExt, Tone};
use components::design::{radius, size, space};
use components::test_support::{UiHarnessSnapshot, setup_egui};
use components::{
    BladeNavigation, Button, ButtonSize, ClusterAction, ClusterEntry, ClusterSwitcher,
    ClusterSwitcherState, ClusterTarget, ClusterTile, CodeMark, CodeSpan, ControlState, FilterChip,
    FloatingToolbar, GroupHeader, InspectorHeader, InspectorHeaderResponse, InspectorStatus,
    LevelBadge, LiveMeta, LogHeader, LogLevel, LogLine, LogStream, ProblemTick, ProblemTicks,
    Readout, ReadoutRow, ReadoutSize, ReadoutTone, ScopePill, SearchPill, SearchStatus, Section,
    SectionRail, SourcePill, Table, TableGroup, TableRowBuilder, TopBar, TopBarKeys, VolumeBar,
    VolumeBars, VolumeStrip,
};
use egui::{Key, Modifiers, Ui, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn harness<'a>(size: egui::Vec2, scene: impl FnMut(&mut Ui) + 'a) -> Harness<'a> {
    let mut harness = Harness::builder().with_size(size).build_ui(scene);
    setup_egui(&mut harness);
    harness.set_size(size);
    harness.run();
    harness
}

/// A column of demos 12 points apart on the canvas.
fn column(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let theme = ui.tokens();
    ui.ctx()
        .layer_painter(egui::LayerId::background())
        .rect_filled(egui::Rect::EVERYTHING, 0, theme.surface.canvas);
    ui.spacing_mut().item_spacing = vec2(space::S8, space::S12);
    add(ui);
}

// C16

#[test]
fn top_bar_parts_snapshot() {
    let mut harness = harness(vec2(760.0, 390.0), |ui| {
        column(ui, |ui| {
            let mut empty = String::new();
            let mut typed = "app=ranker".to_owned();
            ui.horizontal(|ui| {
                SearchPill::new(&mut empty).id_salt("rest").show(ui);
                SearchPill::new(&mut typed)
                    .id_salt("typing")
                    .force_state(ControlState::Focused)
                    .show(ui);
            });
            ui.horizontal(|ui| {
                let mut regex = true;
                let mut pattern = "ranker-[".to_owned();
                SearchPill::new(&mut pattern)
                    .id_salt("regex")
                    .regex(&mut regex)
                    .status(SearchStatus::Invalid(
                        "Invalid regular expression".to_owned(),
                    ))
                    .show(ui);
            });
            ui.horizontal(|ui| {
                ui.add_enabled_ui(false, |ui| {
                    SearchPill::new(&mut empty).id_salt("disabled").show(ui);
                });
                ScopePill::namespaces(["kube-system", "monitoring", "default"], Some(true))
                    .id_salt("several")
                    .combobox()
                    .filter_by(|name: &&str| name)
                    .show_items(ui, &["kube-system"], |list, name| {
                        list.item(*name, true);
                    });
            });
            ui.horizontal(|ui| {
                for (salt, pill) in [
                    ("all", ScopePill::all_namespaces(Some(12_400))),
                    ("cluster", ScopePill::cluster_wide()),
                ] {
                    pill.id_salt(salt)
                        .combobox()
                        .filter_by(|name: &&str| name)
                        .show_items(ui, &["default"], |list, name| {
                            list.item(*name, false);
                        });
                }
            });
            ui.horizontal(|ui| {
                FilterChip::grouping(Some("namespace")).show(ui);
                FilterChip::grouping(None).show(ui);
                FilterChip::problems(98, false).show(ui);
                FilterChip::problems(98, true)
                    .force_state(ControlState::Hovered)
                    .show(ui);
            });
            LiveMeta::new("12 pods").show(ui);
            LiveMeta::new("1 pod").reconnecting(true).show(ui);
            LiveMeta::new("8 pods")
                .warning(Some("namespaces unavailable"))
                .problem_keys(false)
                .show(ui);
            LiveMeta::new("3 releases")
                .live(false)
                .problem_keys(false)
                .show(ui);
        });
    });
    harness.ui_harness("top_bar/top_bar_parts");
}

#[test]
fn the_search_pill_regex_toggle_flips_regex_and_a_request_focuses_it() {
    let regex = Rc::new(RefCell::new(false));
    let state = regex.clone();
    let focus = Rc::new(RefCell::new(true));
    let request = focus.clone();
    let mut harness = harness(vec2(360.0, 60.0), move |ui| {
        let mut query = String::new();
        SearchPill::new(&mut query)
            .regex(&mut state.borrow_mut())
            .request_focus(request.replace(false))
            .show(ui);
    });
    assert!(harness.get_by_label("Search").is_focused());
    harness.get_by_label("Regular expression").click();
    harness.run();
    assert!(*regex.borrow());
}

#[test]
fn slash_focuses_the_search_pill_without_typing_a_slash() {
    let query = Rc::new(RefCell::new(String::new()));
    let state = query.clone();
    let mut harness = harness(vec2(360.0, 60.0), move |ui| {
        SearchPill::new(&mut state.borrow_mut()).show(ui);
    });
    assert!(!harness.get_by_label("Search").is_focused());
    let events = &mut harness.input_mut().events;
    events.push(egui::Event::Key {
        key: Key::Slash,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    events.push(egui::Event::Text("/".to_owned()));
    harness.run();
    harness.run();
    assert!(harness.get_by_label("Search").is_focused());
    assert_eq!(*query.borrow(), "");
}

#[test]
fn top_bar_keys_act_only_while_nothing_has_focus() {
    let keys = Rc::new(RefCell::new(Vec::<TopBarKeys>::new()));
    let seen = keys.clone();
    let mut harness = harness(vec2(360.0, 60.0), move |ui| {
        seen.borrow_mut().push(TopBar::keys(ui));
        let mut query = String::new();
        SearchPill::new(&mut query).show(ui);
    });
    harness.key_press(Key::P);
    harness.run();
    harness.key_press(Key::CloseBracket);
    harness.run();
    let pressed = keys
        .borrow()
        .iter()
        .fold(TopBarKeys::default(), |acc, keys| TopBarKeys {
            toggle_problems: acc.toggle_problems || keys.toggle_problems,
            previous_problem: acc.previous_problem || keys.previous_problem,
            next_problem: acc.next_problem || keys.next_problem,
        });
    assert!(pressed.toggle_problems && pressed.next_problem && !pressed.previous_problem);

    keys.borrow_mut().clear();
    harness.get_by_label("Search").focus();
    harness.run();
    harness.key_press(Key::P);
    harness.run();
    assert!(keys.borrow().iter().all(|keys| !keys.toggle_problems));
}

#[test]
fn a_narrow_window_shrinks_the_pills_and_drops_the_grouping_chip() {
    let wide = TopBar::new(1200.0);
    let narrow = TopBar::new(900.0);
    assert!(wide.shows_grouping() && !narrow.shows_grouping());
    assert_eq!(wide.search_width(), size::SEARCH_PILL);
    assert_eq!(narrow.search_width(), size::SEARCH_PILL_NARROW);
    assert_eq!(narrow.scope_width(), size::SCOPE_PILL_NARROW);
    assert_eq!(
        ScopePill::namespaces(["a", "b", "c"], None).text(),
        "a, b",
        "two names, then the +N chip"
    );
    assert_eq!(
        ScopePill::all_namespaces(Some(182)).text(),
        "All namespaces · 182"
    );
}

// C17

struct Pod {
    name: &'static str,
    status: (&'static str, Tone),
}

const PODS: [Pod; 3] = [
    Pod {
        name: "api-0",
        status: ("Running", Tone::Success),
    },
    Pod {
        name: "api-1",
        status: ("CrashLoopBackOff", Tone::Danger),
    },
    Pod {
        name: "worker-0",
        status: ("Pending", Tone::Warning),
    },
];

fn problem(pod: &Pod) -> Option<Tone> {
    matches!(pod.status.1, Tone::Warning | Tone::Danger).then_some(pod.status.1)
}

fn grouped_table(ui: &mut Ui, expanded: bool) -> components::GroupedTableResponse {
    let mut selected: HashSet<&str> = HashSet::new();
    Table::new("grouped")
        .column("name", "Name", |col| {
            col.fill_remaining().initial_width(0.0)
        })
        .column("status", "Status", |col| col.initial_width(160.0))
        .selectable()
        .show_grouped(
            ui,
            vec![
                TableGroup::new(
                    GroupHeader::new("api", 2).unit("pod", "pods").failing(1),
                    &PODS[..2],
                ),
                TableGroup::new(GroupHeader::new("jobs", 1).unit("pod", "pods"), &PODS[2..])
                    .expanded(expanded),
            ],
            Some(&mut selected),
            |pod: &Pod| Some(pod.name),
            None,
            problem,
            |ui, pod, column| match column {
                0 => TableRowBuilder::name(ui, pod.name),
                _ => {
                    TableRowBuilder::problem_status(ui, pod.status.0, pod.status.1);
                }
            },
        )
}

#[test]
fn grouped_table_and_problem_ticks_snapshot() {
    let mut harness = harness(vec2(560.0, 330.0), |ui| {
        column(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(500.0);
                    ui.set_height(260.0);
                    grouped_table(ui, false);
                });
                let ticks = [
                    ProblemTick {
                        position: 0.2,
                        tone: Tone::Warning,
                    },
                    ProblemTick {
                        position: 0.6,
                        tone: Tone::Danger,
                    },
                ];
                let _ = ProblemTicks::new(&ticks).thumb(0.1, 0.4).show(ui, 200.0);
            });
            ui.set_width(420.0);
            let _ = GroupHeader::new("search", 1_210)
                .unit("pod", "pods")
                .failing(31)
                .window("first 20 of 1,210")
                .force_state(ControlState::Hovered)
                .show(ui);
        });
    });
    harness.ui_harness("grouped_table/grouped_table_and_ticks");
}

#[test]
fn group_chips_toggle_and_problem_rows_become_ticks() {
    let response = Rc::new(RefCell::new(None));
    let expanded = Rc::new(RefCell::new(true));
    let (seen, state) = (response.clone(), expanded.clone());
    let mut harness = harness(vec2(560.0, 400.0), move |ui| {
        let result = grouped_table(ui, *state.borrow());
        if let Some(group) = result.toggled {
            assert_eq!(group, 1);
            let mut expanded = state.borrow_mut();
            *expanded = !*expanded;
        }
        *seen.borrow_mut() = Some((result.ticks, result.content_height));
    });
    let (ticks, height) = response.borrow().clone().expect("the table ran");
    let header = size::GROUP_HEADER;
    assert_eq!(height, 2.0 * header + 3.0 * size::ROW_PITCH);
    assert_eq!(
        ticks,
        vec![
            ProblemTick {
                position: (header + size::ROW_PITCH) / height,
                tone: Tone::Danger,
            },
            ProblemTick {
                position: (2.0 * header + 2.0 * size::ROW_PITCH) / height,
                tone: Tone::Warning,
            },
        ]
    );
    harness.get_by_label("Collapse jobs").click();
    harness.run();
    harness.run();
    assert!(!*expanded.borrow());
    assert!(harness.query_by_label("Expand jobs").is_some());
    assert!(harness.query_by_label("worker-0").is_none());
}

#[test]
fn clicking_the_track_picks_the_nearest_problem() {
    let clicked = Rc::new(RefCell::new(None));
    let seen = clicked.clone();
    let mut harness = harness(vec2(40.0, 120.0), move |ui| {
        let ticks = [
            ProblemTick {
                position: 0.1,
                tone: Tone::Warning,
            },
            ProblemTick {
                position: 0.9,
                tone: Tone::Danger,
            },
        ];
        if let Some(index) = ProblemTicks::new(&ticks).show(ui, 100.0).clicked {
            *seen.borrow_mut() = Some(index);
        }
    });
    let track = harness.get_by_label("2 problems").rect();
    let pos = egui::pos2(track.center().x, track.bottom() - 5.0);
    harness.event(egui::Event::PointerMoved(pos));
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run();
    assert_eq!(*clicked.borrow(), Some(1));
}

// C18

fn cluster(
    name: &'static str,
    key: Option<&'static str>,
    status: Option<Tone>,
) -> ClusterEntry<'static> {
    ClusterEntry {
        name,
        initials: "CL",
        category: 1,
        meta: "KIND · LOCAL",
        detail: "3 nodes",
        status,
        key,
        summary: None,
    }
}

#[test]
fn cluster_tiles_snapshot() {
    let entries = [
        cluster("kind-kind", Some("⌘1"), Some(Tone::Success)),
        cluster(
            "prod-eu-with-a-very-long-cluster-name",
            None,
            Some(Tone::Danger),
        ),
        cluster("payments", None, None),
    ];
    let mut harness = harness(vec2(340.0, 240.0), move |ui| {
        column(ui, |ui| {
            ClusterTile::new(&entries[0])
                .highlighted(true)
                .show(ui, 320.0);
            ClusterTile::new(&entries[1])
                .force_state(ControlState::Hovered)
                .show(ui, 320.0);
            ClusterTile::new(&entries[2]).show(ui, 320.0);
        });
    });
    harness.ui_harness("cluster_switcher/cluster_tiles");
}

#[test]
fn the_switcher_moves_its_highlight_switches_pins_and_closes_by_key() {
    let state = Rc::new(RefCell::new(ClusterSwitcherState::default()));
    let actions = Rc::new(RefCell::new(Vec::new()));
    let (switcher_state, seen) = (state.clone(), actions.clone());
    let mut harness = harness(vec2(1100.0, 600.0), move |ui| {
        let mut state = switcher_state.borrow_mut();
        let action = ClusterSwitcher::new(&mut state)
            .width(1000.0)
            .provider("All", 4)
            .section(
                "Pinned",
                vec![
                    cluster("kind-kind", Some("⌘1"), Some(Tone::Success)),
                    cluster("prod-eu", Some("⌘2"), Some(Tone::Danger)),
                ],
            )
            .section(
                "Recent",
                vec![
                    cluster("dev-eu", None, Some(Tone::Success)),
                    cluster("staging", None, Some(Tone::Success)),
                    cluster("qa", None, Some(Tone::Success)),
                ],
            )
            .show(ui);
        if let Some(action) = action {
            seen.borrow_mut().push(action);
        }
    });
    let target = |section, entry| ClusterTarget { section, entry };

    harness.key_press(Key::ArrowRight);
    harness.run();
    assert_eq!(state.borrow().highlight, 1);
    harness.key_press(Key::ArrowDown);
    harness.run();
    assert_eq!(
        state.borrow().highlight,
        3,
        "down moves to the tile below, not three on in a shorter row"
    );
    harness.key_press(Key::ArrowUp);
    harness.run();
    assert_eq!(state.borrow().highlight, 1);
    harness.key_press(Key::ArrowDown);
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::P);
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Num2);
    harness.run();
    harness.key_press(Key::Escape);
    harness.run();
    assert_eq!(
        *actions.borrow(),
        vec![
            ClusterAction::Switch(target(1, 1)),
            ClusterAction::OpenInNewWindow(target(1, 1)),
            ClusterAction::TogglePin(target(1, 1)),
            ClusterAction::Switch(target(0, 1)),
            ClusterAction::Close,
        ]
    );

    state.borrow_mut().query = "dev".to_owned();
    harness.run();
    assert!(harness.query_by_label_contains("staging").is_none());
    assert!(harness.query_by_label_contains("dev-eu").is_some());
    assert_eq!(
        state.borrow().highlight,
        0,
        "the highlight stays on a shown cluster"
    );
    // "eu" shows prod-eu and dev-eu.
    state.borrow_mut().query = "eu".to_owned();
    harness.run();
    harness.key_press(Key::ArrowRight);
    harness.run();
    assert_eq!(
        state.borrow().highlight,
        0,
        "while a query is typed, the arrows move the caret, not the highlight"
    );
}

// C19

#[test]
fn readouts_rail_and_inspector_header_snapshot() {
    let readouts = [
        Readout::new("Ready", "5 / 6"),
        Readout::new("Node", "ip-10-0-41-17.eu-west-1.compute.internal").tone(ReadoutTone::Link),
        Readout::new("CPU", "412m").unit("/ 2"),
        Readout::new("Failing", "98").tone(ReadoutTone::Problem),
    ];
    let sections = [
        Section::new("Overview"),
        Section::new("Containers").count(6),
        Section::new("Environment").count(63),
        Section::new("Events").count(37),
    ];
    let mut harness = harness(vec2(760.0, 520.0), move |ui| {
        column(ui, |ui| {
            ui.set_width(720.0);
            ReadoutRow::new(&readouts).show(ui);
            ReadoutRow::new(&readouts[2..])
                .size(ReadoutSize::Gauge)
                .show(ui);
            let _ = InspectorHeader::new(
                "coredns-66bc5c9577-ffw2s",
                BladeNavigation::new(false, true),
            )
            .kind("Pod", None)
            .status(InspectorStatus {
                tone: Tone::Success,
                state: "Running",
                reason: None,
            })
            .readouts(&readouts[..3])
            .show(ui, |ui| {
                Button::primary("Logs")
                    .size(ButtonSize::Sm)
                    .shortcut("L")
                    .show(ui);
            });
            let mut current = 1;
            let _ = SectionRail::new("On this pod", &sections, &mut current)
                .corner_radius(radius::CARD)
                .force_hover(2)
                .show(ui, 190.0);
        });
    });
    harness.ui_harness("readouts/readouts_rail_header");
}

#[test]
fn section_rail_selects_by_click_and_arrow_keys() {
    let current = Rc::new(RefCell::new(0));
    let state = current.clone();
    let mut harness = harness(vec2(200.0, 200.0), move |ui| {
        let sections = [
            Section::new("Overview"),
            Section::new("Containers").count(6),
            Section::new("Events").count(37),
        ];
        let _ = SectionRail::new("On this pod", &sections, &mut state.borrow_mut()).show(ui, 180.0);
    });
    harness.get_by_label("Containers, 6").click();
    harness.run();
    assert_eq!(*current.borrow(), 1);

    // ↓ moves one section, not on through every section after it.
    harness.get_by_label("Overview").focus();
    harness.run();
    harness.key_press(Key::ArrowDown);
    harness.run();
    assert_eq!(*current.borrow(), 1);
    assert!(harness.get_by_label("Containers, 6").is_focused());
    harness.key_press(Key::ArrowDown);
    harness.run();
    assert_eq!(*current.borrow(), 2);
    assert!(harness.get_by_label("Events, 37").is_focused());

    // The ends hold: ↓ on the last section and ↑ on the first stay put.
    harness.key_press(Key::ArrowDown);
    harness.run();
    assert!(harness.get_by_label("Events, 37").is_focused());
    for _ in 0..3 {
        harness.key_press(Key::ArrowUp);
        harness.run();
    }
    assert_eq!(*current.borrow(), 0);
    assert!(harness.get_by_label("Overview").is_focused());
}

#[test]
fn inspector_header_reports_back_and_close() {
    let response = Rc::new(RefCell::new(InspectorHeaderResponse::default()));
    let seen = response.clone();
    let mut harness = harness(vec2(720.0, 160.0), move |ui| {
        let result = InspectorHeader::new("api-0", BladeNavigation::new(true, false))
            .kind("Pod", None)
            .show(ui, |_| {});
        let mut seen = seen.borrow_mut();
        seen.back |= result.back;
        seen.close |= result.close;
    });
    harness.get_by_label("Back").click();
    harness.run();
    harness.get_by_label("Close blade").click();
    harness.run();
    let response = *response.borrow();
    assert!(response.back && response.close && !response.forward);
}

// C20

#[test]
fn log_viewer_parts_snapshot() {
    let bars: Vec<VolumeBar> = (0..60)
        .map(|index| VolumeBar {
            height: 0.2 + 0.6 * ((index * 7 % 11) as f32 / 10.0),
            tone: match index {
                20 => Some(Tone::Danger),
                40 => Some(Tone::Warning),
                _ => None,
            },
        })
        .collect();
    let mut harness = harness(vec2(760.0, 520.0), move |ui| {
        column(ui, |ui| {
            ui.set_width(720.0);
            for (index, stream) in [LogStream::Following, LogStream::Unavailable]
                .into_iter()
                .enumerate()
            {
                ui.push_id(index, |ui| {
                    LogHeader::new("Pod", "api-0", "container api · 1.2k lines", stream)
                        .show(ui, |_| {});
                });
            }
            let _ = VolumeStrip::new(
                "Volume · 15 min · 1 error",
                VolumeBars::new(&bars).window(30.0, 12.0),
            )
            .source(SourcePill::live("api-0", 1_204))
            .source(SourcePill::reconnecting("api-1"))
            .show(ui);
            ui.horizontal(|ui| {
                for level in [
                    LogLevel::Error,
                    LogLevel::Warn,
                    LogLevel::Info,
                    LogLevel::Debug,
                ] {
                    LevelBadge(level).show(ui);
                }
            });
            let theme = ui.tokens();
            let (panel, _) =
                ui.allocate_exact_size(vec2(720.0, 2.0 * size::LOG_ROW), egui::Sense::hover());
            ui.painter().rect_filled(panel, 0, theme.code.surface);
            let mut lines = ui.new_child(egui::UiBuilder::new().max_rect(panel));
            lines.spacing_mut().item_spacing = egui::Vec2::ZERO;
            LogLine::new([
                CodeSpan::text("read "),
                CodeSpan::text("timeout").mark(CodeMark::Underline),
            ])
            .number(41)
            .time("15:20:00.000")
            .level(LogLevel::Error)
            .show(&mut lines);
            LogLine::new([CodeSpan::text("single source, no pill")])
                .number(42)
                .level(LogLevel::Info)
                .current(true)
                .show(&mut lines);
            FloatingToolbar::show(ui, |ui| {
                components::ToggleButton::text("Only matches", true).show(ui);
                components::ToggleButton::text("Follow", false)
                    .leading_small_icon(components::icons::Icon::ChevronDoubleDown)
                    .show(ui);
            });
        });
    });
    harness.ui_harness("log_viewer/log_viewer_parts");
}

#[test]
fn source_pills_report_clicks_and_streams_name_their_state() {
    let clicked = Rc::new(RefCell::new(None));
    let seen = clicked.clone();
    let mut harness = harness(vec2(520.0, 120.0), move |ui| {
        let bars = [VolumeBar {
            height: 0.5,
            tone: None,
        }];
        let response = VolumeStrip::new("Volume", VolumeBars::new(&bars))
            .source(SourcePill::live("api-0", 3))
            .source(SourcePill::offline("api-1"))
            .show(ui);
        if let Some(index) = response.source_clicked {
            *seen.borrow_mut() = Some(index);
        }
    });
    harness.get_by_label("api-1 · offline").click();
    harness.run();
    assert_eq!(*clicked.borrow(), Some(1));

    assert_eq!(
        LogStream::Streaming { live: 5, total: 6 }.pill(),
        ("5 of 6 streaming".to_owned(), Tone::Warning)
    );
    assert_eq!(
        LogStream::Streaming { live: 6, total: 6 }.pill().1,
        Tone::Success
    );
    assert_eq!(LogStream::Following.pill().0, "Following");
}
