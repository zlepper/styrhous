use super::*;

impl UiState {
    pub(crate) fn record_operation_outcome(
        &mut self,
        cluster_key: i32,
        outcome: OperationOutcome,
        title: impl Into<String>,
        namespace: Option<&str>,
        resource_name: impl Into<String>,
        details: Option<String>,
    ) {
        let Some(cluster) = self.clusters.get(&cluster_key) else {
            return;
        };
        // A reconnect replaces the resource state before its lifecycle command
        // is dispatched. The worker deliberately drains older mutations first,
        // so their terminal results must not leak into the new session.
        if !matches!(cluster.connection, ClusterConnectionState::Connected) {
            return;
        }
        let sequence = self.next_operation_toast_sequence;
        self.next_operation_toast_sequence += 1;
        let cluster = self
            .clusters
            .get_mut(&cluster_key)
            .expect("cluster connection was just verified");
        let resource_name = resource_name.into();
        let target = namespace.map_or(resource_name.clone(), |namespace| {
            format!("{resource_name} • {namespace}")
        });
        let title = title.into();
        let occurred_at = Instant::now();
        cluster.operation_history.push(OperationHistoryEntry {
            outcome,
            title: title.clone(),
            target: target.clone(),
            details,
            occurred_at,
            sequence,
            unread: true,
        });
        cluster
            .transient_operation_toasts
            .push(TransientOperationToast {
                outcome,
                title,
                target,
                sequence,
                expires_at: occurred_at + Duration::from_secs(5),
            });
    }

    pub(crate) fn safe_operation_detail(api_resource: &ApiResource, error: String) -> String {
        if api_resource.kind.eq_ignore_ascii_case("secret") {
            "Details are hidden for Secret operations. Check the resource state and your Kubernetes permissions.".to_owned()
        } else {
            error
        }
    }

    pub(crate) fn record_resource_operation_failure(
        &mut self,
        cluster_key: i32,
        api_resource: &ApiResource,
        title: impl Into<String>,
        namespace: Option<&str>,
        resource_name: impl Into<String>,
        error: String,
    ) -> String {
        let details = Self::safe_operation_detail(api_resource, error);
        self.record_operation_outcome(
            cluster_key,
            OperationOutcome::Failure,
            title,
            namespace,
            resource_name,
            Some(details.clone()),
        );
        details
    }

    pub(crate) fn settle_cron_job_run(&mut self, cluster_key: i32, operation_id: u64) -> bool {
        self.clusters
            .get_mut(&cluster_key)
            .is_some_and(|cluster| cluster.cron_job_runs_in_flight.remove(&operation_id))
    }

    pub(crate) fn unread_operation_count(&self) -> usize {
        self.clusters
            .values()
            .flat_map(|cluster| &cluster.operation_history)
            .filter(|entry| entry.unread)
            .count()
    }

    pub(crate) fn mark_operation_history_read(&mut self) {
        for entry in self
            .clusters
            .values_mut()
            .flat_map(|cluster| &mut cluster.operation_history)
        {
            entry.unread = false;
        }
    }

    pub(crate) fn visible_operation_toasts(
        &mut self,
        now: Instant,
    ) -> Vec<TransientOperationToast> {
        let mut toasts = self
            .clusters
            .values_mut()
            .flat_map(|cluster| {
                cluster
                    .transient_operation_toasts
                    .retain(|toast| toast.expires_at > now);
                cluster.transient_operation_toasts.iter().cloned()
            })
            .collect::<Vec<_>>();
        toasts.sort_by_key(|toast| toast.sequence);
        const MAX_VISIBLE_TOASTS: usize = 3;
        let first_visible = toasts.len().saturating_sub(MAX_VISIBLE_TOASTS);
        toasts.drain(..first_visible);
        toasts
    }

    pub(crate) fn clear_operation_toasts(&mut self) {
        for cluster in self.clusters.values_mut() {
            cluster.transient_operation_toasts.clear();
        }
    }

    pub(crate) fn operation_history_entries(&self) -> Vec<OperationHistoryDisplayEntry> {
        let mut entries = self
            .clusters
            .values()
            .flat_map(|cluster| {
                cluster.operation_history.iter().cloned().map(|entry| {
                    OperationHistoryDisplayEntry {
                        cluster_name: cluster.name.clone(),
                        entry,
                    }
                })
            })
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            right
                .entry
                .occurred_at
                .cmp(&left.entry.occurred_at)
                .then_with(|| right.entry.sequence.cmp(&left.entry.sequence))
        });
        entries
    }
}

#[derive(Debug, Clone)]
pub(crate) struct OperationHistoryDisplayEntry {
    pub(crate) cluster_name: String,
    pub(crate) entry: OperationHistoryEntry,
}
