use super::*;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};

struct AbortProbe(Arc<AtomicUsize>);

impl Future for AbortProbe {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}

impl Drop for AbortProbe {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

fn worker_state() -> WorkerState {
    let (sender, _receiver) = mpsc::channel(1);
    WorkerState {
        results: WorkerResultSender::new(sender, None),
        connections: Arc::new(Mutex::new(HashMap::new())),
        resource_watches: Arc::new(Mutex::new(ResourceWatchRegistry::default())),
        detail_watches: Arc::new(TaskRegistry::default()),
        pod_metrics_watches: Arc::new(TaskRegistry::default()),
        node_metrics_watches: Arc::new(TaskRegistry::default()),
        log_streams: Arc::new(TaskRegistry::default()),
        watch_initialization_slots: Arc::new(Mutex::new(HashMap::new())),
        log_store_appender: None,
    }
}

fn pod_resource() -> ApiResource {
    ApiResource {
        group: "core".to_owned(),
        version: "v1".to_owned(),
        kind: "Pod".to_owned(),
        name: "pods".to_owned(),
        namespaced: true,
    }
}

mod commands;
mod flow;
mod watches;
