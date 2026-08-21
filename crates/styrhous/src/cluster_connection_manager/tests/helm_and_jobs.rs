use super::*;

#[test]
fn duplicate_helm_revisions_prefer_the_secret_storage_record() {
    let release = |storage| HelmRelease {
        storage,
        storage_name: "record".into(),
        name: "demo".into(),
        namespace: "apps".into(),
        revision: 1,
        status: "deployed".into(),
        description: String::new(),
        notes: String::new(),
        chart: "chart".into(),
        chart_version: "1.0.0".into(),
        app_version: String::new(),
        first_deployed: String::new(),
        last_deployed: String::new(),
        values: Default::default(),
        manifest: String::new(),
        storage_labels: BTreeMap::new(),
        storage_annotations: BTreeMap::new(),
    };
    let records = BTreeMap::from([
        ("configmap/record".into(), release(StorageDriver::ConfigMap)),
        ("secret/record".into(), release(StorageDriver::Secret)),
    ]);

    let merged = merged_helm_releases(&records);

    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].storage, StorageDriver::Secret);
}

#[test]
fn cron_job_run_copies_the_template_and_marks_the_job_as_manual() {
    let cron_job = CronJob {
        metadata: ObjectMeta {
            name: Some("nightly-report".into()),
            uid: Some("cron-job-uid".into()),
            ..Default::default()
        },
        spec: Some(CronJobSpec {
            schedule: "0 0 * * *".into(),
            job_template: JobTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(BTreeMap::from([("team".into(), "analytics".into())])),
                    annotations: Some(BTreeMap::from([(
                        "example.com/runbook".into(),
                        "nightly".into(),
                    )])),
                    ..Default::default()
                }),
                spec: Some(JobSpec::default()),
            },
            ..Default::default()
        }),
        ..Default::default()
    };

    let job = job_from_cron_job(&cron_job).expect("valid CronJob");

    assert_eq!(
        job.metadata.generate_name.as_deref(),
        Some("nightly-report-manual-")
    );
    assert_eq!(job.metadata.name, None);
    assert_eq!(job.metadata.labels.as_ref().unwrap()["team"], "analytics");
    assert_eq!(
        job.metadata.annotations.as_ref().unwrap()["example.com/runbook"],
        "nightly"
    );
    assert_eq!(
        job.metadata.annotations.as_ref().unwrap()["cronjob.kubernetes.io/instantiate"],
        "manual"
    );
    assert_eq!(job.spec, Some(JobSpec::default()));
    assert_eq!(
        job.metadata.owner_references,
        Some(vec![OwnerReference {
            api_version: "batch/v1".into(),
            kind: "CronJob".into(),
            name: "nightly-report".into(),
            uid: "cron-job-uid".into(),
            controller: Some(true),
            block_owner_deletion: None,
        }])
    );
}

#[test]
fn cron_job_run_rejects_missing_required_source_fields() {
    assert!(job_from_cron_job(&CronJob::default()).is_err());

    let missing_spec = CronJob {
        metadata: ObjectMeta {
            name: Some("nightly-report".into()),
            uid: Some("cron-job-uid".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(job_from_cron_job(&missing_spec).is_err());

    let missing_template_spec = CronJob {
        metadata: ObjectMeta {
            name: Some("nightly-report".into()),
            uid: Some("cron-job-uid".into()),
            ..Default::default()
        },
        spec: Some(CronJobSpec {
            schedule: "0 0 * * *".into(),
            job_template: JobTemplateSpec::default(),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert!(job_from_cron_job(&missing_template_spec).is_err());

    let missing_uid = CronJob {
        metadata: ObjectMeta {
            name: Some("nightly-report".into()),
            ..Default::default()
        },
        spec: Some(CronJobSpec {
            schedule: "0 0 * * *".into(),
            job_template: JobTemplateSpec {
                spec: Some(JobSpec::default()),
                ..Default::default()
            },
            ..Default::default()
        }),
        ..Default::default()
    };
    assert!(job_from_cron_job(&missing_uid).is_err());
}
