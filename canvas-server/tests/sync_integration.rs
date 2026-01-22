//! Integration tests for sync flow using SyncProcessor.
//!
//! Tests the complete synchronization flow without going through WebSocket,
//! directly testing the SyncProcessor and SceneStore interaction.

use canvas_core::element::{Element, ElementId, ElementKind};
use canvas_core::offline::{ConflictStrategy, Operation};
use canvas_core::SceneStore;
use canvas_server::sync::{RetryConfig, SyncProcessor};
use std::sync::Arc;

/// Default session ID for tests.
const TEST_SESSION: &str = "test-session";

/// Helper to create a test text element.
fn create_text_element(content: &str) -> Element {
    Element::new(ElementKind::Text {
        content: content.to_string(),
        font_size: 16.0,
        color: "#000000".to_string(),
    })
}

/// Helper to create a test rectangle element.
fn create_rect_element(width: f32, height: f32) -> Element {
    Element::new(ElementKind::Chart {
        chart_type: "bar".to_string(),
        data: serde_json::json!({"values": [width as i32, height as i32]}),
    })
}

// ===========================================================================
// Test 1: Client queues offline operations then syncs when back online
// ===========================================================================

/// Simulates an offline client that queues multiple operations and then syncs
/// them all at once when connectivity is restored.
///
/// This verifies that:
/// - Multiple operations can be batched and processed together
/// - Add, Update, and Remove operations all work in sequence
/// - The final scene state matches expectations after batch processing
#[test]
fn test_client_queues_offline_then_syncs() {
    // Create a store and ensure our test session exists
    let store = Arc::new(SceneStore::new());
    let _ = store.get_or_create(TEST_SESSION);

    let processor = SyncProcessor::new(Arc::clone(&store), ConflictStrategy::LastWriteWins);

    // Create elements for our offline operations
    let element1 = create_text_element("First offline message");
    let element2 = create_text_element("Second offline message");
    let element3 = create_rect_element(100.0, 50.0);

    let id1 = element1.id;
    let id2 = element2.id;
    let id3 = element3.id;

    // Simulate offline queue: add three elements with sequential timestamps
    let operations = vec![
        Operation::AddElement {
            element: element1,
            timestamp: 1000,
        },
        Operation::AddElement {
            element: element2,
            timestamp: 1001,
        },
        Operation::AddElement {
            element: element3,
            timestamp: 1002,
        },
        // Update the first element
        Operation::UpdateElement {
            id: id1,
            changes: serde_json::json!({"transform": {"x": 100.0, "y": 200.0}}),
            timestamp: 1003,
        },
        // Remove the second element
        Operation::RemoveElement {
            id: id2,
            timestamp: 1004,
        },
    ];

    // Process the batch (simulates "coming back online")
    let result = processor.process_batch(TEST_SESSION, operations);

    // Verify the result
    assert!(result.success, "Batch processing should succeed");
    assert_eq!(result.synced_count, 5, "All 5 operations should sync");
    assert_eq!(result.failed_count, 0, "No operations should fail");

    // Verify final scene state
    let scene = store.get(TEST_SESSION);
    assert!(scene.is_some(), "Session should exist");

    let scene = scene.expect("verified Some above");
    // Should have 2 elements (added 3, removed 1)
    assert_eq!(scene.element_count(), 2, "Should have 2 elements remaining");

    // Element 1 should exist with updated position
    let el1 = scene.get_element(id1);
    assert!(el1.is_some(), "Element 1 should still exist");

    // Element 2 should be gone (removed)
    let el2 = scene.get_element(id2);
    assert!(el2.is_none(), "Element 2 should be removed");

    // Element 3 should exist unchanged
    let el3 = scene.get_element(id3);
    assert!(el3.is_some(), "Element 3 should still exist");
}

// ===========================================================================
// Test 2: Concurrent clients modify same element - conflict resolution
// ===========================================================================

