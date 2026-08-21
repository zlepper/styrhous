use super::*;

#[derive(Debug, Clone)]

pub(crate) struct PendingDelete {
    pub(crate) api_resource: ApiResource,
    pub(crate) resource_name: String,
    pub(crate) namespace: Option<String>,
    pub(crate) confirmation_available_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct BulkDeleteTarget {
    pub(crate) uid: String,
    pub(crate) name: String,
    pub(crate) namespace: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingBulkDelete {
    pub(crate) api_resource: ApiResource,
    pub(crate) targets: Vec<BulkDeleteTarget>,
    pub(crate) confirmation_available_at: Instant,
}

impl PendingBulkDelete {
    pub(crate) fn new(api_resource: ApiResource, targets: Vec<BulkDeleteTarget>) -> Self {
        Self {
            api_resource,
            targets,
            confirmation_available_at: Instant::now() + DELETE_CONFIRMATION_DELAY,
        }
    }
}

#[derive(Debug)]
pub(crate) struct BulkDeleteProgress {
    pub(crate) id: u64,
    pub(crate) api_resource: ApiResource,
    pub(crate) remaining_targets: HashSet<BulkDeleteTarget>,
    pub(crate) failures: Vec<(BulkDeleteTarget, String)>,
}

impl BulkDeleteProgress {
    pub(crate) fn new(id: u64, api_resource: ApiResource, targets: Vec<BulkDeleteTarget>) -> Self {
        Self {
            id,
            api_resource,
            remaining_targets: targets.into_iter().collect(),
            failures: Vec::new(),
        }
    }

    pub(crate) fn target_for(
        &self,
        api_resource: &ApiResource,
        name: &str,
        namespace: &Option<String>,
    ) -> Option<BulkDeleteTarget> {
        if self.api_resource != *api_resource {
            return None;
        }
        self.remaining_targets
            .iter()
            .find(|target| target.name == name && target.namespace == *namespace)
            .cloned()
    }
}

impl BulkDeleteTarget {
    pub(crate) fn display_name(&self) -> String {
        self.namespace.as_deref().map_or_else(
            || self.name.clone(),
            |namespace| format!("{namespace}/{}", self.name),
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PendingForceDelete {
    pub(crate) api_resource: ApiResource,
    pub(crate) resource_name: String,
    pub(crate) resource_uid: String,
    pub(crate) namespace: Option<String>,
    pub(crate) finalizers: Vec<String>,
    pub(crate) acknowledgement: String,
    pub(crate) confirmation_available_at: Instant,
}

impl PendingForceDelete {
    pub(crate) fn new(
        api_resource: ApiResource,
        resource_name: String,
        resource_uid: String,
        namespace: Option<String>,
        finalizers: Vec<String>,
    ) -> Self {
        Self {
            api_resource,
            resource_name,
            resource_uid,
            namespace,
            finalizers,
            acknowledgement: String::new(),
            confirmation_available_at: Instant::now() + DELETE_CONFIRMATION_DELAY,
        }
    }
}

impl PendingDelete {
    pub(crate) fn new(
        api_resource: ApiResource,
        resource_name: String,
        namespace: Option<String>,
    ) -> Self {
        Self {
            api_resource,
            resource_name,
            namespace,
            confirmation_available_at: Instant::now() + DELETE_CONFIRMATION_DELAY,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PendingDeploymentRestart {
    pub(crate) resource_name: String,
    pub(crate) namespace: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingCronJobRun {
    pub(crate) resource_name: String,
    pub(crate) namespace: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingScale {
    pub(crate) api_resource: ApiResource,
    pub(crate) resource_name: String,
    pub(crate) namespace: Option<String>,
    pub(crate) current_replicas: i32,
    pub(crate) desired_replicas: String,
}
