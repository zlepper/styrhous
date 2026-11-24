//! The cluster switcher (A16, C18) and the facts about the clusters that it
//! and the landing page (A01) show: what each context is, how it is
//! connected and what is known of it.

use super::cluster_roster::ClusterRoster;
use super::sidebar::{connection, contexts};
use super::state::{
    ClusterConnectionState, ClusterState, ManagedClusterImport, OperationOutcome, UiState,
};
use super::widgets::count_noun;
use super::workspace::listed_count;
use crate::api_resource::{ApiResource, CORE_GROUP};
use crate::worker::WorkerCommandBox;
use components::design::theme::Tone;
use components::{
    ClusterAction, ClusterEntry, ClusterSummary, ClusterSwitcher, ClusterSwitcherState, Readout,
    ReadoutTone,
};
use std::collections::HashSet;
use std::time::Duration;

/// How long the toasts about adding or opening a cluster stay.
const TOAST_DURATION: Duration = Duration::from_secs(5);
/// How many clusters get a ⌘1…9 key.
pub(super) const KEYED: usize = 9;
/// What the readout strip shows for a reading the app does not have.
const UNKNOWN: &str = "—";
/// The heading of the clusters discovery found.
const FOUND_TITLE: &str = "Found in cloud accounts";

/// What the open switcher remembers.
#[derive(Debug, Default)]
pub(super) struct SwitcherSession {
    state: ClusterSwitcherState,
    /// Opened this frame: focus the search and highlight the current cluster.
    fresh: bool,
}

/// What choosing a cluster does.
#[derive(Clone, Debug)]
pub(super) enum ChoiceTarget {
    /// A context of the kubeconfig.
    Context { cluster_key: i32, name: String },
    /// A cluster found in a cloud account, to add to the kubeconfig.
    Found(ManagedClusterImport),
}

/// One cluster as the switcher and the landing page list it.
#[derive(Clone, Debug)]
pub(super) struct ClusterChoice {
    pub(super) target: ChoiceTarget,
    pub(super) name: String,
    pub(super) initials: String,
    pub(super) category: usize,
    /// The line under the name: only for a cluster discovery found
    /// ("EKS · NOT IN KUBECONFIG"); a context is its name alone.
    pub(super) meta: Option<String>,
    /// The cloud a found cluster lives in, for the footer's notice.
    cloud: Option<&'static str>,
    pub(super) detail: String,
    pub(super) status: Option<Tone>,
    pub(super) key: Option<String>,
    summary: Option<Summary>,
    is_current: bool,
}

/// The readout strip of a connected cluster.
#[derive(Clone, Debug)]
struct Summary {
    readouts: Vec<(&'static str, String, ReadoutTone)>,
}

impl ClusterChoice {
    /// The choice as the component lists it.
    pub(super) fn entry(&self) -> ClusterEntry<'_> {
        ClusterEntry {
            name: &self.name,
            initials: &self.initials,
            category: self.category,
            meta: self.meta.as_deref(),
            detail: &self.detail,
            status: self.status,
            key: self.key.as_deref(),
            summary: self.summary.as_ref().map(|summary| ClusterSummary {
                readouts: summary
                    .readouts
                    .iter()
                    .map(|(label, value, tone)| Readout::new(label, value).tone(*tone))
                    .collect(),
                // The app does not measure the API server's latency.
                latency: Vec::new(),
            }),
        }
    }
}

/// The ⌘ key label of the `index`th (from 0) quick-switch slot.
pub(super) fn key_label(index: usize) -> Option<String> {
    (index < KEYED).then(|| format!("⌘{}", index + 1))
}

fn nodes_kind(cluster: &ClusterState) -> Option<&ApiResource> {
    kind(cluster, "nodes")
}

fn kind<'a>(cluster: &'a ClusterState, name: &str) -> Option<&'a ApiResource> {
    cluster
        .resource_navigation
        .all_api_resources()
        .find(|api_resource| api_resource.identity() == (CORE_GROUP, name))
}

/// The Nodes the app is watching, if it is.
fn node_count(cluster: &ClusterState) -> Option<usize> {
    listed_count(cluster, nodes_kind(cluster)?)
}

