//! Background recovery of API groups whose discovery failed during the initial inspection,
//! so a transient failure (for example an aggregated API that was still starting) does not
//! leave the cluster half-loaded until the application restarts.

use super::{ApiGroupTarget, KubernetesApiInspector};

use crate::api_resource::{ApiResource, ApiResourceSet};
use crate::worker::*;
use anyhow::Result;
use futures_util::stream::StreamExt;
use std::collections::BTreeSet;
use std::time::Duration;

/// A retry that has not answered by now is counted as failed, like in the initial discovery.
const GROUP_RETRY_TIMEOUT: Duration = Duration::from_secs(15);
const CONCURRENT_GROUP_RETRIES: usize = 4;

/// Groups that answered a retry, with the resources they expose.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct RecoveredApiGroups {
    pub(crate) group_versions: Vec<String>,
    pub(crate) api_resources: Vec<ApiResource>,
    pub(crate) scalable_api_resources: BTreeSet<ApiResource>,
    /// Groups that are still failing and will be retried.
    pub(crate) still_failing: Vec<String>,
}

/// Retries `pending` until every group has recovered, passing each round's recoveries to
/// `publish`. `publish` returns `false` when nobody is listening any more, which ends the
/// loop. The owner stops the loop by dropping (aborting) the future.
///
/// `delays` supplies the wait before each round and `sleep` performs it, so tests can run
/// the schedule without real time.
pub(crate) async fn retry_failed_api_groups<Sleep, Fetch, Publish>(
    mut delays: impl Iterator<Item = Duration>,
    sleep: impl Fn(Duration) -> Sleep,
    fetch_group: impl Fn(ApiGroupTarget) -> Fetch,
    mut publish: impl FnMut(RecoveredApiGroups) -> Publish,
    mut pending: Vec<ApiGroupTarget>,
) where
    Sleep: Future<Output = ()>,
    Fetch: Future<Output = Result<ApiResourceSet>>,
    Publish: Future<Output = bool>,
{
    while !pending.is_empty() {
        let Some(delay) = delays.next() else {
            return;
        };
        sleep(delay).await;

        let attempts = futures_util::stream::iter(std::mem::take(&mut pending))
            .map(|target| {
                let fetch_group = &fetch_group;
                async move {
                    let outcome = fetch_group(target.clone()).await;
                    (target, outcome)
                }
            })
            .buffered(CONCURRENT_GROUP_RETRIES)
            .collect::<Vec<_>>()
            .await;

        let mut recovered = RecoveredApiGroups::default();
        for (target, outcome) in attempts {
            match outcome {
                Ok(discovered) => {
                    recovered.group_versions.push(target.group_version);
                    recovered.api_resources.extend(discovered.api_resources);
                    recovered
                        .scalable_api_resources
                        .extend(discovered.scalable_api_resources);
                }
                Err(error) => {
                    tracing::debug!(
                        group_version = %target.group_version,
                        error = %format!("{error:#}"),
                        "API group discovery retry failed"
                    );
                    pending.push(target);
                }
            }
        }
        if recovered.group_versions.is_empty() {
            continue;
        }
        recovered.still_failing = pending
            .iter()
            .map(|target| target.group_version.clone())
            .collect();
        if !publish(recovered).await {
            return;
        }
    }
}

/// Retries against a real cluster and reports recoveries to the UI.
pub(crate) async fn retry_cluster_api_groups(
    cluster_key: i32,
    client: kube::Client,
    event_output: WorkerResultSender,
    failed: Vec<ApiGroupTarget>,
    delays: impl Iterator<Item = Duration>,
) {
    retry_failed_api_groups(
        delays,
        tokio::time::sleep,
        |target| {
            let client = client.clone();
            async move { fetch_group(&client, &target).await }
        },
        |recovered| {
            let event_output = event_output.clone();
            async move {
                event_output
                    .send(KubernetesApiGroupsRecovered {
                        cluster_key,
                        group_versions: recovered.group_versions,
                        api_resources: recovered.api_resources,
                        scalable_api_resources: recovered.scalable_api_resources,
                        still_failing_group_versions: recovered.still_failing,
                    })
                    .await
                    .is_ok()
            }
        },
        failed,
    )
    .await;
}

