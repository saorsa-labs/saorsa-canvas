//! # Offline Mode and Sync
//!
//! Provides offline operation queuing and eventual consistency.
//!
//! ## Usage
//!
//! ```text
//! 1. When online: operations execute immediately
//! 2. When offline: operations queue locally
//! 3. On reconnect: queued operations sync with conflict resolution
//! ```

use crate::element::{Element, ElementId};
use crate::event::InputEvent;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

/// A queued operation for offline sync.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Operation {
    /// Add a new element to the scene.
    AddElement {
        /// The element to add.
        element: Element,
        /// Timestamp when operation was created (ms since epoch).
        timestamp: u64,
    },
    /// Update an existing element.
    UpdateElement {
        /// The element ID to update.
        id: ElementId,
        /// The changes as JSON patch.
        changes: serde_json::Value,
        /// Timestamp when operation was created (ms since epoch).
        timestamp: u64,
    },
    /// Remove an element from the scene.
    RemoveElement {
        /// The element ID to remove.
        id: ElementId,
        /// Timestamp when operation was created (ms since epoch).
        timestamp: u64,
    },
    /// A user interaction event.
    Interaction {
        /// The input event.
        event: InputEvent,
        /// Timestamp when operation was created (ms since epoch).
        timestamp: u64,
    },
}

impl Operation {
    /// Get the timestamp of this operation.
    #[must_use]
    pub const fn timestamp(&self) -> u64 {
        match self {
            Self::AddElement { timestamp, .. }
            | Self::UpdateElement { timestamp, .. }
            | Self::RemoveElement { timestamp, .. }
            | Self::Interaction { timestamp, .. } => *timestamp,
        }
    }

    /// Get the current timestamp in milliseconds since epoch.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // Timestamps won't exceed u64 for billions of years
    pub fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

/// Result of a sync operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncResult {
    /// Number of operations successfully synced.
    pub synced_count: usize,
    /// Number of operations that had conflicts.
    pub conflict_count: usize,
    /// Number of operations received from remote.
    pub received_count: usize,
    /// Whether all operations were synced successfully.
    pub success: bool,
}

/// Conflict resolution strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ConflictStrategy {
    /// Last write wins - newer timestamp takes precedence.
    #[default]
    LastWriteWins,
    /// Local wins - local operations take precedence.
    LocalWins,
    /// Remote wins - remote operations take precedence.
    RemoteWins,
}

/// Queue for offline operations with persistence support.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OfflineQueue {
    /// Pending operations to sync.
    pending: VecDeque<Operation>,
    /// Last successful sync timestamp.
    last_sync: Option<u64>,
    /// Conflict resolution strategy.
    strategy: ConflictStrategy,
    /// Maximum queue size (oldest operations dropped when exceeded).
    max_size: usize,
}

