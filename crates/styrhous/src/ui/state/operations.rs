use std::collections::BTreeMap;

/// Worker operations of one kind that still await their own terminal result.
///
/// Every submitted operation keeps its own id until its result arrives, so a
/// later submission can never hide an earlier operation's outcome (GitHub
/// issue #30), and duplicate or unknown results are recognisable. Ids are never
/// reused, not even after a reconnect (see [`Self::restarted`]), so a result
/// that outlives its session cannot settle an operation of the next one.
#[derive(Debug)]
pub(crate) struct InFlightOperations<T = ()> {
    last_id: u64,
    operations: BTreeMap<u64, T>,
}

impl<T> Default for InFlightOperations<T> {
    fn default() -> Self {
        Self {
            last_id: 0,
            operations: BTreeMap::new(),
        }
    }
}

impl<T> InFlightOperations<T> {
    /// Registers a new operation and returns the id its command must carry.
    pub(crate) fn start(&mut self, operation: T) -> u64 {
        self.last_id += 1;
        self.operations.insert(self.last_id, operation);
        self.last_id
    }

    /// Removes the operation for a terminal result, or `None` when the result
    /// is a duplicate or belongs to no tracked operation.
    pub(crate) fn settle(&mut self, operation_id: u64) -> Option<T> {
        self.operations.remove(&operation_id)
    }

    /// Forgets the tracked operations for a new session but keeps the id
    /// sequence, so ids from the previous session stay unknown.
    pub(crate) fn restarted(&self) -> Self {
        Self {
            last_id: self.last_id,
            operations: BTreeMap::new(),
        }
    }

    pub(crate) fn contains(&self, operation: &T) -> bool
    where
        T: PartialEq,
    {
        self.operations.values().any(|tracked| tracked == operation)
    }

    #[cfg(test)]
    pub(crate) fn ids(&self) -> Vec<u64> {
        self.operations.keys().copied().collect()
    }

    #[cfg(test)]
    pub(crate) fn last_started_id(&self) -> u64 {
        self.last_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_operation_is_settled_once_by_its_own_id() {
        let mut operations = InFlightOperations::default();
        let first = operations.start("first");
        let second = operations.start("second");
        assert_eq!((first, second), (1, 2));

        assert_eq!(operations.settle(second), Some("second"));
        assert_eq!(operations.settle(second), None, "duplicate result");
        assert_eq!(operations.settle(99), None, "unknown result");
        assert!(operations.contains(&"first"));
        assert!(!operations.contains(&"second"));
        assert_eq!(operations.start("third"), 3, "ids are never reused");

        let mut next_session = operations.restarted();
        assert_eq!(next_session.ids(), Vec::<u64>::new());
        assert_eq!(
            next_session.settle(first),
            None,
            "previous sessions are unknown"
        );
        assert_eq!(
            next_session.start("fourth"),
            4,
            "ids continue across sessions"
        );
    }
}