/// Remember how many nodes each connected cluster has, so it can say so
/// while it is not connected.
pub(super) fn note_known_nodes(ui_state: &mut UiState) {
    for cluster in ui_state.clusters.values() {
        if let Some(count) = node_count(cluster) {
            ui_state.cluster_roster.note_nodes(&cluster.name, count);
        }
    }
}

/// The node count the app has seen of the cluster, live or last time.
fn known_nodes(cluster: &ClusterState, roster: &ClusterRoster) -> Option<String> {
    node_count(cluster)
        .or_else(|| roster.known_nodes.get(&cluster.name).copied())
        .map(|count| count_noun(count, "node"))
}

fn detail(cluster: &ClusterState, nodes: Option<String>) -> String {
    match &cluster.connection {
        ClusterConnectionState::Failed(_) => nodes.unwrap_or_else(|| "unreachable".to_owned()),
        ClusterConnectionState::Connecting => "connecting".to_owned(),
        ClusterConnectionState::Connected => nodes.unwrap_or_else(|| "connected".to_owned()),
        ClusterConnectionState::Disconnected => nodes.unwrap_or_else(|| "not connected".to_owned()),
    }
}

/// The readouts of a connected cluster, as far as the app knows them.
fn summary(cluster: &ClusterState) -> Option<Summary> {
    if !matches!(cluster.connection, ClusterConnectionState::Connected) || !cluster.reachable() {
        return None;
    }
    let show = |count: Option<usize>| count.map_or_else(|| UNKNOWN.to_owned(), |c| c.to_string());
    // Pods of every namespace being watched, each counted once.
    let mut pods = HashSet::new();
    let mut failing = 0;
    let mut watching_pods = false;
    for ((api_resource, _), watch) in &cluster.resource_cache {
        if api_resource.identity() != (CORE_GROUP, "pods") {
            continue;
        }
        watching_pods = true;
        for pod in watch.resources.values() {
            if pods.insert(pod.uid.clone()) && pod.problem_tone().is_some() {
                failing += 1;
            }
        }
    }
    let crds = cluster.api_discovery.complete.then(|| {
        cluster
            .resource_navigation
            .other_api_groups
            .values()
            .map(Vec::len)
            .sum()
    });
    let pods = watching_pods.then_some(pods.len());
    Some(Summary {
        readouts: vec![
            ("Nodes", show(node_count(cluster)), ReadoutTone::Neutral),
            ("Pods", show(pods), ReadoutTone::Neutral),
            (
                "Failing",
                show(watching_pods.then_some(failing)),
                if failing > 0 {
                    ReadoutTone::Problem
                } else {
                    ReadoutTone::Neutral
                },
            ),
            (
                "Namespaces",
                show(
                    matches!(
                        cluster.namespaces_load,
                        super::state::ClusterLoadState::Ready
                    )
                    .then_some(cluster.namespaces.len()),
                ),
                ReadoutTone::Neutral,
            ),
            ("CRDs", show(crds), ReadoutTone::Neutral),
            ("Latency", UNKNOWN.to_owned(), ReadoutTone::Neutral),
        ],
    })
}

/// Every context of the kubeconfig in the sidebar's order, ready to list.
/// The pinned ones take the keys ⌘1… in the order they were pinned.
pub(super) fn context_choices(ui_state: &UiState) -> Vec<ClusterChoice> {
    contexts(ui_state)
        .into_iter()
        .enumerate()
        .map(|(position, cluster)| {
            let (status, _) = connection(cluster);
            let name = cluster.presentation.display_name(&cluster.name).to_owned();
            let nodes = known_nodes(cluster, &ui_state.cluster_roster);
            ClusterChoice {
                target: ChoiceTarget::Context {
                    cluster_key: cluster.cluster_key,
                    name: cluster.name.clone(),
                },
                initials: cluster.presentation.badge(&cluster.name),
                category: cluster.presentation.colour.map_or_else(
                    || crate::context_presentation::automatic_colour(position),
                    usize::from,
                ),
                meta: None,
                cloud: None,
                detail: detail(cluster, nodes),
                status: Some(status),
                key: None,
                summary: summary(cluster),
                is_current: ui_state.selected_cluster == Some(cluster.cluster_key),
                name,
            }
        })
        .collect()
}

