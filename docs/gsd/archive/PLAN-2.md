# Phase 1.2: WebSocket Protocol

> Wire UI mutations through WebSocket for bidirectional sync

## Goal

Enable toolbar actions in `web/index.html` to send mutations via WebSocket so that:
1. Changes persist on the server
2. Changes survive page refresh
3. Changes propagate to other connected clients

## Current State

### Server (Ready)
- `canvas-server/src/sync.rs` already handles:
  - `add_element` with `message_id` acknowledgment
  - `update_element` with partial changes
  - `remove_element` by ID
  - `sync_queue` for offline operation replay
- Broadcasts `element_added`, `element_updated`, `element_removed` events

### Frontend (Blocked)
- `web/index.html:526-537` - `sendEvent()` restricts allowed messages:
  ```javascript
  const allowed = new Set(['subscribe', 'get_scene']);
  if (!allowed.has(event.type)) { return; }
  ```
- `web/index.html:679-712` - Toolbar actions call `canvasApp.addElement()` directly (local-only)
- No acknowledgment handling or error feedback

## Tasks

### Task 1: Remove sendEvent Restriction

**File**: `web/index.html`

**Changes**:
1. Remove the `allowed` set and type check in `sendEvent()`
2. The server handles unknown message types gracefully (returns error)

**Verification**:
- Console shows WebSocket messages being sent for all event types

---

### Task 2: Create sendMutation Helper

**File**: `web/index.html`

**Changes**:
1. Add `sendMutation(type, payload, callback)` function that:
   - Generates a unique `message_id`
   - Stores callback in a pending-acks map
   - Sends the message via WebSocket
   - Handles timeout for unacknowledged messages
2. Add message handler for `ack` and `error` responses

**New Code**:
```javascript
const pendingAcks = new Map();
let messageCounter = 0;

function sendMutation(type, payload, onAck, onError) {
    const messageId = `msg-${Date.now()}-${++messageCounter}`;
    const message = { type, ...payload, message_id: messageId };

    pendingAcks.set(messageId, { onAck, onError, timestamp: Date.now() });
    ws.send(JSON.stringify(message));

    // Timeout after 5 seconds
    setTimeout(() => {
        if (pendingAcks.has(messageId)) {
            pendingAcks.delete(messageId);
            if (onError) onError({ code: 'timeout', message: 'Request timed out' });
        }
    }, 5000);
}
```

**Verification**:
- Ack messages resolve pending callbacks
- Timeout fires for unresponsive server

---

### Task 3: Wire Toolbar Actions to WebSocket

**File**: `web/index.html`

**Changes**:
1. Update toolbar button handlers to use `sendMutation()`:
   - `add-bar`, `add-pie`, `add-line` → `sendMutation('add_element', { element: {...} })`
   - `add-text` → same pattern
   - `add-image` → same pattern
2. On success, update local state from server response
3. On error, show user feedback (toast or console)

**Example**:
```javascript
case 'add-bar':
    const barJson = createChartElement('bar', baseX, baseY, 300, 200);
    sendMutation('add_element', { element: JSON.parse(barJson) },
        (result) => {
            console.log('Element added:', result.id);
            // Local state will update via scene_update broadcast
        },
        (error) => {
            console.error('Failed to add element:', error.message);
        }
    );
    break;
```

**Verification**:
- Click toolbar → WebSocket message sent → element appears in scene
- Refresh page → element persists

---

### Task 4: Handle Server Broadcasts

**File**: `web/index.html`

**Changes**:
1. Update WebSocket message handler to process:
   - `ack` → resolve pending callback
   - `error` → reject pending callback
   - `element_added` → (already handled via scene_update)
   - `element_updated` → (already handled via scene_update)
   - `element_removed` → (already handled via scene_update)
2. Parse ack responses and call stored callbacks

**Verification**:
- Open two browser tabs
- Add element in tab 1 → appears in tab 2

---

### Task 5: Add Error Toast UI

**File**: `web/index.html`

**Changes**:
1. Add simple toast notification for errors
2. Show "Failed to sync: {message}" on mutation errors
3. Auto-dismiss after 3 seconds

**Verification**:
- Disconnect server → click add → see error toast

---

## Test Plan

| # | Test | Expected |
|---|------|----------|
| 1 | Click "Add Bar Chart" | WebSocket sends `add_element`, server responds with ack |
| 2 | Refresh page | Chart persists (loaded from server scene) |
| 3 | Open two tabs, add in one | Element appears in both |
| 4 | Kill server, click add | Error toast shown |
| 5 | Console shows message flow | `add_element` → `ack` → `scene_update` |

## Files Modified

- `web/index.html` - Frontend WebSocket mutations

## Dependencies

- Phase 1.1 complete (SceneStore shared) ✅
- Server already handles mutations ✅

## Out of Scope

- Offline queue replay (Phase 1.3)
- Conflict resolution beyond last-write-wins (Future)
- Element selection sync (Future)