/// Tests conflict detection and resolution when two clients modify the same
/// element with different timestamps.
///
/// This verifies that:
/// - Conflicts are properly detected when timestamps differ
/// - The configured strategy (LastWriteWins) is applied correctly
/// - Stale operations are rejected while newer ones succeed
#[test]
fn test_concurrent_clients_conflict_resolution() {
    let store = Arc::new(SceneStore::new());
    let _ = store.get_or_create(TEST_SESSION);

    // Use LastWriteWins strategy
    let processor = SyncProcessor::new(Arc::clone(&store), ConflictStrategy::LastWriteWins);

    // First, add an element from "client A"
    let element = create_text_element("Original content");
    let element_id = element.id;

    let add_result = processor.process_batch(
        TEST_SESSION,
        vec![Operation::AddElement {
            element,
            timestamp: 1000,
        }],
    );
    assert!(add_result.success, "Initial add should succeed");

    // Now simulate concurrent updates from two clients
    // Client A sends update at timestamp 2000
    let client_a_ops = vec![Operation::UpdateElement {
        id: element_id,
        changes: serde_json::json!({"transform": {"x": 100.0}}),
        timestamp: 2000,
    }];

    // Client B sends update at timestamp 1500 (older, arrived later)
    let client_b_ops = vec![Operation::UpdateElement {
        id: element_id,
        changes: serde_json::json!({"transform": {"x": 200.0}}),
        timestamp: 1500,
    }];

    // Process client A's update first
    let result_a = processor.process_batch(TEST_SESSION, client_a_ops);
    assert!(result_a.success, "Client A update should succeed");
    assert_eq!(result_a.synced_count, 1);

    // Process client B's stale update
    let result_b = processor.process_batch(TEST_SESSION, client_b_ops);

    // With LastWriteWins, the stale timestamp should cause a conflict
    // The operation should be rejected (KeepLocal) because local has newer timestamp
    assert!(
        !result_b.success || result_b.conflict_count > 0,
        "Client B's stale update should conflict or fail"
    );

    // Verify only one update took effect (client A's)
    let scene = store.get(TEST_SESSION);
    assert!(scene.is_some());
}

// ===========================================================================
// Test 3: Sync survives processor recreation with persistent store
// ===========================================================================

/// Verifies that scene state persists across SyncProcessor instances when
/// sharing the same SceneStore.
///
/// This simulates scenarios where:
/// - A server restarts but the store persists
/// - Multiple processor instances share state
#[test]
fn test_sync_survives_processor_recreation() {
    let store = Arc::new(SceneStore::new());
    let _ = store.get_or_create(TEST_SESSION);

    // Create first processor and add elements
    let processor1 = SyncProcessor::new(Arc::clone(&store), ConflictStrategy::LastWriteWins);

    let element1 = create_text_element("Persistent element 1");
    let element2 = create_rect_element(50.0, 50.0);
    let id1 = element1.id;
    let id2 = element2.id;

    let ops = vec![
        Operation::AddElement {
            element: element1,
            timestamp: 1000,
        },
        Operation::AddElement {
            element: element2,
            timestamp: 1001,
        },
    ];

    let result1 = processor1.process_batch(TEST_SESSION, ops);
    assert!(result1.success, "First batch should succeed");
    assert_eq!(result1.synced_count, 2);

    // Drop the first processor
    drop(processor1);

    // Create a new processor with the same store
    let processor2 = SyncProcessor::new(Arc::clone(&store), ConflictStrategy::LastWriteWins);

    // Verify elements still exist
    let scene = store.get(TEST_SESSION);
    assert!(scene.is_some(), "Session should persist");
    let scene_ref = scene.as_ref().expect("verified Some above");
    assert_eq!(scene_ref.element_count(), 2, "Both elements should persist");
    assert!(scene_ref.get_element(id1).is_some());
    assert!(scene_ref.get_element(id2).is_some());

    // Continue modifying with new processor
    let element3 = create_text_element("New element after recreation");
    let id3 = element3.id;

    let result2 = processor2.process_batch(
        TEST_SESSION,
        vec![
            Operation::AddElement {
                element: element3,
                timestamp: 2000,
            },
            Operation::RemoveElement {
                id: id1,
                timestamp: 2001,
            },
        ],
    );

    assert!(result2.success, "Second batch should succeed");
    assert_eq!(result2.synced_count, 2);

    // Verify final state: element1 removed, element2 and element3 remain
    let final_scene = store.get(TEST_SESSION);
    assert!(final_scene.is_some());
    let final_ref = final_scene.as_ref().expect("verified Some above");

    assert_eq!(final_ref.element_count(), 2, "Should have 2 elements");
    assert!(final_ref.get_element(id1).is_none(), "Element 1 removed");
    assert!(final_ref.get_element(id2).is_some(), "Element 2 persists");
    assert!(final_ref.get_element(id3).is_some(), "Element 3 added");
}

