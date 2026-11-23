use super::*;
use crate::worker::PodLogStreamTarget;

#[derive(Debug)]

pub(crate) enum ResourceAction {
    OpenDetails {
        name: String,
        namespace: Option<String>,
        uid: String,
    },
    EditYaml {
        name: String,
        namespace: Option<String>,
    },
    RequestDelete {
        name: String,
        namespace: Option<String>,
    },
    RequestForceDelete {
        name: String,
        uid: String,
        namespace: Option<String>,
        finalizers: Vec<String>,
    },
    RequestDeploymentRestart {
        name: String,
        namespace: String,
    },
    RequestDeploymentRollback {
        name: String,
        namespace: String,
    },
    RequestCronJobRun {
        name: String,
        namespace: String,
    },
    RequestScale {
        name: String,
        namespace: Option<String>,
    },
    SaveData {
        expected_values: BTreeMap<String, String>,
        updated_values: BTreeMap<String, String>,
    },
    ViewLogs {
        targets: Vec<PodLogStreamTarget>,
    },
    Shell {
        name: String,
        namespace: Option<String>,
        container: PodLogContainer,
    },
    PodDebugShell {
        name: String,
        namespace: Option<String>,
        target_container: String,
        preset: DebugImagePreset,
    },
    NodeShell {
        name: String,
        preset: DebugImagePreset,
    },
    /// Open the terminal settings, where the debug image presets live.
    ConfigureDebugImages,
    NavigateDetails {
        api_resource: ApiResource,
        name: String,
        namespace: Option<String>,
        uid: String,
    },
}

impl ResourceAction {
    /// Open `resource`'s inspector.
    pub(crate) fn open_details(resource: &crate::minimal_resource::MinimalResource) -> Self {
        Self::OpenDetails {
            name: resource.name.clone(),
            namespace: resource.namespace.clone(),
            uid: resource.uid.clone(),
        }
    }

    pub(crate) fn shell_request(&self, kube_context: &str) -> Option<ShellRequest> {
        match self {
            Self::Shell {
                name,
                namespace: Some(namespace),
                container,
            } => Some(ShellRequest::Pod {
                kube_context: kube_context.to_owned(),
                namespace: namespace.clone(),
                pod_name: name.clone(),
                container: container.name.clone(),
            }),
            Self::NodeShell { name, preset } => Some(ShellRequest::Node {
                kube_context: kube_context.to_owned(),
                node_name: name.clone(),
                preset: preset.clone(),
            }),
            Self::PodDebugShell {
                name,
                namespace: Some(namespace),
                target_container,
                preset,
            } => Some(ShellRequest::PodDebug {
                kube_context: kube_context.to_owned(),
                namespace: namespace.clone(),
                pod_name: name.clone(),
                target_container: target_container.clone(),
                preset: preset.clone(),
            }),
            Self::Shell {
                namespace: None, ..
            }
            | Self::PodDebugShell {
                namespace: None, ..
            }
            | Self::OpenDetails { .. }
            | Self::EditYaml { .. }
            | Self::RequestDelete { .. }
            | Self::RequestForceDelete { .. }
            | Self::RequestDeploymentRestart { .. }
            | Self::RequestDeploymentRollback { .. }
            | Self::RequestCronJobRun { .. }
            | Self::RequestScale { .. }
            | Self::SaveData { .. }
            | Self::ViewLogs { .. }
            | Self::NavigateDetails { .. }
            | Self::ConfigureDebugImages => None,
        }
    }
}
