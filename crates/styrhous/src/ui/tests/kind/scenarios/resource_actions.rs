//! Kind CronJob and resource-scale action scenarios.

use super::*;

#[test]
fn test_cron_job_run_now_integration() {
    let fixture = IntegrationNamespaceFixture::create("cron-job-run", "anchor", "unused");
    let cron_job_name = "on-demand-report".to_owned();
    let runtime = &fixture.runtime;
    let client = runtime.block_on(async {
        Client::try_default()
            .await
            .expect("Failed to create Kubernetes client")
    });
    let cron_jobs: Api<CronJob> = Api::namespaced(client.clone(), &fixture.namespace);
    let jobs: Api<Job> = Api::namespaced(client, &fixture.namespace);
    let cron_job = runtime.block_on(async {
        cron_jobs
            .create(
                &Default::default(),
                &CronJob {
                    metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                        name: Some(cron_job_name.clone()),
                        namespace: Some(fixture.namespace.clone()),
                        ..Default::default()
                    },
                    spec: Some(CronJobSpec {
                        schedule: "0 0 1 1 *".into(),
                        suspend: Some(true),
                        job_template: JobTemplateSpec {
                            metadata: Some(
                                k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                                    labels: Some(BTreeMap::from([(
                                        "app".to_owned(),
                                        "on-demand-report".to_owned(),
                                    )])),
                                    annotations: Some(BTreeMap::from([(
                                        "example.com/runbook".to_owned(),
                                        "reporting".to_owned(),
                                    )])),
                                    ..Default::default()
                                },
                            ),
                            spec: Some(JobSpec {
                                template: PodTemplateSpec {
                                    spec: Some(PodSpec {
                                        restart_policy: Some("Never".to_owned()),
                                        containers: vec![Container {
                                            name: "report".to_owned(),
                                            image: Some("registry.k8s.io/pause:3.10".to_owned()),
                                            ..Default::default()
                                        }],
                                        ..Default::default()
                                    }),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }),
                        },
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .await
            .expect("Failed to create CronJob")
    });
    let cron_job_uid = cron_job.metadata.uid.expect("CronJob has UID");

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &fixture.namespace);
    let cron_jobs_resource = select_resource(&mut harness, "Apps & Containers", "Cron Jobs");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        cron_jobs_resource,
        &fixture.namespace,
    );
    for _ in 0..3 {
        harness.run_steps(1);
    }
    harness
        .get_by_label(&format!("More actions for {cron_job_name}"))
        .click();
    wait_for_harness(
        &mut harness,
        |harness| {
            harness
                .query_by_role_and_label(egui::accesskit::Role::Button, "Run now")
                .map(|_| ())
        },
        5_000,
    );
    harness.get_by_label("Run now").click();
    wait_for(
        &mut harness,
        |app| {
            app.ui_state.clusters[&cluster_key]
                .pending_cron_job_run
                .as_ref()
                .filter(|pending| pending.resource_name == cron_job_name)
                .map(|_| ())
        },
        5_000,
    );
    wait_for_harness(
        &mut harness,
        |harness| {
            let mut buttons =
                harness.query_all_by_role_and_label(egui::accesskit::Role::Button, "Run now");
            let button = buttons.next()?;
            (buttons.next().is_none() && !button.accesskit_node().is_disabled()).then_some(())
        },
        5_000,
    );
    harness.get_by_label("Run now").click();

    wait_for_with_diagnostic(
        &mut harness,
        |_| {
            runtime
                .block_on(async { jobs.list(&Default::default()).await })
                .ok()
                .and_then(|list| {
                    list.items.into_iter().find(|job| {
                        job.metadata
                            .generate_name
                            .as_deref()
                            .is_some_and(|prefix| prefix == "on-demand-report-manual-")
                            && job
                                .metadata
                                .annotations
                                .as_ref()
                                .and_then(|annotations| {
                                    annotations.get("cronjob.kubernetes.io/instantiate")
                                })
                                .is_some_and(|value| value == "manual")
                            && job
                                .metadata
                                .labels
                                .as_ref()
                                .and_then(|labels| labels.get("app"))
                                .is_some_and(|value| value == "on-demand-report")
                            && job
                                .metadata
                                .owner_references
                                .as_ref()
                                .is_some_and(|owners| {
                                    owners.iter().any(|owner| {
                                        owner.kind == "CronJob"
                                            && owner.name == cron_job_name
                                            && owner.uid == cron_job_uid
                                            && owner.controller == Some(true)
                                    })
                                })
                            && job.spec.as_ref().is_some_and(|spec| {
                                spec.template
                                    .spec
                                    .as_ref()
                                    .and_then(|pod_spec| pod_spec.containers.first())
                                    .and_then(|container| container.image.as_deref())
                                    == Some("registry.k8s.io/pause:3.10")
                            })
                    })
                })
                .map(|_| ())
        },
        |app| {
            app.ui_state.clusters[&cluster_key]
                .cron_job_run_error
                .clone()
        },
        10_000,
    );
}

/// Verifies that the generic Scale action uses the discovered Deployment scale endpoint.

#[test]
fn test_resource_scale_integration() {
    let fixture = IntegrationNamespaceFixture::create("resource-scale", "anchor", "unused");
    let deployment_name = "scalable-deployment".to_owned();
    let runtime = &fixture.runtime;
    let client = runtime.block_on(async {
        Client::try_default()
            .await
            .expect("Failed to create Kubernetes client")
    });
    let deployments: Api<Deployment> = Api::namespaced(client, &fixture.namespace);
    runtime.block_on(async {
        deployments
            .create(
                &Default::default(),
                &Deployment {
                    metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                        name: Some(deployment_name.clone()),
                        namespace: Some(fixture.namespace.clone()),
                        ..Default::default()
                    },
                    spec: Some(DeploymentSpec {
                        replicas: Some(1),
                        selector: LabelSelector {
                            match_labels: Some(BTreeMap::from([(
                                "app".to_owned(),
                                deployment_name.clone(),
                            )])),
                            ..Default::default()
                        },
                        template: PodTemplateSpec {
                            metadata: Some(
                                k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                                    labels: Some(BTreeMap::from([(
                                        "app".to_owned(),
                                        deployment_name.clone(),
                                    )])),
                                    ..Default::default()
                                },
                            ),
                            spec: Some(PodSpec {
                                containers: vec![Container {
                                    name: "pause".to_owned(),
                                    image: Some("registry.k8s.io/pause:3.10".to_owned()),
                                    ..Default::default()
                                }],
                                ..Default::default()
                            }),
                        },
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .await
            .expect("Failed to create Deployment");
    });

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &fixture.namespace);
    let deployments_resource = select_resource(&mut harness, "Apps & Containers", "Deployments");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        deployments_resource,
        &fixture.namespace,
    );
    for _ in 0..3 {
        harness.run_steps(1);
    }
    let actions_label = format!("More actions for {deployment_name}");
    harness.get_by_label(&actions_label).click();
    harness.run_steps(1);
    harness.get_by_label("Scale").click();
    wait_for(
        &mut harness,
        |app| {
            app.ui_state.clusters[&cluster_key]
                .pending_scale
                .as_ref()
                .map(|_| ())
        },
        10_000,
    );
    // Worker results are applied after the workspace render pass. Render the
    // resulting modal before targeting its pointer controls.
    harness.run_steps(1);
    harness.get_by_label("Increase desired replicas").click();
    wait_for(
        &mut harness,
        |app| {
            app.ui_state.clusters[&cluster_key]
                .pending_scale
                .as_ref()
                .filter(|pending| pending.desired_replicas == "2")
                .map(|_| ())
        },
        5_000,
    );
    harness.get_by_label("Update scale").click();
    wait_for_with_diagnostic(
        &mut harness,
        |app| {
            app.ui_state.clusters[&cluster_key]
                .pending_scale
                .is_none()
                .then_some(())
        },
        |app| app.ui_state.clusters[&cluster_key].scale_error.clone(),
        5_000,
    );

    wait_for(
        &mut harness,
        |_| {
            runtime
                .block_on(async { deployments.get(&deployment_name).await })
                .ok()
                .and_then(|deployment| deployment.spec.and_then(|spec| spec.replicas))
                .filter(|replicas| *replicas == 2)
                .map(|_| ())
        },
        10_000,
    );
}
