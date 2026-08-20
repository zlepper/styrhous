use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use kubernetes_dev_ui::YamlEditorProfile;
use std::hint::black_box;
use std::time::Duration;

fn yaml_editor(c: &mut Criterion) {
    let mut group = c.benchmark_group("deployment_yaml_editor");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(5));

    group.bench_function("apps_v1_openapi_schema_preparation", |bench| {
        bench.iter(|| {
            black_box(
                YamlEditorProfile::prepare_deployment_schema()
                    .expect("Deployment schema fixture must initialize"),
            )
        })
    });

    for document_bytes in configured_document_sizes() {
        let size_label = document_size_label(document_bytes);
        group.bench_with_input(
            BenchmarkId::new("first_render", &size_label),
            &document_bytes,
            |bench, &document_bytes| {
                bench.iter_batched_ref(
                    || {
                        YamlEditorProfile::with_document_bytes(document_bytes)
                            .expect("YAML editor profile must initialize")
                    },
                    |profile| black_box(profile.run_frame()),
                    BatchSize::SmallInput,
                )
            },
        );

        let mut idle_profile = YamlEditorProfile::with_document_bytes(document_bytes)
            .expect("YAML editor profile must initialize");
        idle_profile.run_frame();
        group.bench_with_input(
            BenchmarkId::new("warmed_idle_frame", &size_label),
            &document_bytes,
            |bench, _| bench.iter(|| black_box(idle_profile.run_frame())),
        );

        let mut scroll_profile = YamlEditorProfile::with_document_bytes(document_bytes)
            .expect("YAML editor profile must initialize");
        scroll_profile.run_frame();
        group.bench_with_input(
            BenchmarkId::new("mouse_wheel_scroll_frame", &size_label),
            &document_bytes,
            |bench, _| bench.iter(|| black_box(scroll_profile.scroll_frame())),
        );
    }
    group.finish();
}

fn configured_document_sizes() -> Vec<usize> {
    match std::env::var("YAML_EDITOR_DOCUMENT_BYTES") {
        Ok(value) => value
            .split(',')
            .map(|value| {
                let value = value.trim().parse().unwrap_or_else(|_| {
                    panic!("YAML_EDITOR_DOCUMENT_BYTES must be comma-separated positive integers")
                });
                assert!(
                    value > 0,
                    "YAML_EDITOR_DOCUMENT_BYTES must contain only positive integers"
                );
                value
            })
            .collect(),
        Err(_) => YamlEditorProfile::DEFAULT_DOCUMENT_BYTES.to_vec(),
    }
}

fn document_size_label(bytes: usize) -> String {
    if bytes.is_multiple_of(1024) {
        format!("{}k", bytes / 1024)
    } else {
        format!("{bytes}b")
    }
}

criterion_group!(benches, yaml_editor);
criterion_main!(benches);