/// The clusters that discovery found in cloud accounts and the kubeconfig
/// does not have yet. `first_category` continues the contexts' colours.
fn found_choices(ui_state: &UiState, first_category: usize) -> Vec<ClusterChoice> {
    let discovery = &ui_state.managed_cluster_discovery;
    let imports = discovery
        .aws
        .clusters
        .iter()
        .filter(|cluster| !cluster.configured)
        .map(|cluster| {
            (
                cluster.name.clone(),
                ("EKS", "AWS"),
                ManagedClusterImport::Eks {
                    account_id: cluster.account_id.clone(),
                    region: cluster.region.clone(),
                    name: cluster.name.clone(),
                    profile: cluster.profile.clone(),
                },
            )
        })
        .chain(
            discovery
                .aks_clusters
                .iter()
                .filter(|cluster| !cluster.configured)
                .map(|cluster| {
                    (
                        cluster.name.clone(),
                        ("AKS", "Azure"),
                        ManagedClusterImport::Aks {
                            subscription_id: cluster.subscription_id.clone(),
                            resource_group: cluster.resource_group.clone(),
                            cluster_name: cluster.name.clone(),
                        },
                    )
                }),
        )
        .chain(
            discovery
                .tailscale_clusters
                .iter()
                .filter(|cluster| !cluster.configured)
                .map(|cluster| {
                    (
                        cluster.host_name.clone(),
                        ("Tailscale", "Tailscale"),
                        ManagedClusterImport::Tailscale {
                            host_name: cluster.host_name.clone(),
                        },
                    )
                }),
        );
    imports
        .enumerate()
        .map(|(index, (name, (provider, cloud), import))| ClusterChoice {
            target: ChoiceTarget::Found(import),
            initials: crate::context_presentation::automatic_badge(&name),
            category: crate::context_presentation::automatic_colour(first_category + index),
            meta: Some(format!("{} · NOT IN KUBECONFIG", provider.to_uppercase())),
            cloud: Some(cloud),
            detail: "add".to_owned(),
            status: None,
            key: None,
            summary: None,
            is_current: false,
            name,
        })
        .collect()
}

/// The titled groups of the switcher: the pinned clusters, the recent ones,
/// the rest, and what discovery found; while a query is typed one group of
/// the contexts that match it.
fn build_sections(
    ui_state: &UiState,
    mut contexts: Vec<ClusterChoice>,
    query: &str,
) -> Vec<(String, Vec<ClusterChoice>)> {
    let roster = &ui_state.cluster_roster;
    let found = found_choices(ui_state, contexts.len());
    let context_name = |choice: &ClusterChoice| match &choice.target {
        ChoiceTarget::Context { name, .. } => name.clone(),
        ChoiceTarget::Found(_) => String::new(),
    };
    let take = |names: &[String], contexts: &mut Vec<ClusterChoice>| {
        names
            .iter()
            .filter_map(|wanted| {
                let at = contexts
                    .iter()
                    .position(|choice| context_name(choice) == *wanted)?;
                Some(contexts.remove(at))
            })
            .collect::<Vec<_>>()
    };
    let mut pinned = take(&roster.pinned, &mut contexts);
    for (index, choice) in pinned.iter_mut().enumerate() {
        choice.key = key_label(index);
    }
    let recent = take(&roster.recent, &mut contexts);
    let mut sections = Vec::new();
    if query.trim().is_empty() {
        let rest_title = if pinned.is_empty() && recent.is_empty() {
            "All clusters"
        } else {
            "Other clusters"
        };
        sections.push(("Pinned".to_owned(), pinned));
        sections.push(("Recent".to_owned(), recent));
        sections.push((rest_title.to_owned(), contexts));
    } else {
        let matching = pinned.into_iter().chain(recent).chain(contexts).collect();
        sections.push((format!("Matching “{}”", query.trim()), matching));
    }
    sections.push((FOUND_TITLE.to_owned(), found));
    sections.retain(|(_, choices)| !choices.is_empty());
    sections
}

impl UiState {
    /// Open the switcher with the search focused and the current cluster
    /// highlighted.
    pub(super) fn open_cluster_switcher(&mut self) {
        self.cluster_switcher = Some(SwitcherSession {
            fresh: true,
            ..Default::default()
        });
    }

    pub(super) fn toggle_cluster_switcher(&mut self) {
        if self.cluster_switcher.is_some() {
            self.cluster_switcher = None;
        } else {
            self.open_cluster_switcher();
        }
    }

