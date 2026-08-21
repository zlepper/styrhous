use super::*;

pub(crate) struct KubernetesApiInspector {
    pub(crate) client: kube::Client,
}

pub(crate) struct ApiInspection {
    pub(crate) api_resources: Vec<ApiResource>,
    pub(crate) scalable_api_resources: BTreeSet<ApiResource>,
    pub(crate) pod_metrics_api_available: bool,
    pub(crate) node_metrics_api_available: bool,
    pub(crate) custom_resource_columns: BTreeMap<ApiResource, Vec<CustomResourceColumn>>,
    pub(crate) resource_schemas: BTreeMap<ApiResource, ResourceSchema>,
}

pub(crate) struct DiscoveredApiResources {
    api_resources: Vec<ApiResource>,
    scalable_api_resources: BTreeSet<ApiResource>,
}

impl KubernetesApiInspector {
    async fn get_api_resources_for_group_versions(
        &self,
        api_group: APIGroup,
        versions: Vec<GroupVersionForDiscovery>,
    ) -> Result<DiscoveredApiResources> {
        let tasks = versions.iter().map(|api_group_version| {
            self.client
                .list_api_group_resources(&api_group_version.group_version)
        });

        let api_group_name = api_group.name;
        let resources = try_join_all(tasks)
            .await?
            .iter()
            .zip(versions)
            .map(|(resources, version)| {
                let version_name = version.version.clone();

                let mut api_resources = Vec::new();
                let mut scalable_api_resources = BTreeSet::new();

                for resource in &resources.resources {
                    // Skip resources like "Status" and "Scale"
                    if resource.name.contains('/') {
                        continue;
                    }

                    let api_resource = ApiResource {
                        group: api_group_name.clone(),
                        version: version_name.clone(),
                        kind: resource.kind.clone(),
                        name: resource.name.clone(),
                        namespaced: resource.namespaced,
                    };
                    if supports_scale_subresource(&resources.resources, &resource.name) {
                        scalable_api_resources.insert(api_resource.clone());
                    }
                    api_resources.push(api_resource);
                }

                DiscoveredApiResources {
                    api_resources,
                    scalable_api_resources,
                }
            })
            .fold(
                DiscoveredApiResources {
                    api_resources: Vec::new(),
                    scalable_api_resources: BTreeSet::new(),
                },
                |mut all, discovered| {
                    all.api_resources.extend(discovered.api_resources);
                    all.scalable_api_resources
                        .extend(discovered.scalable_api_resources);
                    all
                },
            );

        Ok(resources)
    }

    async fn get_core_api_resources(&self) -> Result<DiscoveredApiResources> {
        let core_api_versions = self.client.list_core_api_versions().await?;

        let mut discovered = DiscoveredApiResources {
            api_resources: Vec::new(),
            scalable_api_resources: BTreeSet::new(),
        };

        for version in &core_api_versions.versions {
            let api_resources = self.client.list_core_api_resources(version).await?;

            for resource in &api_resources.resources {
                if resource.name.contains("/") {
                    continue;
                }

                let api_resource = ApiResource {
                    group: "core".to_string(),
                    version: version.clone(),
                    kind: resource.kind.clone(),
                    name: resource.name.clone(),
                    namespaced: resource.namespaced,
                };
                if supports_scale_subresource(&api_resources.resources, &resource.name) {
                    discovered
                        .scalable_api_resources
                        .insert(api_resource.clone());
                }
                discovered.api_resources.push(api_resource);
            }
        }

        Ok(discovered)
    }

    pub(crate) async fn inspect_api(&self) -> Result<ApiInspection> {
        let api_groups = self.client.list_api_groups().await?;

        let tasks = api_groups.groups.into_iter().map(|api_group| {
            let versions = api_group
                .preferred_version
                .clone()
                .map(|v| vec![v])
                .unwrap_or_else(|| api_group.versions.clone());

            self.get_api_resources_for_group_versions(api_group, versions)
        });

        let core_resources = self.get_core_api_resources().await?;

        let discovered_resources =
            try_join_all(tasks)
                .await?
                .into_iter()
                .fold(core_resources, |mut all, discovered| {
                    all.api_resources.extend(discovered.api_resources);
                    all.scalable_api_resources
                        .extend(discovered.scalable_api_resources);
                    all
                });

        let pod_metrics_api_available =
            pod_metrics_api_available(&discovered_resources.api_resources);
        let node_metrics_api_available =
            node_metrics_api_available(&discovered_resources.api_resources);
        let (custom_resource_columns, resource_schemas) = self.custom_resource_metadata().await;
        Ok(ApiInspection {
            api_resources: discovered_resources.api_resources,
            scalable_api_resources: discovered_resources.scalable_api_resources,
            pod_metrics_api_available,
            node_metrics_api_available,
            custom_resource_columns,
            resource_schemas,
        })
    }

    async fn custom_resource_metadata(
        &self,
    ) -> (
        BTreeMap<ApiResource, Vec<CustomResourceColumn>>,
        BTreeMap<ApiResource, ResourceSchema>,
    ) {
        let crds = Api::<CustomResourceDefinition>::all(self.client.clone());
        let Ok(crds) = crds.list(&Default::default()).await else {
            // Access to CRDs is commonly restricted. Dynamic resources still work without
            // their optional columns, so do not fail API discovery in that case.
            return (BTreeMap::new(), BTreeMap::new());
        };

        let mut columns_by_resource = BTreeMap::new();
        let mut schemas_by_resource = BTreeMap::new();
        for crd in &crds.items {
            let spec = &crd.spec;
            for version in &spec.versions {
                let api_resource = ApiResource {
                    group: spec.group.clone(),
                    version: version.name.clone(),
                    kind: spec.names.kind.clone(),
                    name: spec.names.plural.clone(),
                    namespaced: spec.scope == "Namespaced",
                };
                if let Some(columns) = &version.additional_printer_columns {
                    columns_by_resource.insert(
                        api_resource.clone(),
                        columns
                            .iter()
                            .enumerate()
                            .map(|(index, column)| CustomResourceColumn {
                                id: format!("crd-{index}"),
                                label: column.name.clone(),
                                json_path: column.json_path.clone(),
                                type_: column.type_.clone(),
                                format: column.format.clone(),
                            })
                            .collect(),
                    );
                }
                if let Some(schema) = version
                    .schema
                    .as_ref()
                    .and_then(|schema| schema.open_api_v3_schema.as_ref())
                    && let Ok(root) = k8s_openapi::serde_json::to_value(schema)
                {
                    schemas_by_resource.insert(api_resource, ResourceSchema::new(root));
                }
            }
        }
        (columns_by_resource, schemas_by_resource)
    }

    pub(crate) async fn custom_resource_columns(
        &self,
    ) -> BTreeMap<ApiResource, Vec<CustomResourceColumn>> {
        self.custom_resource_metadata().await.0
    }
}

pub(crate) fn pod_metrics_api_available(api_resources: &[ApiResource]) -> bool {
    metrics_api_available(api_resources, "PodMetrics", "pods")
}

pub(crate) fn node_metrics_api_available(api_resources: &[ApiResource]) -> bool {
    metrics_api_available(api_resources, "NodeMetrics", "nodes")
}

pub(crate) fn metrics_api_available(api_resources: &[ApiResource], kind: &str, name: &str) -> bool {
    api_resources.iter().any(|resource| {
        resource.group == "metrics.k8s.io"
            && resource.version == "v1beta1"
            && resource.kind == kind
            && resource.name == name
    })
}