async fn fetch_group(client: &kube::Client, target: &ApiGroupTarget) -> Result<ApiResourceSet> {
    let inspector = KubernetesApiInspector {
        client: client.clone(),
    };
    match tokio::time::timeout(
        GROUP_RETRY_TIMEOUT,
        inspector.get_api_resources_for_group_versions(target),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => anyhow::bail!("timed out waiting for the API server to respond"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::bail;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::GroupVersionForDiscovery;
    use kube::runtime::watcher::DefaultBackoff;
    use std::cell::RefCell;
    use std::collections::{BTreeMap, VecDeque};

    fn target(group: &str) -> ApiGroupTarget {
        ApiGroupTarget {
            group_version: format!("{group}/v1"),
            group_name: group.to_owned(),
            versions: vec![GroupVersionForDiscovery {
                group_version: format!("{group}/v1"),
                version: "v1".to_owned(),
            }],
        }
    }

    fn resource(group: &str) -> ApiResource {
        ApiResource {
            group: group.to_owned(),
            version: "v1".to_owned(),
            kind: "Thing".to_owned(),
            name: "things".to_owned(),
            namespaced: true,
        }
    }

    /// Scripted per-group results (exhausted scripts keep failing), a recorded sleep
    /// schedule and recorded recoveries.
    #[derive(Default)]
    struct Fake {
        scripts: RefCell<BTreeMap<String, VecDeque<bool>>>,
        sleeps: RefCell<Vec<Duration>>,
        recoveries: RefCell<Vec<RecoveredApiGroups>>,
        stop_after_sleeps: Option<usize>,
    }

    impl Fake {
        fn script(self, group: &str, succeeds: &[bool]) -> Self {
            self.scripts
                .borrow_mut()
                .insert(format!("{group}/v1"), succeeds.iter().copied().collect());
            self
        }

        async fn run(&self, delays: &[u64], pending: Vec<ApiGroupTarget>) {
            retry_failed_api_groups(
                delays.iter().copied().map(Duration::from_secs),
                |delay| async move {
                    self.sleeps.borrow_mut().push(delay);
                    if self.stop_after_sleeps == Some(self.sleeps.borrow().len()) {
                        std::future::pending::<()>().await;
                    }
                },
                |target| async move {
                    let succeeds = self
                        .scripts
                        .borrow_mut()
                        .get_mut(&target.group_version)
                        .and_then(VecDeque::pop_front)
                        .unwrap_or(false);
                    if !succeeds {
                        bail!("503 Service Unavailable");
                    }
                    let resource = resource(&target.group_name);
                    Ok(ApiResourceSet {
                        scalable_api_resources: BTreeSet::from([resource.clone()]),
                        api_resources: vec![resource],
                    })
                },
                |recovered| async move {
                    self.recoveries.borrow_mut().push(recovered);
                    true
                },
                pending,
            )
            .await;
        }
    }

    const DELAYS: [u64; 8] = [5, 10, 20, 40, 40, 40, 40, 40];

    #[tokio::test]
    async fn a_group_that_fails_twice_then_succeeds_is_published_once() {
        let fake = Fake::default().script("metrics.k8s.io", &[false, false, true]);

        fake.run(&DELAYS, vec![target("metrics.k8s.io")]).await;

        assert_eq!(
            *fake.recoveries.borrow(),
            vec![RecoveredApiGroups {
                group_versions: vec!["metrics.k8s.io/v1".to_owned()],
                api_resources: vec![resource("metrics.k8s.io")],
                scalable_api_resources: BTreeSet::from([resource("metrics.k8s.io")]),
                still_failing: Vec::new(),
            }]
        );
        assert_eq!(
            *fake.sleeps.borrow(),
            [5, 10, 20].map(Duration::from_secs),
            "the loop ends once nothing is left to retry"
        );
    }

    #[tokio::test]
    async fn recoveries_are_published_per_round_and_report_the_groups_still_failing() {
        let fake = Fake::default()
            .script("a.example.com", &[true])
            .script("b.example.com", &[false, true]);

        fake.run(
            &DELAYS,
            vec![target("a.example.com"), target("b.example.com")],
        )
        .await;

        let recoveries = fake.recoveries.borrow();
        assert_eq!(recoveries.len(), 2);
        assert_eq!(recoveries[0].group_versions, vec!["a.example.com/v1"]);
        assert_eq!(recoveries[0].still_failing, vec!["b.example.com/v1"]);
        assert_eq!(recoveries[1].group_versions, vec!["b.example.com/v1"]);
        assert!(recoveries[1].still_failing.is_empty());
    }

    #[tokio::test]
    async fn a_group_that_never_recovers_keeps_retrying_on_the_supplied_growing_delays() {
        let fake = Fake {
            stop_after_sleeps: Some(8),
            ..Default::default()
        };

        let outcome = tokio::time::timeout(
            Duration::from_millis(200),
            fake.run(&DELAYS, vec![target("stale.example.com")]),
        )
        .await;

        assert!(outcome.is_err(), "the loop must not give up by itself");
        assert!(fake.recoveries.borrow().is_empty());
        assert_eq!(*fake.sleeps.borrow(), DELAYS.map(Duration::from_secs));
    }

    #[tokio::test]
    async fn the_loop_ends_when_the_receiver_is_gone() {
        let sleeps = RefCell::new(0);
        let fetched = RefCell::new(0);
        retry_failed_api_groups(
            DELAYS.map(Duration::from_secs).into_iter(),
            |_| async { *sleeps.borrow_mut() += 1 },
            |target| {
                *fetched.borrow_mut() += 1;
                async move {
                    if target.group_name == "b.example.com" {
                        bail!("still failing");
                    }
                    Ok(ApiResourceSet {
                        scalable_api_resources: BTreeSet::new(),
                        api_resources: vec![resource(&target.group_name)],
                    })
                }
            },
            |_| async { false },
            vec![target("a.example.com"), target("b.example.com")],
        )
        .await;

        assert_eq!(*sleeps.borrow(), 1, "b would be retried if anyone listened");
        assert_eq!(*fetched.borrow(), 2);
    }

    #[test]
    fn the_cluster_backoff_grows_and_stays_bounded() {
        let delays = DefaultBackoff::default().take(20).collect::<Vec<_>>();
        // kube jitters the 30s cap itself, so delays stay within twice of it.
        assert!(
            delays.iter().all(|delay| *delay <= Duration::from_secs(60)),
            "{delays:?}"
        );
        assert!(delays[0] < Duration::from_secs(2));
        assert!(delays[19] > delays[0]);
    }
}
