use super::*;
use crate::ui::global_blade::resource_detail_entry_ids;
use crate::worker::GetResourceScale;

/// The part of the view a scale lookup was requested from. A reply is only
/// useful while the user is still looking at it; otherwise the dialog would
/// pop up over unrelated work (or on another cluster).
#[derive(Debug, Clone, PartialEq, Eq)]
struct ScaleLookupView {
    selected_cluster: Option<i32>,
    selected_api_resource: Option<ApiResource>,
    selected_namespaces: HashSet<String>,
    resource_detail_entry_ids: Vec<u64>,
}

#[derive(Debug)]
pub(crate) struct AwaitedScaleLookup {
    request_id: u64,
    cluster_key: i32,
    view: ScaleLookupView,
}

impl UiState {
    fn scale_lookup_view(
        &self,
        cluster_key: i32,
        resource_detail_entry_ids: Vec<u64>,
    ) -> ScaleLookupView {
        let cluster = self.clusters.get(&cluster_key);
        ScaleLookupView {
            selected_cluster: self.selected_cluster,
            selected_api_resource: cluster
                .and_then(|cluster| cluster.selected_api_resource.clone()),
            selected_namespaces: cluster
                .map(|cluster| cluster.selected_namespaces.clone())
                .unwrap_or_default(),
            resource_detail_entry_ids,
        }
    }

    fn current_scale_lookup_view(&self, cluster_key: i32) -> ScaleLookupView {
        self.scale_lookup_view(
            cluster_key,
            resource_detail_entry_ids(self.global_blades.navigator()),
        )
    }

    /// The only way to build a [`GetResourceScale`]: it registers the lookup as
    /// the single one allowed to open the scale dialog, superseding any earlier one.
    ///
    /// Callers that run while the blade coordinator is moved out of `self` (the
    /// inspector effects) must pass the inspectors they are operating on, since
    /// `self.global_blades` is empty then.
    pub(crate) fn request_resource_scale(
        &mut self,
        resource_detail_entry_ids: Vec<u64>,
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_name: String,
    ) -> GetResourceScale {
        self.next_scale_request_id = self.next_scale_request_id.wrapping_add(1);
        let request_id = self.next_scale_request_id;
        self.awaited_scale_lookup = Some(AwaitedScaleLookup {
            request_id,
            cluster_key,
            view: self.scale_lookup_view(cluster_key, resource_detail_entry_ids),
        });
        GetResourceScale {
            cluster_key,
            request_id,
            api_resource,
            namespace,
            resource_name,
        }
    }

    /// Consumes the lookup if `request_id` is the awaited one, reporting whether
    /// the user is still in the view it was requested from.
    pub(crate) fn settle_scale_lookup(&mut self, cluster_key: i32, request_id: u64) -> bool {
        if self.awaited_scale_lookup.as_ref().is_none_or(|lookup| {
            lookup.request_id != request_id || lookup.cluster_key != cluster_key
        }) {
            return false;
        }
        let lookup = self
            .awaited_scale_lookup
            .take()
            .expect("awaited lookup was checked above");
        lookup.view == self.current_scale_lookup_view(cluster_key)
    }
}
