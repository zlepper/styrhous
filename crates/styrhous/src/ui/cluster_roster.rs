//! The clusters the user keeps close: the ones pinned in the cluster
//! switcher (C18), the ones visited lately, and the node count last seen of
//! each, so a cluster that is not connected can still say how big it is.
//! Contexts are named, never keyed, because the keys change whenever the
//! kubeconfig is read again.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// How many visited contexts the switcher remembers.
const RECENT_LIMIT: usize = 8;

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct ClusterRoster {
    /// The pinned contexts in the order they were pinned (⌘1 is the first).
    #[serde(default)]
    pub(super) pinned: Vec<String>,
    /// The visited contexts, the latest first.
    #[serde(default)]
    pub(super) recent: Vec<String>,
    /// The node count each context had when it was last seen.
    #[serde(default)]
    pub(super) known_nodes: BTreeMap<String, usize>,
}

impl ClusterRoster {
    pub(super) fn is_pinned(&self, context: &str) -> bool {
        self.pinned.iter().any(|pinned| pinned == context)
    }

    /// Pin `context` after the pinned ones, or unpin it.
    pub(super) fn toggle_pin(&mut self, context: &str) {
        if self.is_pinned(context) {
            self.pinned.retain(|pinned| pinned != context);
        } else {
            self.pinned.push(context.to_owned());
        }
    }

    /// Remember that `context` was just switched to.
    pub(super) fn visit(&mut self, context: &str) {
        self.recent.retain(|recent| recent != context);
        self.recent.insert(0, context.to_owned());
        self.recent.truncate(RECENT_LIMIT);
    }

    /// Remember how many nodes `context` has.
    pub(super) fn note_nodes(&mut self, context: &str, nodes: usize) {
        if self.known_nodes.get(context) != Some(&nodes) {
            self.known_nodes.insert(context.to_owned(), nodes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_keep_their_order_and_toggle() {
        let mut roster = ClusterRoster::default();
        roster.toggle_pin("kind-kind");
        roster.toggle_pin("prod-eu");
        assert_eq!(roster.pinned, ["kind-kind", "prod-eu"]);
        roster.toggle_pin("kind-kind");
        assert_eq!(roster.pinned, ["prod-eu"]);
        assert!(roster.is_pinned("prod-eu"));
        assert!(!roster.is_pinned("kind-kind"));
    }

    #[test]
    fn visits_list_the_latest_first_without_repeats_and_are_capped() {
        let mut roster = ClusterRoster::default();
        for context in ["a", "b", "a"] {
            roster.visit(context);
        }
        assert_eq!(roster.recent, ["a", "b"]);
        for index in 0..RECENT_LIMIT + 3 {
            roster.visit(&format!("context-{index}"));
        }
        assert_eq!(roster.recent.len(), RECENT_LIMIT);
        assert_eq!(roster.recent[0], format!("context-{}", RECENT_LIMIT + 2));
    }
}
