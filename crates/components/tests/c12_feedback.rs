//! The C12 "Toast, banner, callout & empty state" component board
//! (`docs/design-oracles/harbour/src/C12.html`) rebuilt from the real
//! components, compared with its oracle by `scripts/design-oracles/compare.sh C12`.

use components::design::theme::{ThemeExt, Tone};
use components::design::{radius, space};
use components::icons::Icon;
use components::test_support::board::{
    BoardHeader, board, cell, column, row, section, snapshot_board,
};
use components::test_support::{UiHarnessSnapshot, setup_egui};
use components::{Banner, Callout, EmptyState, Spinner, TOAST_WIDTH, Toast};
use egui::{Align, Layout, Ui, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

const HEADER: BoardHeader<'static> = BoardHeader {
    eyebrow: "C12 · components::{Toast, Banner, Callout, EmptyState}",
    title: "Toast, banner, callout & empty state",
    description: "Toasts report the outcome of something the user just did; banners report \
                  app-wide conditions; callouts explain something in place; empty states say \
                  what is missing and what to do next.",
};

/// Gap between the board's rows and between the banner and callout sections.
const SECTION_GAP: f32 = 28.0;
/// Gap between the toast column and the banner/callout column.
const COLUMN_GAP: f32 = 48.0;
/// Gap between stacked toasts, banners and callouts.
const STACK_GAP: f32 = space::S8;
/// Width of the banner and callout demos.
const WIDE: f32 = 640.0;
/// The empty-state cards.
const CARD: egui::Vec2 = vec2(300.0, 176.0);
const CARD_GAP: f32 = space::S16;
const CARD_PADDING_X: f32 = space::S16;

fn toasts(ui: &mut Ui) {
    section(ui, "Toast", |ui| {
        column(ui, STACK_GAP, |ui| {
            let _ = Toast::new(Tone::Success, "CronJob started")
                .detail("nightly-report · kube-system")
                .action("View job")
                .show(ui);
            let _ = Toast::new(Tone::Danger, "Couldn’t run CronJob")
                .detail("Forbidden: cannot create jobs.batch")
                .show(ui);
            let _ = Toast::new(Tone::Warning, "1 API group couldn’t be loaded")
                .detail("metrics.k8s.io/v1beta1 · retrying")
                .show(ui);
            let _ = Toast::new(Tone::Accent, "Update ready")
                .detail("Restart Styrhous to use 0.9.2")
                .action("Restart now")
                .show(ui);
        });
    });
}

fn banners_and_callouts(ui: &mut Ui) {
    column(ui, SECTION_GAP, |ui| {
        section(ui, "Banner (spans the workspace)", |ui| {
            column(ui, STACK_GAP, |ui| {
                ui.set_width(WIDE);
                let _ = Banner::new(Tone::Warning, "License grace period ends 1 Jan 2100.")
                    .action("Manage license")
                    .dismissible(true)
                    .show(ui);
                let _ = Banner::new(
                    Tone::Danger,
                    "License check failed: the licensing service is unreachable.",
                )
                .action("Retry")
                .dismissible(true)
                .show(ui);
                let _ = Banner::new(
                    Tone::Info,
                    "Viewing previous container instance — logs are not live.",
                )
                .show(ui);
            });
        });
        section(ui, "Callout (inline)", |ui| {
            column(ui, STACK_GAP, |ui| {
                ui.set_width(WIDE);
                Callout::new(Tone::Info, "Use {command} as the placeholder")
                    .body("It is replaced with the complete, already-quoted kubectl command.")
                    .show(ui);
                Callout::new(Tone::Warning, "Values may contain secrets")
                    .body("Treat this content like a Kubernetes Secret.")
                    .show(ui);
                Callout::new(Tone::Danger, "Kubeconfig is read-only")
                    .body("Styrhous could not write to ~/.kube/config.")
                    .show(ui);
                Callout::new(Tone::Success, "Added to kubeconfig")
                    .body("payments · eu-west-1")
                    .show(ui);
            });
        });
    });
}

fn empty_states(ui: &mut Ui) {
    section(ui, "Empty, error & loading states", |ui| {
        row(ui, CARD.y, CARD_GAP, Align::Min, |ui| {
            let states = [
                EmptyState::new(Icon::ServerStack, "Choose a cluster")
                    .description("Select a context from the rail to start exploring."),
                EmptyState::new(Icon::Cube, "No pods in kube-system")
                    .description("Try another namespace or All namespaces."),
                EmptyState::error("Couldn’t watch pods")
                    .description("Forbidden: user cannot list pods")
                    .action_with_icon(Icon::ArrowPath, "Retry"),
                EmptyState::loading("Loading pods…")
                    .spinner(Spinner::new().rotation(0.0))
                    .description("Waiting for the first list response."),
            ];
            for state in states {
                cell(ui, CARD, Layout::top_down(Align::Min), 0.0, |ui| {
                    let card = ui.max_rect();
                    ui.painter()
                        .rect_filled(card, radius::CARD, ui.tokens().card_fill());
                    let inner = card.shrink2(vec2(CARD_PADDING_X, 0.0));
                    let mut inner_ui = ui.new_child(egui::UiBuilder::new().max_rect(inner));
                    state.show(&mut inner_ui);
                });
            }
        });
    });
}