impl OfflineQueue {
    /// Create a new empty offline queue.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            last_sync: None,
            strategy: ConflictStrategy::default(),
            max_size: 1000,
        }
    }

    /// Create a queue with a custom max size.
    #[must_use]
    pub fn with_max_size(max_size: usize) -> Self {
        Self {
            max_size,
            ..Self::new()
        }
    }

    /// Set the conflict resolution strategy.
    pub fn set_strategy(&mut self, strategy: ConflictStrategy) {
        self.strategy = strategy;
    }

    /// Get the current conflict resolution strategy.
    #[must_use]
    pub const fn strategy(&self) -> ConflictStrategy {
        self.strategy
    }

    /// Enqueue an operation.
    pub fn enqueue(&mut self, op: Operation) {
        // Drop oldest if at capacity
        if self.pending.len() >= self.max_size {
            self.pending.pop_front();
        }
        self.pending.push_back(op);
    }

    /// Get the number of pending operations.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Check if the queue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Get the last sync timestamp.
    #[must_use]
    pub const fn last_sync(&self) -> Option<u64> {
        self.last_sync
    }

    /// Peek at pending operations without removing them.
    #[must_use]
    pub fn pending(&self) -> &VecDeque<Operation> {
        &self.pending
    }

    /// Take all pending operations for syncing.
    pub fn take_pending(&mut self) -> Vec<Operation> {
        self.pending.drain(..).collect()
    }

    /// Mark operations as synced successfully.
    pub fn mark_synced(&mut self, count: usize, timestamp: u64) {
        self.last_sync = Some(timestamp);
        // Operations are already removed by take_pending
        let _ = count; // Used for logging/metrics
    }

    /// Re-queue operations that failed to sync.
    pub fn requeue(&mut self, ops: Vec<Operation>) {
        for op in ops.into_iter().rev() {
            self.pending.push_front(op);
        }
    }

    /// Resolve conflicts between local and remote operations.
    #[must_use]
    pub fn resolve_conflict(&self, local: &Operation, remote: &Operation) -> ConflictResolution {
        match self.strategy {
            ConflictStrategy::LastWriteWins => {
                if local.timestamp() >= remote.timestamp() {
                    ConflictResolution::KeepLocal
                } else {
                    ConflictResolution::KeepRemote
                }
            }
            ConflictStrategy::LocalWins => ConflictResolution::KeepLocal,
            ConflictStrategy::RemoteWins => ConflictResolution::KeepRemote,
        }
    }

    /// Clear all pending operations.
    pub fn clear(&mut self) {
        self.pending.clear();
    }

    /// Serialize the queue for persistence.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize a queue from persisted JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Result of conflict resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    /// Keep the local operation.
    KeepLocal,
    /// Keep the remote operation.
    KeepRemote,
    /// Merge both operations (not yet implemented).
    Merge,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::ElementKind;

    fn create_test_element() -> Element {
        Element::new(ElementKind::Text {
            content: "Test".to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        })
    }

    #[test]
    fn test_queue_creation() {
        let queue = OfflineQueue::new();
        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
        assert!(queue.last_sync().is_none());
    }

    #[test]
    fn test_queue_with_max_size() {
        let queue = OfflineQueue::with_max_size(10);
        assert_eq!(queue.max_size, 10);
    }

    #[test]
    fn test_enqueue_operation() {
        let mut queue = OfflineQueue::new();
        let element = create_test_element();
        let op = Operation::AddElement {
            element,
            timestamp: Operation::now(),
        };

        queue.enqueue(op);
        assert_eq!(queue.len(), 1);
        assert!(!queue.is_empty());
    }

    #[test]
    fn test_enqueue_multiple() {
        let mut queue = OfflineQueue::new();

        for i in 0..5 {
            let op = Operation::RemoveElement {
                id: ElementId::new(),
                timestamp: Operation::now() + i,
            };
            queue.enqueue(op);
        }

        assert_eq!(queue.len(), 5);
    }

    #[test]
    fn test_max_size_drops_oldest() {
        let mut queue = OfflineQueue::with_max_size(3);

        for i in 0_u64..5 {
            let op = Operation::RemoveElement {
                id: ElementId::new(),
                timestamp: i,
            };
            queue.enqueue(op);
        }

        // Should only have 3 (the newest)
        assert_eq!(queue.len(), 3);

        // Check timestamps are 2, 3, 4 (oldest dropped)
        let pending: Vec<_> = queue.pending().iter().collect();
        assert_eq!(pending[0].timestamp(), 2);
        assert_eq!(pending[1].timestamp(), 3);
        assert_eq!(pending[2].timestamp(), 4);
    }

    #[test]
    fn test_take_pending() {
        let mut queue = OfflineQueue::new();
        queue.enqueue(Operation::RemoveElement {
            id: ElementId::new(),
            timestamp: 1,
        });
        queue.enqueue(Operation::RemoveElement {
            id: ElementId::new(),
            timestamp: 2,
        });

        let ops = queue.take_pending();
        assert_eq!(ops.len(), 2);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_requeue() {
        let mut queue = OfflineQueue::new();
        let id = ElementId::new();

        queue.enqueue(Operation::RemoveElement {
            id,
            timestamp: 1,
        });

        let ops = queue.take_pending();
        assert!(queue.is_empty());

        queue.requeue(ops);
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn test_mark_synced() {
        let mut queue = OfflineQueue::new();
        queue.enqueue(Operation::RemoveElement {
            id: ElementId::new(),
            timestamp: 1,
        });

        let _ = queue.take_pending();
        queue.mark_synced(1, 1000);

        assert_eq!(queue.last_sync(), Some(1000));
    }

    #[test]
    fn test_conflict_last_write_wins_local() {
        let queue = OfflineQueue::new();

        let local = Operation::RemoveElement {
            id: ElementId::new(),
            timestamp: 200,
        };
        let remote = Operation::RemoveElement {
            id: ElementId::new(),
            timestamp: 100,
        };

        let resolution = queue.resolve_conflict(&local, &remote);
        assert_eq!(resolution, ConflictResolution::KeepLocal);
    }

    #[test]
    fn test_conflict_last_write_wins_remote() {
        let queue = OfflineQueue::new();

        let local = Operation::RemoveElement {
            id: ElementId::new(),
            timestamp: 100,
        };
        let remote = Operation::RemoveElement {
            id: ElementId::new(),
            timestamp: 200,
        };

        let resolution = queue.resolve_conflict(&local, &remote);
        assert_eq!(resolution, ConflictResolution::KeepRemote);
    }

    #[test]
    fn test_conflict_local_wins() {
        let mut queue = OfflineQueue::new();
        queue.set_strategy(ConflictStrategy::LocalWins);

        let local = Operation::RemoveElement {
            id: ElementId::new(),
            timestamp: 100,
        };
        let remote = Operation::RemoveElement {
            id: ElementId::new(),
            timestamp: 200,
        };

        let resolution = queue.resolve_conflict(&local, &remote);
        assert_eq!(resolution, ConflictResolution::KeepLocal);
    }

    #[test]
    fn test_conflict_remote_wins() {
        let mut queue = OfflineQueue::new();
        queue.set_strategy(ConflictStrategy::RemoteWins);

        let local = Operation::RemoveElement {
            id: ElementId::new(),
            timestamp: 200,
        };
        let remote = Operation::RemoveElement {
            id: ElementId::new(),
            timestamp: 100,
        };

        let resolution = queue.resolve_conflict(&local, &remote);
        assert_eq!(resolution, ConflictResolution::KeepRemote);
    }

    #[test]
    fn test_json_serialization() {
        let mut queue = OfflineQueue::new();
        let element = create_test_element();

        queue.enqueue(Operation::AddElement {
            element,
            timestamp: 12345,
        });

        let json = queue.to_json().expect("serialization should work");
        assert!(json.contains("12345"));
        assert!(json.contains("AddElement"));
    }

    #[test]
    fn test_json_deserialization() {
        let mut queue = OfflineQueue::new();
        queue.enqueue(Operation::RemoveElement {
            id: ElementId::new(),
            timestamp: 999,
        });

        let json = queue.to_json().expect("serialization should work");
        let restored = OfflineQueue::from_json(&json).expect("deserialization should work");

        assert_eq!(restored.len(), 1);
        assert_eq!(restored.pending()[0].timestamp(), 999);
    }

    #[test]
    fn test_clear() {
        let mut queue = OfflineQueue::new();
        queue.enqueue(Operation::RemoveElement {
            id: ElementId::new(),
            timestamp: 1,
        });
        queue.enqueue(Operation::RemoveElement {
            id: ElementId::new(),
            timestamp: 2,
        });

        assert_eq!(queue.len(), 2);
        queue.clear();
        assert!(queue.is_empty());
    }

    #[test]
    fn test_operation_timestamp() {
        let now = Operation::now();
        assert!(now > 0);

        let op = Operation::AddElement {
            element: create_test_element(),
            timestamp: now,
        };
        assert_eq!(op.timestamp(), now);
    }

    #[test]
    fn test_update_element_operation() {
        let op = Operation::UpdateElement {
            id: ElementId::new(),
            changes: serde_json::json!({"color": "#ff0000"}),
            timestamp: 100,
        };

        assert_eq!(op.timestamp(), 100);
    }

    #[test]
    fn test_sync_result_default() {
        let result = SyncResult::default();
        assert_eq!(result.synced_count, 0);
        assert_eq!(result.conflict_count, 0);
        assert_eq!(result.received_count, 0);
        assert!(!result.success);
    }

    #[test]
    fn test_strategy_getter() {
        let queue = OfflineQueue::new();
        assert_eq!(queue.strategy(), ConflictStrategy::LastWriteWins);
    }
}