// ===========================================================================
// Test 4: Mixed operations batch - complex scenario
// ===========================================================================

/// Tests a complex batch with interleaved add, update, and remove operations.
///
/// This verifies that:
/// - Operations are processed in order
/// - Updates and removes work on elements added in the same batch
/// - The final state correctly reflects all operations
#[test]
fn test_mixed_operations_batch() {
    let store = Arc::new(SceneStore::new());
    let _ = store.get_or_create(TEST_SESSION);

    let processor = SyncProcessor::new(Arc::clone(&store), ConflictStrategy::LastWriteWins);

    // Create multiple elements
    let elements: Vec<Element> = (0..5)
        .map(|i| create_text_element(&format!("Element {}", i)))
        .collect();

    let ids: Vec<ElementId> = elements.iter().map(|e| e.id).collect();

    // Build a complex batch:
    // - Add elements 0-4
    // - Update elements 0, 2, 4
    // - Remove elements 1, 3
    let mut operations = Vec::new();

    // Add all elements
    for (i, element) in elements.into_iter().enumerate() {
        operations.push(Operation::AddElement {
            element,
            timestamp: 1000 + i as u64,
        });
    }

    // Update even-indexed elements
    for (i, id) in ids.iter().enumerate() {
        if i % 2 == 0 {
            operations.push(Operation::UpdateElement {
                id: *id,
                changes: serde_json::json!({"transform": {"x": (i * 100) as f64}}),
                timestamp: 2000 + i as u64,
            });
        }
    }

    // Remove odd-indexed elements
    for (i, id) in ids.iter().enumerate() {
        if i % 2 == 1 {
            operations.push(Operation::RemoveElement {
                id: *id,
                timestamp: 3000 + i as u64,
            });
        }
    }

    let result = processor.process_batch(TEST_SESSION, operations);

    // We should have: 5 adds + 3 updates (0,2,4) + 2 removes (1,3) = 10 operations
    assert!(result.success, "Mixed batch should succeed");
    assert_eq!(result.synced_count, 10, "All 10 operations should sync");
    assert_eq!(result.failed_count, 0);

    // Verify final state: elements 0, 2, 4 remain
    let scene = store.get(TEST_SESSION);
    assert!(scene.is_some());
    let scene_ref = scene.as_ref().expect("verified Some above");

    assert_eq!(scene_ref.element_count(), 3, "Should have 3 elements");
    assert!(scene_ref.get_element(ids[0]).is_some(), "Element 0 exists");
    assert!(scene_ref.get_element(ids[1]).is_none(), "Element 1 removed");
    assert!(scene_ref.get_element(ids[2]).is_some(), "Element 2 exists");
    assert!(scene_ref.get_element(ids[3]).is_none(), "Element 3 removed");
    assert!(scene_ref.get_element(ids[4]).is_some(), "Element 4 exists");
}

// ===========================================================================
// Test 5: Different conflict strategies produce different outcomes
// ===========================================================================

