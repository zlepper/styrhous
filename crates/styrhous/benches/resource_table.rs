use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;
use styrhous::ResourceTableProfile;

fn resource_table(c: &mut Criterion) {
    let mut group = c.benchmark_group("resource_table");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(2));

    for row_count in configured_row_counts() {
        group.bench_with_input(
            BenchmarkId::new("first_render", row_count),
            &row_count,
            |bench, &row_count| {
                bench.iter_batched_ref(
                    || {
                        ResourceTableProfile::with_resource_count(row_count)
                            .expect("resource-table profile must initialize")
                    },
                    |profile| black_box(profile.run_frame()),
                    BatchSize::SmallInput,
                )
            },
        );

        let mut idle = warmed_profile(row_count);
        group.bench_with_input(
            BenchmarkId::new("warmed_idle_frame", row_count),
            &row_count,
            |bench, _| bench.iter(|| black_box(idle.run_frame())),
        );

        let mut scrolling = warmed_profile(row_count);
        group.bench_with_input(
            BenchmarkId::new("scroll_frame", row_count),
            &row_count,
            |bench, _| bench.iter(|| black_box(scrolling.scroll_frame())),
        );

        let mut searching = warmed_profile(row_count);
        group.bench_with_input(
            BenchmarkId::new("fuzzy_search_frame", row_count),
            &row_count,
            |bench, _| bench.iter(|| black_box(searching.search_frame())),
        );

        let mut sorting = warmed_profile(row_count);
        group.bench_with_input(
            BenchmarkId::new("name_sort_frame", row_count),
            &row_count,
            |bench, _| bench.iter(|| black_box(sorting.sort_frame())),
        );

        let mut updating = warmed_profile(row_count);
        group.bench_with_input(
            BenchmarkId::new("watch_update_frame", row_count),
            &row_count,
            |bench, _| bench.iter(|| black_box(updating.update_frame())),
        );
    }
    group.finish();
}

/// Pods in the realistic scenario: a medium cluster.
const REALISTIC_POD_COUNT: usize = 600;

/// Whole frames as the app renders them (sidebar, a Pods table with status lamps,
/// tessellation) at a whole and a fractional display scale.
fn realistic_frames(c: &mut Criterion) {
    let mut group = c.benchmark_group("realistic_frame");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(3));

    for pixels_per_point in [1.0, 1.25] {
        let mut idle = warmed_realistic_profile(pixels_per_point);
        group.bench_with_input(
            BenchmarkId::new("idle", pixels_per_point),
            &pixels_per_point,
            |bench, _| bench.iter(|| black_box(idle.run_frame())),
        );

        let mut scrolling = warmed_realistic_profile(pixels_per_point);
        group.bench_with_input(
            BenchmarkId::new("scroll", pixels_per_point),
            &pixels_per_point,
            |bench, _| bench.iter(|| black_box(scrolling.scroll_frame())),
        );
        eprintln!(
            "realistic_frame @{pixels_per_point}×: {:?}",
            scrolling.last_frame()
        );
    }
    group.finish();
}

fn warmed_realistic_profile(pixels_per_point: f32) -> ResourceTableProfile {
    let mut profile = ResourceTableProfile::realistic(REALISTIC_POD_COUNT, pixels_per_point)
        .expect("realistic profile must initialize");
    profile.run_frame();
    profile.run_frame();
    profile
}

fn warmed_profile(row_count: usize) -> ResourceTableProfile {
    let mut profile = ResourceTableProfile::with_resource_count(row_count)
        .expect("resource-table profile must initialize");
    profile.run_frame();
    profile
}

fn configured_row_counts() -> Vec<usize> {
    match std::env::var("RESOURCE_TABLE_ROW_COUNTS") {
        Ok(value) => value
            .split(',')
            .map(|value| {
                let count = value.trim().parse().unwrap_or_else(|_| {
                    panic!("RESOURCE_TABLE_ROW_COUNTS must contain positive integers")
                });
                assert!(count > 0, "RESOURCE_TABLE_ROW_COUNTS must be positive");
                count
            })
            .collect(),
        Err(_) => vec![1_000, 5_000, 10_000],
    }
}

criterion_group!(benches, resource_table, realistic_frames);
criterion_main!(benches);
