use super::*;

pub(crate) fn is_managed_workload_child(parent: &ApiResource, child: &ApiResource) -> bool {
    matches!(
        (
            parent.group.as_str(),
            parent.kind.as_str(),
            child.group.as_str(),
            child.kind.as_str(),
        ),
        ("apps", "Deployment", "apps", "ReplicaSet")
            | ("batch", "CronJob", "batch", "Job")
            | ("apps", "ReplicaSet", "core", "Pod")
            | ("apps", "StatefulSet", "core", "Pod")
            | ("apps", "DaemonSet", "core", "Pod")
            | ("core", "ReplicationController", "core", "Pod")
            | ("batch", "Job", "core", "Pod")
    )
}

pub(crate) async fn watch_detail_object(
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
    history_entry_id: u64,
    event_sender: WorkerResultSender,
) {
    let api = match dynamic_api::create(&client, &api_resource, namespace.as_deref()).await {
        Ok(api) => api,
        Err(error) => {
            send_detail_error(&event_sender, cluster_key, history_entry_id, false, error).await;
            return;
        }
    };
    let config = watcher_config().fields(&format!("metadata.name={resource_name}"));
    let stream = watcher(api, config);
    pin_mut!(stream);
    let mut found_during_initial_list = false;
    while let Some(event) = stream.next().await {
        let event = match event {
            Ok(event) => event,
            Err(error) => {
                send_detail_error(&event_sender, cluster_key, history_entry_id, false, error).await;
                return;
            }
        };
        match event {
            Event::Apply(object) => {
                event_sender
                    .send(ResourceDetailUpdated {
                        cluster_key,
                        history_entry_id,
                        detail: Box::new(
                            resource_detail_from_dynamic(&client, api_resource.clone(), object)
                                .await,
                        ),
                    })
                    .await
                    .log_if_error("Failed to send resource detail update");
            }
            Event::InitApply(object) => {
                found_during_initial_list = true;
                event_sender
                    .send(ResourceDetailUpdated {
                        cluster_key,
                        history_entry_id,
                        detail: Box::new(
                            resource_detail_from_dynamic(&client, api_resource.clone(), object)
                                .await,
                        ),
                    })
                    .await
                    .log_if_error("Failed to send resource detail update");
            }
            Event::Delete(_) => event_sender
                .send(ResourceDetailDeleted {
                    cluster_key,
                    history_entry_id,
                })
                .await
                .log_if_error("Failed to send resource detail deletion"),
            Event::Init => found_during_initial_list = false,
            Event::InitDone if !found_during_initial_list => event_sender
                .send(ResourceDetailDeleted {
                    cluster_key,
                    history_entry_id,
                })
                .await
                .log_if_error("Failed to send missing resource detail deletion"),
            Event::InitDone => {}
        }
    }
}

pub(crate) async fn watch_detail_events(
    cluster_key: i32,
    client: kube::Client,
    namespace: Option<String>,
    resource_uid: String,
    history_entry_id: u64,
    event_sender: WorkerResultSender,
) {
    let api: Api<KubernetesEvent> = match namespace.as_deref() {
        Some(namespace) => Api::namespaced(client, namespace),
        None => Api::all(client),
    };
    let config = watcher_config().fields(&format!("involvedObject.uid={resource_uid}"));
    let stream = watcher(api, config);
    pin_mut!(stream);
    let mut events = BTreeMap::new();
    while let Some(event) = stream.next().await {
        let event = match event {
            Ok(event) => event,
            Err(error) => {
                send_detail_error(&event_sender, cluster_key, history_entry_id, true, error).await;
                return;
            }
        };
        match event {
            Event::Init => events.clear(),
            Event::InitApply(event) | Event::Apply(event) => {
                events.insert(
                    get_resource_uid(&event),
                    resource_event_from_kubernetes(event),
                );
            }
            Event::Delete(event) => {
                events.remove(&get_resource_uid(&event));
            }
            Event::InitDone => {}
        }
        send_detail_events(&event_sender, cluster_key, history_entry_id, &events).await;
    }
}

pub(crate) async fn send_detail_events(
    event_sender: &WorkerResultSender,
    cluster_key: i32,
    history_entry_id: u64,
    events: &BTreeMap<String, ResourceEvent>,
) {
    let mut events = events.values().cloned().collect::<Vec<_>>();
    events.sort_by_key(|event| std::cmp::Reverse(event.last_timestamp));
    event_sender
        .send(ResourceEventsReplaced {
            cluster_key,
            history_entry_id,
            events,
        })
        .await
        .log_if_error("Failed to send resource event update");
}

