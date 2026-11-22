//! The API resources a cluster session knows about while discovery is still answering.

use super::*;
use crate::api_resource::{ApiResourceSet, canonical_group};

/// What this session's discovery has reported (`fresh`), layered over the resources
/// assumed until it answers (`provisional`), so the workspace is usable at once.
///
/// Once discovery completes, provisional resources it did not confirm are dropped, except
/// those in groups whose discovery failed: they may well still exist.
#[derive(Debug, Default)]
pub(crate) struct ApiDiscoveryState {
    pub(crate) provisional: ApiResourceSet,
    pub(crate) fresh: ApiResourceSet,
    pub(crate) complete: bool,
    /// Names of the groups whose discovery failed and is being retried.
    pub(crate) failed_groups: BTreeSet<String>,
    /// The group list failed, so whether any group other than the core one exists is
    /// unknown until it is listed again.
    pub(crate) group_list_failed: bool,
}

impl ApiDiscoveryState {
    /// Discovery that has not answered yet, assuming `resources` meanwhile: the last
    /// discovery of the context, or the built-in resources.
    pub(crate) fn provisional(resources: ApiResourceSet) -> Self {
        Self {
            provisional: resources,
            ..Self::default()
        }
    }

    /// Records the groups that failed, given as the group versions discovery reports.
    pub(crate) fn set_failed_group_versions<'a>(
        &mut self,
        group_versions: impl IntoIterator<Item = &'a str>,
    ) {
        self.failed_groups = group_versions
            .into_iter()
            .map(|group_version| {
                group_version
                    .split_once('/')
                    .map_or(group_version, |(group, _)| group)
                    .to_owned()
            })
            .collect();
    }

    /// Whether discovery has decided if the Metrics API exists: it completed and listed the
    /// API groups. A failing `metrics.k8s.io` counts as decided: it is there but unavailable.
    pub(crate) fn metrics_decided(&self) -> bool {
        self.complete && !self.group_list_failed
    }

    /// Whether a resource of `group` that discovery has not reported is known not to exist.
    pub(crate) fn absence_is_final(&self, group: &str) -> bool {
        let group = canonical_group(group);
        let unknown =
            self.failed_groups.contains(group) || (self.group_list_failed && group != "core");
        self.complete && !unknown
    }

    /// The resources to show: every fresh resource, then the provisional ones discovery has
    /// neither superseded nor ruled out. A fresh resource supersedes a provisional one with
    /// the same group and plural, whatever their versions.
    pub(crate) fn displayed(&self) -> ApiResourceSet {
        let mut displayed = ApiResourceSet::default();
        let mut shown = BTreeSet::new();
        let fresh = self
            .fresh
            .api_resources
            .iter()
            .map(|resource| (resource, &self.fresh));
        let provisional = self
            .provisional
            .api_resources
            .iter()
            .filter(|resource| !self.absence_is_final(&resource.group))
            .map(|resource| (resource, &self.provisional));
        for (resource, source) in fresh.chain(provisional) {
            if shown.insert(resource.identity()) {
                displayed.push(resource.clone(), source.is_scalable(resource));
            }
        }
        displayed
    }

    /// Whether discovery has reported a Metrics API resource matching `is_metrics`. Only
    /// fresh resources count: a cached Metrics API may since have been removed.
    pub(crate) fn has_fresh(&self, is_metrics: impl Fn(&ApiResource) -> bool) -> bool {
        self.fresh.api_resources.iter().any(is_metrics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource(group: &str, version: &str, name: &str) -> ApiResource {
        ApiResource {
            group: group.to_owned(),
            version: version.to_owned(),
            kind: name.to_owned(),
            name: name.to_owned(),
            namespaced: true,
        }
    }

    fn set(resources: &[ApiResource], scalable: &[ApiResource]) -> ApiResourceSet {
        ApiResourceSet {
            api_resources: resources.to_vec(),
            scalable_api_resources: scalable.iter().cloned().collect(),
        }
    }

    fn names(resources: &ApiResourceSet) -> Vec<String> {
        resources
            .api_resources
            .iter()
            .map(|resource| format!("{}/{}/{}", resource.group, resource.version, resource.name))
            .collect()
    }

    #[test]
    fn fresh_resources_supersede_provisional_ones_of_any_version() {
        let old_hpa = resource("autoscaling", "v1", "horizontalpodautoscalers");
        let new_hpa = resource("autoscaling", "v2", "horizontalpodautoscalers");
        let deployments = resource("apps", "v1", "deployments");
        let mut discovery = ApiDiscoveryState::provisional(set(
            &[old_hpa.clone(), deployments.clone()],
            std::slice::from_ref(&deployments),
        ));
        discovery.fresh = set(std::slice::from_ref(&new_hpa), &[]);

        let displayed = discovery.displayed();

        assert_eq!(
            names(&displayed),
            vec![
                "autoscaling/v2/horizontalpodautoscalers",
                "apps/v1/deployments"
            ]
        );
        assert!(
            displayed.is_scalable(&deployments),
            "provisional scale support is kept"
        );
    }

    #[test]
    fn completed_discovery_keeps_only_unconfirmed_resources_of_failed_groups() {
        let mut discovery = ApiDiscoveryState::provisional(set(
            &[
                resource("core", "v1", "pods"),
                resource("removed.example.com", "v1", "gadgets"),
                resource("flaky.example.com", "v1", "widgets"),
            ],
            &[],
        ));
        discovery.fresh = set(&[resource("core", "v1", "pods")], &[]);
        discovery.complete = true;
        discovery.set_failed_group_versions(["flaky.example.com/v1"]);

        assert_eq!(
            names(&discovery.displayed()),
            vec!["core/v1/pods", "flaky.example.com/v1/widgets"]
        );
        assert!(discovery.absence_is_final("removed.example.com"));
        assert!(!discovery.absence_is_final("flaky.example.com"));
    }
}