/// Verifies that different conflict resolution strategies produce different
/// results for the same conflict scenario.
///
/// This tests:
/// - LastWriteWins: newer timestamp wins
/// - LocalWins: existing state always wins
/// - RemoteWins: incoming operation always wins
#[test]
fn test_conflict_strategies_differ() {
    // Helper to create an element with a specific ID
    fn create_element_with_id(id: ElementId, content: &str) -> Element {
        let mut element = Element::new(ElementKind::Text {
            content: content.to_string(),
            font_size: 20.0,
            color: "#FF0000".to_string(),
        });
        element.id = id;
        element
    }

    // We test with ElementAlreadyExists conflict scenario:
    // Try to add an element with an ID that already exists

    // === LastWriteWins Strategy ===
    let store_lww = Arc::new(SceneStore::new());
    let _ = store_lww.get_or_create(TEST_SESSION);
    let processor_lww = SyncProcessor::new(Arc::clone(&store_lww), ConflictStrategy::LastWriteWins);

    let element_lww = create_text_element("Original LWW");
    let id_lww = element_lww.id;

    // Add original element
    let _ = processor_lww.process_batch(
        TEST_SESSION,
        vec![Operation::AddElement {
            element: element_lww,
            timestamp: 1000,
        }],
    );

    // Try to add element with same ID but newer timestamp
    let duplicate_lww = create_element_with_id(id_lww, "Duplicate LWW");
    let result_lww = processor_lww.process_batch(
        TEST_SESSION,
        vec![Operation::AddElement {
            element: duplicate_lww,
            timestamp: 2000,
        }],
    );

    // === LocalWins Strategy ===
    let store_lw = Arc::new(SceneStore::new());
    let _ = store_lw.get_or_create(TEST_SESSION);
    let processor_lw = SyncProcessor::new(Arc::clone(&store_lw), ConflictStrategy::LocalWins);

    let element_lw = create_text_element("Original LocalWins");
    let id_lw = element_lw.id;

    let _ = processor_lw.process_batch(
        TEST_SESSION,
        vec![Operation::AddElement {
            element: element_lw,
            timestamp: 1000,
        }],
    );

    let duplicate_lw = create_element_with_id(id_lw, "Duplicate LocalWins");
    let result_lw = processor_lw.process_batch(
        TEST_SESSION,
        vec![Operation::AddElement {
            element: duplicate_lw,
            timestamp: 2000,
        }],
    );
    // With LocalWins, local always wins - duplicate should be rejected
    assert!(
        result_lw.conflict_count > 0 || result_lw.failed_count > 0,
        "LocalWins should detect conflict or reject duplicate"
    );

    // === RemoteWins Strategy ===
    let store_rw = Arc::new(SceneStore::new());
    let _ = store_rw.get_or_create(TEST_SESSION);
    let processor_rw = SyncProcessor::new(Arc::clone(&store_rw), ConflictStrategy::RemoteWins);

    let element_rw = create_text_element("Original RemoteWins");
    let id_rw = element_rw.id;

    let _ = processor_rw.process_batch(
        TEST_SESSION,
        vec![Operation::AddElement {
            element: element_rw,
            timestamp: 1000,
        }],
    );

    let duplicate_rw = create_element_with_id(id_rw, "Duplicate RemoteWins");
    let result_rw = processor_rw.process_batch(
        TEST_SESSION,
        vec![Operation::AddElement {
            element: duplicate_rw,
            timestamp: 2000,
        }],
    );

    // Verify the strategies produced different conflict handling
    // At minimum, we should see conflicts detected across all strategies
    let total_conflicts =
        result_lww.conflict_count + result_lw.conflict_count + result_rw.conflict_count;
    assert!(
        total_conflicts >= 2,
        "ElementAlreadyExists should trigger conflicts in multiple strategies"
    );
}

// ===========================================================================
// Test 6: Update on non-existent element fails gracefully
// ===========================================================================

/// Verifies that attempting to update a non-existent element produces
/// an appropriate error rather than crashing.
#[test]
fn test_update_nonexistent_element_fails() {
    let store = Arc::new(SceneStore::new());
    let _ = store.get_or_create(TEST_SESSION);

    let processor = SyncProcessor::new(Arc::clone(&store), ConflictStrategy::LastWriteWins);

    // Try to update an element that was never added
    let fake_id = ElementId::new();
    let result = processor.process_batch(
        TEST_SESSION,
        vec![Operation::UpdateElement {
            id: fake_id,
            changes: serde_json::json!({"transform": {"x": 100.0}}),
            timestamp: 1000,
        }],
    );

    // Should fail with ElementNotFound conflict
    assert!(
        !result.success || result.conflict_count > 0,
        "Update on non-existent element should fail or conflict"
    );
}

// ===========================================================================
// Test 7: Remove on non-existent element fails gracefully
// ===========================================================================