pub(crate) async fn send_detail_error(
    event_sender: &WorkerResultSender,
    cluster_key: i32,
    history_entry_id: u64,
    events: bool,
    error: impl std::fmt::Debug,
) {
    event_sender
        .send(ResourceDetailWatchFailed {
            cluster_key,
            history_entry_id,
            events,
            error: format!("{error:#?}"),
        })
        .await
        .log_if_error("Failed to send resource detail watch failure");
}

pub(crate) async fn resource_detail_from_dynamic(
    client: &kube::Client,
    api_resource: ApiResource,
    object: DynamicObject,
) -> ResourceDetail {
    let metadata = &object.metadata;
    let creation_timestamp = metadata.creation_timestamp.as_ref().and_then(|timestamp| {
        OffsetDateTime::parse(
            &timestamp.0.to_string(),
            &time::format_description::well_known::Rfc3339,
        )
        .ok()
    });
    let mut detail = ResourceDetail {
        api_resource: api_resource.clone(),
        name: metadata.name.clone().unwrap_or_default(),
        namespace: metadata.namespace.clone(),
        uid: get_resource_uid(&object),
        resource_version: metadata.resource_version.clone().unwrap_or_default(),
        is_deleting: metadata.deletion_timestamp.is_some(),
        finalizers: metadata.finalizers.clone().unwrap_or_default(),
        creation_timestamp,
        owners: resource_owners(metadata),
        labels: metadata.labels.clone().unwrap_or_default(),
        annotations: metadata.annotations.clone().unwrap_or_default(),
        payload: resource_handlers::detail_payload(&api_resource, &object),
    };
    if let (Some(namespace), ResourceDetailPayload::Pod(pod)) =
        (detail.namespace.as_deref(), &mut detail.payload)
    {
        resolve_pod_environment_variables(client, namespace, pod).await;
    }
    detail
}

pub(crate) async fn resolve_pod_environment_variables(
    client: &kube::Client,
    namespace: &str,
    pod: &mut crate::resource_detail::PodDetail,
) {
    let mut config_map_names = BTreeSet::new();
    let mut secret_names = BTreeSet::new();
    for container in &pod.containers {
        for variable in &container.environment_variables {
            match &variable.source {
                PodEnvironmentVariableSource::ConfigMapKey { name, .. }
                | PodEnvironmentVariableSource::ConfigMapImport { name, .. } => {
                    config_map_names.insert(name.clone());
                }
                PodEnvironmentVariableSource::SecretKey { name, .. }
                | PodEnvironmentVariableSource::SecretImport { name, .. } => {
                    secret_names.insert(name.clone());
                }
                _ => {}
            }
        }
    }

    let config_maps = fetch_config_maps(client, namespace, config_map_names).await;
    let secrets = fetch_secrets(client, namespace, secret_names).await;
    for container in &mut pod.containers {
        let variables = std::mem::take(&mut container.environment_variables);
        container.environment_variables =
            resolve_environment_variables(variables, &config_maps, &secrets);
    }
}

pub(crate) fn resolve_environment_variables(
    variables: Vec<PodEnvironmentVariableDetail>,
    config_maps: &BTreeMap<String, ConfigMap>,
    secrets: &BTreeMap<String, Secret>,
) -> Vec<PodEnvironmentVariableDetail> {
    let mut variables = variables
        .into_iter()
        .flat_map(|variable| resolve_environment_variable(variable, config_maps, secrets))
        .collect::<Vec<_>>();
    expand_environment_variable_references(&mut variables);
    retain_effective_environment_variables(variables)
}

fn retain_effective_environment_variables(
    variables: Vec<PodEnvironmentVariableDetail>,
) -> Vec<PodEnvironmentVariableDetail> {
    let mut names = BTreeSet::new();
    let mut effective = variables
        .into_iter()
        .rev()
        .filter(|variable| names.insert(variable.name.clone()))
        .collect::<Vec<_>>();
    effective.reverse();
    effective
}

