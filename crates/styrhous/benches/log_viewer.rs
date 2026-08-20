use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;
use styrhous::LogViewerProfile;

const DEFAULT_WIDE_PAYLOAD_BYTES: &[usize] = &[256, 1024, 4 * 1024, 16 * 1024];

fn log_viewer(c: &mut Criterion) {
    if let Some(samples) = std::env::var("LOG_VIEWER_LATENCY_SAMPLES")
        .ok()
        .map(|value| {
            value
                .parse::<usize>()
                .expect("sample count must be a positive integer")
        })
    {
        for scenario in benchmark_scenarios() {
            let mut profile = LogViewerProfile::with_total_lines_and_payload_bytes(
                scenario.row_count,
                scenario.payload_bytes,
            )
            .expect("benchmark log viewer must initialize");
            report_page_transition_latency(&mut profile, scenario, samples);
        }
        return;
    }
    let mut group = c.benchmark_group("pod_log_viewer");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(5));

    for scenario in benchmark_scenarios() {
        let mut profile = LogViewerProfile::with_total_lines_and_payload_bytes(
            scenario.row_count,
            scenario.payload_bytes,
        )
        .expect("benchmark log viewer must initialize");
        group.bench_with_input(
            BenchmarkId::new(
                format!("{}/cached_scroll_cpu", scenario.name),
                scenario.row_count,
            ),
            &scenario,
            |bench, _| bench.iter(|| black_box(profile.scroll_cached_rows())),
        );
        group.bench_with_input(
            BenchmarkId::new(
                format!("{}/page_cache_churn_end_to_end", scenario.name),
                scenario.row_count,
            ),
            &scenario,
            |bench, _| {
                bench.iter(|| {
                    black_box(
                        profile
                            .load_and_render_next_page()
                            .expect("benchmark page load must succeed"),
                    )
                })
            },
        );
    }
    group.finish();
}

#[derive(Clone)]
struct BenchmarkScenario {
    name: String,
    row_count: usize,
    payload_bytes: usize,
}

fn benchmark_scenarios() -> Vec<BenchmarkScenario> {
    configured_row_counts("LOG_VIEWER_ROW_COUNTS", &[10_000, 100_000, 1_000_000])
        .into_iter()
        .map(|row_count| BenchmarkScenario {
            name: "normal".to_owned(),
            row_count,
            payload_bytes: 36,
        })
        .chain(
            configured_row_counts("LOG_VIEWER_WIDE_ROW_COUNTS", &[10_000])
                .into_iter()
                .flat_map(|row_count| {
                    configured_payload_widths(
                        "LOG_VIEWER_WIDE_PAYLOAD_BYTES",
                        DEFAULT_WIDE_PAYLOAD_BYTES,
                    )
                    .into_iter()
                    .map(move |payload_bytes| BenchmarkScenario {
                        name: format!("wide_{}", payload_width_label(payload_bytes)),
                        row_count,
                        payload_bytes,
                    })
                }),
        )
        .collect()
}

fn configured_row_counts(variable: &str, default: &[usize]) -> Vec<usize> {
    configured_positive_integers(variable, default)
}

fn configured_payload_widths(variable: &str, default: &[usize]) -> Vec<usize> {
    configured_positive_integers(variable, default)
}

fn configured_positive_integers(variable: &str, default: &[usize]) -> Vec<usize> {
    match std::env::var(variable) {
        Ok(value) => value
            .split(',')
            .map(|value| {
                let value = value.trim().parse().unwrap_or_else(|_| {
                    panic!("{variable} must be comma-separated positive integers")
                });
                assert!(value > 0, "{variable} must contain only positive integers");
                value
            })
            .collect(),
        Err(_) => default.to_vec(),
    }
}

fn payload_width_label(payload_bytes: usize) -> String {
    if payload_bytes.is_multiple_of(1024) {
        format!("{}k", payload_bytes / 1024)
    } else {
        format!("{payload_bytes}b")
    }
}

fn report_page_transition_latency(
    profile: &mut LogViewerProfile,
    scenario: BenchmarkScenario,
    samples: usize,
) {
    assert!(samples > 0, "sample count must be positive");
    let mut request_frames = Vec::with_capacity(samples);
    let mut store_waits = Vec::with_capacity(samples);
    let mut loaded_frames = Vec::with_capacity(samples);
    for _ in 0..samples {
        let (timings, _) = profile
            .load_and_render_next_page_timed()
            .expect("page transition must succeed");
        request_frames.push(timings.request_frame);
        store_waits.push(timings.store_wait);
        loaded_frames.push(timings.loaded_frame);
    }
    println!(
        "page transition latency ({} rows, {}, {}-byte payloads, {samples} samples):\n  request frame: {}\n  store ready: {}\n  loaded frame: {}",
        scenario.row_count,
        scenario.name,
        scenario.payload_bytes,
        latency_summary(&request_frames),
        latency_summary(&store_waits),
        latency_summary(&loaded_frames),
    );
}

fn latency_summary(samples: &[Duration]) -> String {
    let mut samples = samples.to_vec();
    samples.sort_unstable();
    let median = samples[samples.len() / 2];
    let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
    format!("p50 {median:?}, p95 {p95:?}")
}

criterion_group!(benches, log_viewer);
criterion_main!(benches);
