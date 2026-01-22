# Phase 7: Offline Mode & Sync

## Overview
Connect the existing offline queue infrastructure (canvas-core/offline.rs), service worker (web/sw.js), and WebSocket sync protocol (canvas-server/src/sync.rs) to enable full offline functionality with eventual consistency.

## Technical Decisions
- Breakdown approach: By layer (Server sync logic → Client integration → UI)
- Task size: Small (1 file, ~50 lines)
- Testing strategy: Unit tests + Integration tests + Property tests
- Dependencies: Uses Phase 6 types/infrastructure
- Pattern: Follow existing offline.rs conflict resolution and sync.rs protocol

## Tasks

<task type="auto" priority="p1">
  <n>Task 1: Add SyncProcessor struct</n>
  <files>
    canvas-server/src/sync.rs
  </files>
  <depends></depends>
  <action>
    Create a SyncProcessor struct that handles batched operation processing:

    ```rust
    pub struct SyncProcessor {
        store: Arc<SceneStore>,
        conflict_strategy: ConflictStrategy,
    }

    impl SyncProcessor {
        pub fn new(store: Arc<SceneStore>, strategy: ConflictStrategy) -> Self;
        pub async fn process_batch(&self, session_id: &str, operations: Vec<Operation>) -> SyncResult;
    }
    ```

    Requirements:
    - NO .unwrap() or .expect() in src/
    - Use thiserror for error types
    - Follow patterns from canvas-core/offline.rs
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server -- -D warnings
    cargo test -p canvas-server
  </verify>
  <done>
    - SyncProcessor struct exists with new() and process_batch()
    - Compiles without warnings
    - Basic tests pass
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 2: Implement operation replay logic</n>
  <files>
    canvas-server/src/sync.rs
  </files>
  <depends>Task 1</depends>
  <action>
    Implement the apply_operation method to replay queued operations:

    ```rust
    impl SyncProcessor {
        fn apply_operation(&self, session_id: &str, op: &Operation) -> Result<(), SyncError> {
            match op {
                Operation::AddElement { element, .. } => {
                    self.store.add_element(session_id, element.clone())?;
                }
                Operation::UpdateElement { id, changes, .. } => {
                    self.store.update_element(session_id, id, changes)?;
                }
                Operation::RemoveElement { id, .. } => {
                    self.store.remove_element(session_id, id)?;
                }
                Operation::Interaction { .. } => {
                    // Interactions are logged but not replayed
                }
            }
            Ok(())
        }
    }
    ```

    Requirements:
    - Handle all Operation variants from canvas-core/offline.rs
    - Return proper errors for each failure case
    - Log operations for debugging
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server -- -D warnings
    cargo test -p canvas-server
  </verify>
  <done>
    - apply_operation handles all Operation variants
    - Errors properly propagated
    - Tests for each operation type
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 3: Add conflict detection & resolution</n>
  <files>
    canvas-server/src/sync.rs
  </files>
  <depends>Task 2</depends>
  <action>
    Add conflict detection when applying operations:

    ```rust
    #[derive(Debug, Clone)]
    pub struct Conflict {
        pub operation: Operation,
        pub reason: ConflictReason,
        pub resolved: bool,
    }

    #[derive(Debug, Clone)]
    pub enum ConflictReason {
        ElementNotFound,
        ElementAlreadyExists,
        StaleTimestamp { local: u64, remote: u64 },
        ConcurrentModification,
    }

    impl SyncProcessor {
        fn detect_conflict(&self, session_id: &str, op: &Operation) -> Option<Conflict>;
        fn resolve_conflict(&self, conflict: &Conflict) -> Resolution;
    }
    ```

    Requirements:
    - Use ConflictStrategy from offline.rs (LastWriteWins, LocalWins, RemoteWins)
    - Track conflicts in SyncResult
    - Log conflict resolution decisions
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server -- -D warnings
    cargo test -p canvas-server
  </verify>
  <done>
    - Conflict struct and ConflictReason enum defined
    - detect_conflict and resolve_conflict implemented
    - Tests for each conflict scenario
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 4: Add sync result tracking & metrics</n>
  <files>
    canvas-server/src/sync.rs
  </files>
  <depends>Task 3</depends>
  <action>
    Enhance SyncResult with detailed metrics:

    ```rust
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SyncResult {
        pub success: bool,
        pub synced_count: usize,
        pub conflict_count: usize,
        pub failed_count: usize,
        pub conflicts: Vec<Conflict>,
        pub failed_operations: Vec<FailedOperation>,
        pub duration_ms: u64,
        pub timestamp: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FailedOperation {
        pub operation: Operation,
        pub error: String,
        pub retryable: bool,
    }
    ```

    Requirements:
    - Track timing for performance monitoring
    - Categorize failures as retryable or permanent
    - Include enough detail for client to show status
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server -- -D warnings
    cargo test -p canvas-server
  </verify>
  <done>
    - SyncResult has all metrics fields
    - FailedOperation tracks retryable status
    - Tests verify correct counting
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 5: Add retry with exponential backoff</n>
  <files>
    canvas-server/src/sync.rs
  </files>
  <depends>Task 4</depends>
  <action>
    Add retry configuration and backoff logic:

    ```rust
    #[derive(Debug, Clone)]
    pub struct RetryConfig {
        pub max_retries: u32,
        pub initial_delay_ms: u64,
        pub max_delay_ms: u64,
        pub backoff_multiplier: f64,
    }

    impl Default for RetryConfig {
        fn default() -> Self {
            Self {
                max_retries: 3,
                initial_delay_ms: 100,
                max_delay_ms: 5000,
                backoff_multiplier: 2.0,
            }
        }
    }

    impl SyncProcessor {
        pub async fn process_with_retry(
            &self,
            session_id: &str,
            operations: Vec<Operation>,
            config: &RetryConfig,
        ) -> SyncResult;
    }
    ```

    Requirements:
    - Only retry retryable failures
    - Cap delay at max_delay_ms
    - Track retry attempts in result
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server -- -D warnings
    cargo test -p canvas-server
  </verify>
  <done>
    - RetryConfig struct with sensible defaults
    - process_with_retry implements backoff
    - Tests verify retry behavior
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 6: Unit tests for SyncProcessor</n>
  <files>
    canvas-server/src/sync.rs
  </files>
  <depends>Task 5</depends>
  <action>
    Add comprehensive unit tests:

    ```rust
    #[cfg(test)]
    mod tests {
        // Test batch processing
        #[tokio::test]
        async fn test_process_empty_batch();
        #[tokio::test]
        async fn test_process_single_add();
        #[tokio::test]
        async fn test_process_mixed_operations();

        // Test conflict detection
        #[test]
        fn test_detect_element_not_found();
        #[test]
        fn test_detect_stale_timestamp();

        // Test conflict resolution
        #[test]
        fn test_resolve_last_write_wins();
        #[test]
        fn test_resolve_local_wins();

        // Test retry logic
        #[tokio::test]
        async fn test_retry_on_transient_failure();
        #[tokio::test]
        async fn test_no_retry_on_permanent_failure();

        // Property tests
        proptest! {
            #[test]
            fn prop_sync_preserves_order(ops in vec(any::<Operation>(), 0..100));
        }
    }
    ```

    Requirements:
    - Cover all public methods
    - Test edge cases (empty batch, all fail, all succeed)
    - Add proptest for invariants
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server -- -D warnings
    cargo test -p canvas-server -- --nocapture
  </verify>
  <done>
    - At least 10 unit tests
    - Property tests with proptest
    - All tests pass
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 7: Integration tests for sync flow</n>
  <files>
    canvas-server/tests/sync_integration.rs
  </files>
  <depends>Task 6</depends>
  <action>
    Create integration test file testing full sync flow:

    ```rust
    //! Integration tests for sync flow

    use canvas_core::{offline::Operation, Element, ElementKind};
    use canvas_server::sync::SyncProcessor;

    #[tokio::test]
    async fn test_client_queues_offline_then_syncs() {
        // 1. Create store
        // 2. Create processor
        // 3. Simulate offline operations
        // 4. Process batch
        // 5. Verify scene state
    }

    #[tokio::test]
    async fn test_concurrent_clients_conflict_resolution() {
        // 1. Two clients modify same element
        // 2. Both sync
        // 3. Verify conflict resolved correctly
    }

    #[tokio::test]
    async fn test_sync_survives_restart() {
        // 1. Queue operations
        // 2. Simulate server restart
        // 3. Resume sync
        // 4. Verify all operations applied
    }
    ```

    Requirements:
    - Test realistic scenarios
    - Use actual SceneStore
    - Verify final state correctness
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server -- -D warnings
    cargo test -p canvas-server --test sync_integration
  </verify>
  <done>
    - At least 5 integration tests
    - Tests cover offline → online sync
    - Tests cover conflict scenarios
    - All tests pass
  </done>
</task>

## Exit Criteria
- [ ] All 7 tasks complete
- [ ] All tests passing
- [ ] Zero clippy warnings
- [ ] Code reviewed via /review