pub(crate) async fn fetch_config_maps(
    client: &kube::Client,
    namespace: &str,
    names: BTreeSet<String>,
) -> BTreeMap<String, ConfigMap> {
    let api = Api::<ConfigMap>::namespaced(client.clone(), namespace);
    let mut config_maps = BTreeMap::new();
    for name in names {
        if let Ok(Some(config_map)) = api.get_opt(&name).await {
            config_maps.insert(name, config_map);
        }
    }
    config_maps
}

pub(crate) async fn fetch_secrets(
    client: &kube::Client,
    namespace: &str,
    names: BTreeSet<String>,
) -> BTreeMap<String, Secret> {
    let api = Api::<Secret>::namespaced(client.clone(), namespace);
    let mut secrets = BTreeMap::new();
    for name in names {
        if let Ok(Some(secret)) = api.get_opt(&name).await {
            secrets.insert(name, secret);
        }
    }
    secrets
}

pub(crate) fn resolve_environment_variable(
    mut variable: PodEnvironmentVariableDetail,
    config_maps: &BTreeMap<String, ConfigMap>,
    secrets: &BTreeMap<String, Secret>,
) -> Vec<PodEnvironmentVariableDetail> {
    match &variable.source {
        PodEnvironmentVariableSource::ConfigMapKey { name, key, .. } => {
            variable.value = config_map_value(config_maps.get(name), key);
            vec![variable]
        }
        PodEnvironmentVariableSource::SecretKey { name, key, .. } => {
            variable.value = secret_value(secrets.get(name), key);
            vec![variable]
        }
        PodEnvironmentVariableSource::ConfigMapImport {
            name,
            prefix,
            optional,
        } => {
            let Some(config_map) = config_maps.get(name) else {
                return vec![variable];
            };
            config_map
                .data
                .as_ref()
                .into_iter()
                .flatten()
                .map(|(key, value)| PodEnvironmentVariableDetail {
                    name: format!("{prefix}{key}"),
                    value: Some(value.clone()),
                    source: PodEnvironmentVariableSource::ConfigMapKey {
                        name: name.clone(),
                        key: key.clone(),
                        optional: *optional,
                    },
                })
                .collect()
        }
        PodEnvironmentVariableSource::SecretImport {
            name,
            prefix,
            optional,
        } => {
            let Some(secret) = secrets.get(name) else {
                return vec![variable];
            };
            secret
                .data
                .as_ref()
                .into_iter()
                .flatten()
                .map(|(key, value)| PodEnvironmentVariableDetail {
                    name: format!("{prefix}{key}"),
                    value: Some(String::from_utf8_lossy(&value.0).into_owned()),
                    source: PodEnvironmentVariableSource::SecretKey {
                        name: name.clone(),
                        key: key.clone(),
                        optional: *optional,
                    },
                })
                .collect()
        }
        _ => vec![variable],
    }
}

pub(crate) fn config_map_value(config_map: Option<&ConfigMap>, key: &str) -> Option<String> {
    config_map
        .and_then(|config_map| config_map.data.as_ref())
        .and_then(|data| data.get(key))
        .cloned()
}

pub(crate) fn secret_value(secret: Option<&Secret>, key: &str) -> Option<String> {
    secret
        .and_then(|secret| secret.data.as_ref())
        .and_then(|data| data.get(key))
        .map(|value| String::from_utf8_lossy(&value.0).into_owned())
}

pub(crate) fn expand_environment_variable_references(
    variables: &mut [PodEnvironmentVariableDetail],
) {
    let mut values = BTreeMap::new();
    for variable in variables {
        if matches!(variable.source, PodEnvironmentVariableSource::Literal)
            && let Some(value) = &variable.value
        {
            variable.value = Some(expand_environment_variable_value(value, &values));
        }
        if let Some(value) = &variable.value {
            values.insert(variable.name.clone(), value.clone());
        }
    }
}

pub(crate) fn expand_environment_variable_value(
    value: &str,
    values: &BTreeMap<String, String>,
) -> String {
    let mut result = String::new();
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '$' {
            result.push(character);
            continue;
        }
        if characters.next_if_eq(&'$').is_some() {
            result.push('$');
            continue;
        }
        if characters.next_if_eq(&'(').is_none() {
            result.push('$');
            continue;
        }
        let mut name = String::new();
        for character in characters.by_ref() {
            if character == ')' {
                break;
            }
            name.push(character);
        }
        if let Some(replacement) = values.get(&name) {
            result.push_str(replacement);
        } else {
            result.push_str("$(");
            result.push_str(&name);
            result.push(')');
        }
    }
    result
}