    /// What choosing `choice` does: switch to the context (and remember it
    /// as visited), or start adding the found cluster to the kubeconfig.
    pub(super) fn choose_cluster(
        &mut self,
        choice: &ClusterChoice,
        commands: &mut Vec<WorkerCommandBox>,
    ) {
        match &choice.target {
            ChoiceTarget::Context { cluster_key, .. } => {
                self.switch_cluster(*cluster_key, commands);
            }
            ChoiceTarget::Found(import) => {
                if self.managed_cluster_discovery.importing.is_some() {
                    return;
                }
                self.begin_managed_import(import.clone(), commands);
                self.switch_to_imported = true;
                self.push_application_toast(
                    OperationOutcome::Success,
                    "Adding cluster to your kubeconfig",
                    choice.name.clone(),
                    TOAST_DURATION,
                );
            }
        }
    }

    pub(super) fn toggle_cluster_pin(&mut self, choice: &ClusterChoice) {
        if let ChoiceTarget::Context { name, .. } = &choice.target {
            self.cluster_roster.toggle_pin(name);
        }
    }
}

/// Show the switcher while it is open and apply what it asks for.
pub(super) fn show(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands: &mut Vec<WorkerCommandBox>,
) {
    let Some(mut session) = ui_state.cluster_switcher.take() else {
        return;
    };
    let contexts = context_choices(ui_state);
    let sections = build_sections(ui_state, contexts, &session.state.query);
    if session.fresh {
        // The current cluster is highlighted on open.
        session.state.highlight = sections
            .iter()
            .flat_map(|(_, choices)| choices)
            .position(|choice| choice.is_current)
            .unwrap_or(0);
    }
    let entries: Vec<Vec<ClusterEntry<'_>>> = sections
        .iter()
        .map(|(_, choices)| choices.iter().map(ClusterChoice::entry).collect())
        .collect();
    let notice = found_notice(&sections);
    let mut switcher = ClusterSwitcher::new(&mut session.state).focus_search(session.fresh);
    for ((title, _), entries) in sections.iter().zip(entries) {
        switcher = switcher.section(title, entries);
    }
    if let Some(notice) = &notice {
        switcher = switcher.notice(notice);
    }
    let action = switcher.show_modal(ctx);
    session.fresh = false;
    let chosen = |target: components::ClusterTarget| &sections[target.section].1[target.entry];
    let mut keep_open = true;
    match action {
        Some(ClusterAction::Switch(target)) => {
            ui_state.choose_cluster(chosen(target), commands);
            keep_open = false;
        }
        Some(ClusterAction::OpenInNewWindow(_)) => {
            ui_state.push_application_toast(
                OperationOutcome::Warning,
                "Not yet implemented",
                "Opening a cluster in a new window",
                TOAST_DURATION,
            );
        }
        Some(ClusterAction::TogglePin(target)) => {
            let choice = chosen(target);
            ui_state.toggle_cluster_pin(choice);
            // The tile moves between sections; keep the highlight on it.
            if let ChoiceTarget::Context { name, .. } = &choice.target {
                let moved =
                    build_sections(ui_state, context_choices(ui_state), &session.state.query);
                if let Some(at) = moved.iter().flat_map(|(_, choices)| choices).position(
                    |c| matches!(&c.target, ChoiceTarget::Context { name: n, .. } if n == name),
                ) {
                    session.state.highlight = at;
                }
            }
        }
        Some(ClusterAction::Close) => keep_open = false,
        None => {}
    }
    if keep_open {
        ui_state.cluster_switcher = Some(session);
    }
}

/// "Found 1 new cluster in AWS": the footer's notice when discovery found
/// clusters the kubeconfig does not have.
fn found_notice(sections: &[(String, Vec<ClusterChoice>)]) -> Option<String> {
    let found = sections
        .iter()
        .find(|(title, _)| title == FOUND_TITLE)
        .map(|(_, choices)| choices)?;
    let mut clouds: Vec<&str> = Vec::new();
    for choice in found {
        let Some(cloud) = choice.cloud else {
            continue;
        };
        if !clouds.contains(&cloud) {
            clouds.push(cloud);
        }
    }
    Some(format!(
        "Found {} in {}",
        count_noun(found.len(), "new cluster"),
        clouds.join(", ")
    ))
}