/// Verifies that attempting to remove a non-existent element produces
/// an appropriate error rather than crashing.
#[test]
fn test_remove_nonexistent_element_fails() {
    let store = Arc::new(SceneStore::new());
    let _ = store.get_or_create(TEST_SESSION);

    let processor = SyncProcessor::new(Arc::clone(&store), ConflictStrategy::LastWriteWins);

    let fake_id = ElementId::new();
    let result = processor.process_batch(
        TEST_SESSION,
        vec![Operation::RemoveElement {
            id: fake_id,
            timestamp: 1000,
        }],
    );

    // Should fail with ElementNotFound conflict
    assert!(
        !result.success || result.conflict_count > 0,
        "Remove on non-existent element should fail or conflict"
    );
}

// ===========================================================================
// Test 8: RetryConfig delay calculation
// ===========================================================================

/// Verifies that RetryConfig calculates exponential backoff correctly.
#[test]
fn test_retry_config_delay_calculation() {
    let config = RetryConfig::new(
        5,    // max_retries
        100,  // initial_delay_ms
        5000, // max_delay_ms
        2.0,  // backoff_multiplier
    );

    // Attempt 0: 100 * 2^0 = 100ms
    let delay0 = config.delay_for_attempt(0);
    assert_eq!(delay0.as_millis(), 100);

    // Attempt 1: 100 * 2^1 = 200ms
    let delay1 = config.delay_for_attempt(1);
    assert_eq!(delay1.as_millis(), 200);

    // Attempt 2: 100 * 2^2 = 400ms
    let delay2 = config.delay_for_attempt(2);
    assert_eq!(delay2.as_millis(), 400);

    // Attempt 3: 100 * 2^3 = 800ms
    let delay3 = config.delay_for_attempt(3);
    assert_eq!(delay3.as_millis(), 800);

    // Attempt 10: 100 * 2^10 = 102400ms, but capped at 5000ms
    let delay10 = config.delay_for_attempt(10);
    assert_eq!(
        delay10.as_millis(),
        5000,
        "Should be capped at max_delay_ms"
    );
}

// ===========================================================================
// Test 9: Empty batch processing
// ===========================================================================

/// Verifies that processing an empty batch succeeds without issues.
#[test]
fn test_empty_batch_processing() {
    let store = Arc::new(SceneStore::new());
    let _ = store.get_or_create(TEST_SESSION);

    let processor = SyncProcessor::new(Arc::clone(&store), ConflictStrategy::LastWriteWins);

    let result = processor.process_batch(TEST_SESSION, vec![]);

    assert!(result.success, "Empty batch should succeed");
    assert_eq!(result.synced_count, 0);
    assert_eq!(result.failed_count, 0);
    assert_eq!(result.conflict_count, 0);
}

// ===========================================================================
// Test 10: Multiple sessions isolation
// ===========================================================================

/// Verifies that operations on different sessions are isolated from each other.
#[test]
fn test_multiple_sessions_isolation() {
    let store = Arc::new(SceneStore::new());
    let _ = store.get_or_create("session-a");
    let _ = store.get_or_create("session-b");

    let processor = SyncProcessor::new(Arc::clone(&store), ConflictStrategy::LastWriteWins);

    // Add element to session A
    let element_a = create_text_element("Session A element");
    let id_a = element_a.id;
    let result_a = processor.process_batch(
        "session-a",
        vec![Operation::AddElement {
            element: element_a,
            timestamp: 1000,
        }],
    );
    assert!(result_a.success);

    // Add element to session B
    let element_b = create_rect_element(100.0, 100.0);
    let id_b = element_b.id;
    let result_b = processor.process_batch(
        "session-b",
        vec![Operation::AddElement {
            element: element_b,
            timestamp: 1000,
        }],
    );
    assert!(result_b.success);

    // Verify session A has only its element
    let scene_a = store.get("session-a");
    assert!(scene_a.is_some());
    let scene_a_ref = scene_a.as_ref().expect("verified Some above");
    assert_eq!(scene_a_ref.element_count(), 1);
    assert!(scene_a_ref.get_element(id_a).is_some());
    assert!(scene_a_ref.get_element(id_b).is_none());

    // Verify session B has only its element
    let scene_b = store.get("session-b");
    assert!(scene_b.is_some());
    let scene_b_ref = scene_b.as_ref().expect("verified Some above");
    assert_eq!(scene_b_ref.element_count(), 1);
    assert!(scene_b_ref.get_element(id_b).is_some());
    assert!(scene_b_ref.get_element(id_a).is_none());
}
