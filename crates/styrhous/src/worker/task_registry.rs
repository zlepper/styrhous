use super::*;

/// Shared state accessible from spawned async tasks
pub(super) type SharedTaskRegistry<Key> = Arc<TaskRegistry<Key>>;

/// Owns one family of cancellable worker tasks.
///
/// Task removal happens while the registry is locked, but task abortion happens
/// after releasing that lock. This keeps lifecycle operations short and avoids
/// holding the registry lock across the bounded join in `abort_task`.
pub(super) struct TaskRegistry<Key> {
    tasks: Mutex<HashMap<Key, JoinHandle<()>>>,
}

impl<Key> Default for TaskRegistry<Key> {
    fn default() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
        }
    }
}

impl<Key> TaskRegistry<Key>
where
    Key: Eq + std::hash::Hash + Clone,
{
    pub(super) async fn replace(&self, key: Key, task: JoinHandle<()>) {
        let previous = self.tasks.lock().await.insert(key, task);
        if let Some(previous) = previous {
            abort_task(previous).await;
        }
    }

    /// Replace a task whose creation must not overlap with the previous task.
    /// Detail watches and pod log streams use this because both can emit data
    /// immediately after they are spawned.
    pub(super) async fn replace_after_abort(
        &self,
        key: Key,
        create: impl FnOnce() -> JoinHandle<()>,
    ) {
        self.abort(&key).await;
        self.replace(key, create()).await;
    }

    pub(super) async fn abort(&self, key: &Key) {
        let task = self.tasks.lock().await.remove(key);
        if let Some(task) = task {
            abort_task(task).await;
        }
    }

    pub(super) async fn abort_matching(&self, matches: impl Fn(&Key) -> bool) {
        let mut tasks = self.tasks.lock().await;
        let keys = tasks
            .keys()
            .filter(|key| matches(key))
            .cloned()
            .collect::<Vec<_>>();
        let removed = keys
            .into_iter()
            .filter_map(|key| tasks.remove(&key))
            .collect::<Vec<_>>();
        drop(tasks);
        for task in removed {
            abort_task(task).await;
        }
    }

    pub(super) async fn abort_all(&self) {
        self.abort_matching(|_| true).await;
    }

    #[cfg(test)]
    pub(super) async fn is_empty(&self) -> bool {
        self.tasks.lock().await.is_empty()
    }

    #[cfg(test)]
    pub(super) async fn contains_key(&self, key: &Key) -> bool {
        self.tasks.lock().await.contains_key(key)
    }
}