fn c12_scene(ui: &mut Ui) {
    board(ui, HEADER, |ui| {
        column(ui, SECTION_GAP, |ui| {
            row(ui, 0.0, COLUMN_GAP, Align::Min, |ui| {
                cell(
                    ui,
                    vec2(TOAST_WIDTH, 0.0),
                    Layout::top_down(Align::Min),
                    0.0,
                    toasts,
                );
                banners_and_callouts(ui);
            });
            empty_states(ui);
        });
    });
}

#[test]
fn c12_feedback_board() {
    snapshot_board("c12_feedback", c12_scene);
}

/// Toasts and banners report their action and dismiss buttons.
#[test]
fn toast_and_banner_report_their_buttons() {
    #[derive(Default)]
    struct Clicks {
        toast_action: bool,
        toast_dismissed: bool,
        banner_action: bool,
        banner_dismissed: bool,
    }
    let mut harness = Harness::builder()
        .with_size(vec2(720.0, 240.0))
        .build_ui_state(
            |ui, clicks: &mut Clicks| {
                canvas(ui);
                let banner = Banner::new(Tone::Danger, "License check failed.")
                    .action("Retry")
                    .dismissible(true)
                    .show(ui);
                clicks.banner_action |= banner.action;
                clicks.banner_dismissed |= banner.dismissed;
                ui.add_space(space::S16);
                let toast = Toast::new(Tone::Success, "CronJob started")
                    .detail("nightly-report · kube-system")
                    .action("View job")
                    .show(ui);
                clicks.toast_action |= toast.action;
                clicks.toast_dismissed |= toast.dismissed;
            },
            Clicks::default(),
        );
    setup_egui(&mut harness);
    harness.set_size(vec2(720.0, 240.0));
    harness.run();
    harness.ui_harness("feedback/toast_and_banner");

    harness.get_by_label("Retry").click();
    harness.get_by_label("View job").click();
    harness.run();
    let dismisses = harness.get_all_by_label("Dismiss").collect::<Vec<_>>();
    assert_eq!(dismisses.len(), 2);
    for dismiss in dismisses {
        dismiss.click();
    }
    harness.run();
    let clicks = harness.state();
    assert!(clicks.banner_action && clicks.banner_dismissed);
    assert!(clicks.toast_action && clicks.toast_dismissed);
}

/// The empty state's action reports clicks and the state is centred in the
/// area it replaces.
#[test]
fn empty_state_centres_and_reports_its_action() {
    let mut harness = Harness::builder()
        .with_size(vec2(480.0, 320.0))
        .build_ui_state(
            |ui, retried: &mut bool| {
                canvas(ui);
                let response = EmptyState::error("Couldn’t watch pods")
                    .description("Forbidden: user cannot list pods")
                    .action_with_icon(Icon::ArrowPath, "Retry")
                    .show(ui);
                *retried |= response.is_some_and(|response| response.clicked());
            },
            false,
        );
    setup_egui(&mut harness);
    harness.set_size(vec2(480.0, 320.0));
    harness.run();
    harness.ui_harness("feedback/empty_state_error");
    let title = harness.get_by_label("Couldn’t watch pods").rect();
    assert!((title.center().x - 240.0).abs() < 1.0, "{title:?}");
    harness.get_by_label("Retry").click();
    harness.run();
    assert!(*harness.state());
}

/// Paint the window canvas behind a focused snapshot.
fn canvas(ui: &Ui) {
    let theme = ui.tokens();
    ui.ctx()
        .layer_painter(egui::LayerId::background())
        .rect_filled(ui.ctx().content_rect(), 0, theme.surface.canvas);
}

/// Toasts that read the same keep their own state: dismissing one leaves the
/// other, as long as each has its own id.
#[test]
fn toasts_with_the_same_title_dismiss_independently() {
    let mut harness = Harness::builder()
        .with_size(vec2(480.0, 320.0))
        .build_ui_state(
            |ui, dismissed: &mut Vec<usize>| {
                canvas(ui);
                for sequence in 0..2 {
                    let response = Toast::new(Tone::Danger, "Couldn’t scale")
                        .detail("Forbidden")
                        .id_salt(("test-toast", sequence))
                        .show(ui);
                    if response.dismissed {
                        dismissed.push(sequence);
                    }
                }
            },
            Vec::new(),
        );
    setup_egui(&mut harness);
    harness.set_size(vec2(480.0, 320.0));
    harness.run();
    let dismisses = harness.get_all_by_label("Dismiss").collect::<Vec<_>>();
    assert_eq!(dismisses.len(), 2);
    dismisses[1].click();
    harness.run();
    assert_eq!(*harness.state(), [1]);
}

/// An alert callout is announced as an alert named by its title; a plain
/// callout is not.
#[test]
fn alert_callouts_are_alerts_named_by_their_title() {
    use egui_kittest::kittest::NodeT;
    let mut harness = Harness::builder()
        .with_size(vec2(480.0, 240.0))
        .build_ui(|ui| {
            Callout::new(Tone::Danger, "The Kubernetes API rejected this resource")
                .alert()
                .show(ui);
            Callout::new(Tone::Info, "Use {command} as the placeholder").show(ui);
        });
    setup_egui(&mut harness);
    harness.run();
    harness.get_by_role_and_label(
        egui::accesskit::Role::Alert,
        "The Kubernetes API rejected this resource",
    );
    assert!(
        harness
            .query_by_role_and_label(
                egui::accesskit::Role::Alert,
                "Use {command} as the placeholder"
            )
            .is_none()
    );
    let _ = harness
        .get_by_label("Use {command} as the placeholder")
        .accesskit_node();
}
