use super::*;

pub(crate) fn detail_payload(object: &kube::api::DynamicObject) -> Option<ResourceDetailPayload> {
    let pod =
        k8s_openapi::serde_json::from_value::<Pod>(k8s_openapi::serde_json::to_value(object).ok()?)
            .ok()?;
    let status = pod.status.as_ref();
    let containers = pod
        .spec
        .as_ref()
        .map(|spec| &spec.containers)
        .map(|containers| {
            containers
                .iter()
                .map(|container| {
                    let status = status
                        .and_then(|status| status.container_statuses.as_ref())
                        .and_then(|statuses| {
                            statuses.iter().find(|entry| entry.name == container.name)
                        });
                    let (state, reason, message) = status
                        .map(container_detail_state)
                        .unwrap_or_else(|| ("Unknown".to_owned(), None, None));
                    PodContainerDetail {
                        name: container.name.clone(),
                        image: container.image.clone().unwrap_or_else(|| "-".to_owned()),
                        ready: status.is_some_and(|status| status.ready),
                        restart_count: status.map_or(0, |status| status.restart_count),
                        state,
                        reason,
                        message,
                        command: container.command.clone().unwrap_or_default(),
                        args: container.args.clone().unwrap_or_default(),
                        ports: container
                            .ports
                            .as_deref()
                            .unwrap_or_default()
                            .iter()
                            .map(|port| {
                                format!(
                                    "{}/{}",
                                    port.container_port,
                                    port.protocol.as_deref().unwrap_or("TCP")
                                )
                            })
                            .collect(),
                        environment_variables: pod_environment_variables(container, &pod),
                        resource_requests: resource_thresholds(
                            container
                                .resources
                                .as_ref()
                                .and_then(|resources| resources.requests.as_ref()),
                        ),
                        resource_limits: resource_thresholds(
                            container
                                .resources
                                .as_ref()
                                .and_then(|resources| resources.limits.as_ref()),
                        ),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let conditions = status
        .and_then(|status| status.conditions.as_ref())
        .map(|conditions| {
            conditions
                .iter()
                .map(|condition| PodConditionDetail {
                    type_: condition.type_.clone(),
                    status: condition.status.clone(),
                    reason: condition.reason.clone(),
                    message: condition.message.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    let volumes = pod
        .spec
        .as_ref()
        .and_then(|spec| spec.volumes.as_ref())
        .map_or_else(Vec::new, |volumes| {
            volumes
                .iter()
                .map(|volume| pod_volume_detail(volume, &pod))
                .collect()
        });

    Some(ResourceDetailPayload::Pod(Box::new(PodDetail {
        phase: status
            .and_then(|status| status.phase.clone())
            .unwrap_or_else(|| "Unknown".to_owned()),
        conditions,
        node_name: pod.spec.as_ref().and_then(|spec| spec.node_name.clone()),
        pod_ip: status.and_then(|status| status.pod_ip.clone()),
        host_ip: status.and_then(|status| status.host_ip.clone()),
        qos_class: status.and_then(|status| status.qos_class.clone()),
        restart_policy: pod
            .spec
            .as_ref()
            .and_then(|spec| spec.restart_policy.clone()),
        service_account_name: pod
            .spec
            .as_ref()
            .and_then(|spec| spec.service_account_name.clone()),
        dns_policy: pod.spec.as_ref().and_then(|spec| spec.dns_policy.clone()),
        containers,
        log_containers: pod_log_containers(&pod),
        volumes,
    })))
}
