# Saorsa Canvas — Semantics Layer Plan (Rail B)
**Path in repo:** `docs/SEMANTICS_PLAN.md`  
**Status:** Draft v0 (team handoff)  
**Purpose:** Make Saorsa Canvas a “pixels + semantics + actions” interface so humans and AIs can operate a fully-rendered canvas UI without a traditional GUI toolkit.

---

## 0. Summary

Saorsa Canvas remains a GPU-rendered, fully addressable surface, and adds a first-class **Semantics Layer**:

- **Rail A — Pixels:** WGPU renders the UI.
- **Rail B — Semantics:** a canonical Semantics Tree (roles, names, states, bounds, children).
- **Rail C — Actions:** `act(node_id, action, args)` is the primary control path; pixel clicks are fallback.

Canvas-rendered UIs need explicit semantics to be accessible and reliably automatable. We align our semantics vocabulary to **WAI-ARIA**, and our platform exposure to OS accessibility APIs via **AccessKit** (native) and an **ARIA DOM mirror** (web).

---

## 1. Goals and non-goals

### Goals
1. **Stable node identity** across frames for AI control, automation, and replay.
2. **Deterministic semantics**: same state produces the same tree (modulo ordering rules).
3. **Primary actuation**: `act(node_id, ...)` works for all core UI flows (no pixel clicks needed for the golden demo).
4. **Accessibility-ready**
   - Web: ARIA-mirrored DOM subtree for screen readers and keyboard nav.
   - Native: AccessKit tree bridged to platform accessibility APIs.
5. **Incremental updates**: stream deltas, not full trees, over WebSocket/MCP.
6. **Debuggability**: inspector overlay, record/replay, golden snapshots.
7. **Trust boundaries**: provenance tags + “trusted shell overlays” for sensitive UI.

### Non-goals (v0)
- Full-feature text editing (complex selection, IME, bidi, rich text).
- Complete ARIA role coverage (we start with a focused subset).
- A fully general layout engine spec (we only need stable bounds, ordering, and hit regions).

---

## 2. Reference model (standards and patterns)

### Semantics vocabulary
- **WAI-ARIA**: roles, states, and properties for custom widgets.
- **APG**: canonical keyboard interaction patterns and widget behaviour expectations.
- **ARIA in HTML**: constraints on valid role/attribute combinations (for web mirror correctness).

### Platform mapping model
- **Core-AAM**: mapping ARIA roles/states/properties to platform accessibility APIs.
- **HTML-AAM**: mapping rules for HTML + ARIA; helpful for DOM mirror decisions.
- **ACCNAME**: accessible name/description computation model; v0 uses a simplified-compatible rule set.

### Native patterns
- **Microsoft UI Automation** is pattern-based (Invoke/Toggle/Value/Scroll/ExpandCollapse/Selection). Our `Action` set mirrors these capabilities conceptually.
- **AccessKit** provides cross-platform accessibility schema and adapters; `accesskit_winit` exposes the tree through platform-native APIs.

---

## 3. Architecture overview

### 3.1 Data flows

**Render loop**
1. Modules/connectors produce a `SceneGraph` (draw ops + layout + semantic annotations).
2. Canvas renders pixels.
3. Canvas updates:
   - `SemanticsTree` (canonical meaning)
   - `HitTestIndex` (x,y → node_id)
4. Canvas publishes `SemanticsDelta` (and optional downscaled image thumbnail) to clients.

**Input loop**
1. Human input events arrive (pointer/touch/keyboard/text input).
2. Runtime hit-tests to resolve x,y → `node_id` and emits `InteractionEvent`.
3. Orchestrator/AI decides:
   - `act(node_id, action, args)` (preferred)
   - pixel `click(x,y)` fallback only when semantics are absent/mismatched.

### 3.2 Crate responsibilities (planned)

- `canvas-core`
  - `semantics` module (tree/nodes/deltas/hit testing)
  - action routing (`ActionRouter`)
  - focus manager and keyboard routing
  - provenance / trust model
- `canvas-renderer`
  - draw ops + layout → semantic bounds
  - optional debug overlay (bounds + IDs + roles)
- `canvas-server`
  - WebSocket protocol for deltas/events/actions
- `canvas-mcp`
  - MCP tools for semantics snapshot/delta, inspect, and actions
- `web/`
  - ARIA DOM mirror + event routing back to `act`
- `canvas-app` / `canvas-desktop`
  - native window host + AccessKit integration

---

## 4. Core invariants (must be enforced)

### 4.1 Stable node identity
If node IDs change every frame, AI control and accessibility break down.

**Rule:** `node_id` must be provided by the producer (module/connector/shell) and be stable across frames for the “same” element.

**Recommended format (string IDs):**
- `"<namespace>:<entity-id>:<sub-id>"`
  - Examples:
    - `chat:channel:general`
    - `chat:msg:01JABC...`
    - `shell:composer:textbox`
    - `shell:dialog:confirm-send:ok`

### 4.2 Tree invariants
- Exactly one root node per session.
- Every node is reachable from the root.
- Children order is deterministic and matches navigation order.
- Bounds are in logical canvas coordinates; DPI scaling is applied separately.

---

## 5. Data model (Rust types)

> Location: `canvas-core/src/semantics/mod.rs` (or `canvas-core/src/semantics.rs`)

### 5.1 Roles (v0 subset, ARIA-aligned)
```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Role {
    Window,
    Document,
    Heading,
    Paragraph,
    Text,
    Image,
    Link,

    Button,
    Checkbox,
    Switch,
    Radio,
    Textbox,
    Searchbox,

    List,
    ListItem,

    Grid,
    Row,
    Cell,

    TabList,
    Tab,
    TabPanel,

    Menu,
    MenuItem,

    Dialog,
    Alert,

    ProgressBar,
    Slider,

    ScrollView,
    Separator,
}
```

### 5.2 States and properties (v0)
```rust
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct State {
    pub disabled: bool,
    pub hidden: bool,
    pub focused: bool,

    pub selected: Option<bool>,
    pub expanded: Option<bool>,
    pub checked: Option<CheckedState>,

    pub read_only: bool,
    pub busy: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CheckedState {
    False,
    True,
    Mixed,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Props {
    pub name: Option<String>,        // accessible name
    pub description: Option<String>, // accessible description
    pub value: Option<Value>,        // textbox/slider/progress
    pub placeholder: Option<String>, // textbox hint
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Value {
    Text(String),
    Number(f64),
    Percent(f32),
}
```

### 5.3 Geometry
```rust
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
```

### 5.4 Actions (v0)
Our action set mirrors UIA’s conceptual patterns: invoke/toggle/value/scroll/expand-collapse/selection.
```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Action {
    Invoke,
    Toggle,
    Focus,
    Blur,

    SetValue,        // args: Value
    SetSelection,    // args: selection model
    Expand,
    Collapse,

    ScrollTo,        // args: (x?, y?)
    ScrollBy,        // args: (dx, dy)
}
```

### 5.5 Semantics nodes and tree
```rust
pub type NodeId = String;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Origin {
    pub namespace: String,     // "shell", "chat", "calendar", "drive", ...
    pub module: String,        // module identifier (connector name, plugin ID, etc.)
    pub trust: TrustLevel,     // enforced by runtime
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TrustLevel {
    TrustedShell,
    SignedModule,
    UntrustedContent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SemanticsNode {
    pub id: NodeId,
    pub role: Role,
    pub bounds: Rect,
    pub children: Vec<NodeId>,
    pub state: State,
    pub props: Props,
    pub actions: Vec<Action>,
    pub origin: Origin,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SemanticsTree {
    pub root: NodeId,
    pub nodes: HashMap<NodeId, SemanticsNode>,
    pub focus: Option<NodeId>,
    pub revision: u64,
}
```

---

## 6. Accessible name/description strategy (v0)

We implement a deterministic rule set that is compatible with ACCNAME principles, without implementing every edge case in v0.

### 6.1 Name priority order
1. `props.name` if present (explicit).
2. If role is `Textbox` and `props.placeholder` exists and no explicit name: use placeholder as fallback name (mark as fallback in debug, optional).
3. Concatenate direct visible text children (Role::Text) in child order.
4. Else empty.

### 6.2 Description priority order
1. `props.description` if present.
2. Else empty.

---

## 7. Hit testing and focus

### 7.1 HitTestIndex
A spatial index mapping pointer coordinates → `node_id`:
- respects z-order and clipping
- returns deepest/topmost node + ancestry path
- is updated whenever bounds change

**Implementation v0:** grid bins.  
**Upgrade later:** R-tree if needed.

Provide a debug API:
- `inspect(x,y) -> { node_id, path, role, name, actions }`

### 7.2 Focus manager
- Track `tree.focus`.
- Keyboard events route to focused node.
- Tab traversal order:
  - Start with tree order of focusable nodes.
  - Add explicit tab index later.

---

## 8. Deltas (streaming semantics)

### 8.1 Delta format
```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SemanticsDelta {
    pub base_revision: u64,
    pub new_revision: u64,
    pub upsert: Vec<SemanticsNode>,
    pub remove: Vec<NodeId>,
    pub root: Option<NodeId>,
    pub focus_changed: Option<Option<NodeId>>,
}
```

### 8.2 Delta invariants
- Apply only if `base_revision` matches; otherwise request snapshot.
- Revision increments monotonically.

---

## 9. WebSocket protocol (canvas-server)

### 9.1 Server → client
- `semantics_snapshot { tree }`
- `semantics_delta { delta }`
- `interaction_event { event }`
- `action_result { result }`

### 9.2 Client → server
- `subscribe { want: ["semantics","events"] }`
- `request_snapshot { since_revision? }`
- `act { node_id, action, args, correlation_id }`
- `inspect { x, y }`

### 9.3 InteractionEvent
```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InteractionEvent {
    pub node_id: Option<NodeId>,
    pub kind: InteractionKind,
    pub pointer: Option<PointerEvent>,
    pub key: Option<KeyEvent>,
    pub timestamp_ms: u64,
    pub modifiers: Modifiers,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum InteractionKind {
    PointerDown,
    PointerUp,
    PointerMove,
    Scroll,
    KeyDown,
    KeyUp,
    TextInput,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PointerEvent {
    pub x: f32,
    pub y: f32,
    pub button: Option<String>,
    pub pointer_type: String, // "mouse" | "touch" | "pen"
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyEvent {
    pub key: String,
    pub code: String,
}
```

---

## 10. MCP tools (canvas-mcp)

### 10.1 Tools (v0)
1. `canvas_get_semantics(session_id, since_revision?) -> {tree|delta}`
2. `canvas_act(session_id, node_id, action, args) -> action_result`
3. `canvas_inspect(session_id, x, y) -> {node_id, path, node, actions}`
4. `canvas_subscribe(session_id) -> stream {semantics_delta, interaction_event}`
5. `canvas_export(session_id, format) -> bytes|url` (existing)

### 10.2 Rule: prefer `act` over pixel clicks
Agents should use `canvas_act` unless semantics are missing or inconsistent.

---

## 11. Web client: ARIA DOM mirror (web/)

Pixels remain the visible surface. The DOM mirror exists for:
- screen readers
- keyboard navigation
- test automation

### 11.1 DOM mirror strategy
- Maintain a visually hidden container (but accessible to assistive tech).
- For each semantics node, create/update a DOM element with:
  - `role=...`
  - `aria-*` attributes for state/value
  - accessible name via `aria-label` (or content)
- Position DOM element bounds to match canvas bounds.

### 11.2 Event routing
DOM events map back to canvas actions:
- click → `Invoke`
- checkbox change → `Toggle`
- input change → `SetValue`
- disclosure → `Expand/Collapse`
- list selection → `SetSelection`

Keyboard handling should follow APG patterns for the widget roles used.

---

## 12. Native desktop: AccessKit integration (canvas-app / canvas-desktop)

### 12.1 Why AccessKit
AccessKit provides a cross-platform accessibility schema and adapters for toolkits that render their own UI elements.

### 12.2 Integration steps
1. Create the winit window initially **invisible** (`WindowAttributes::with_visible(false)`).
2. Create `accesskit_winit::Adapter` before showing the window.
3. Show the window.
4. On each semantics update: translate our `SemanticsTree` to AccessKit nodes and update the adapter.
5. Route AccessKit action requests into our `ActionRouter`.

### 12.3 Known risk
A winit issue reports that `with_visible(false)` may be ignored on Windows in certain versions; test this early and apply workaround if needed.

---

## 13. Security and trust boundaries (mandatory)

If AI can render anything, pixels alone are spoofable. Enforce trust with provenance:

### 13.1 Provenance tags
Every node carries:
- `origin.namespace`
- `origin.module`
- `origin.trust`

### 13.2 Trusted shell overlays
Only `TrustLevel::TrustedShell` may create:
- passphrase / credential prompts
- signing / approvals / payments
- permission grants

The shell draws a non-spoofable visual affordance (border/watermark) for these views.

### 13.3 Action gating
Runtime enforces:
- untrusted nodes cannot trigger privileged operations directly
- privileged operations require shell-mediated confirmation

---

## 14. Golden demo UI (acceptance target)

Implement a minimal Communitas-like demo purely to validate semantics and actions:

- Channel list (`List` + `ListItem`)
- Message timeline (`List` with virtualisation placeholder, v0 can be non-virtual)
- Composer (`Textbox` + `Button`)
- Reactions (`Toggle`) on messages
- Expandable thread (`Expand/Collapse`)

### Success criteria
- The demo UI is fully operable via `act()` alone.
- No critical flows require pixel clicks.
- Semantics tree has meaningful names and roles.
- Focus and basic keyboard navigation work.

---

## 15. Testing strategy

### 15.1 Unit tests (canvas-core)
- Tree invariants: reachability, deterministic order.
- Hit testing: point → node correctness for synthetic scenes.
- Delta replay: applying deltas yields expected snapshots.

### 15.2 Golden snapshots
- `semantics_snapshot.json` at key states
- `interaction_trace.json` (events + actions + results)
CI asserts snapshot stability (or approved diffs).

### 15.3 Accessibility smoke tests
- Web: validate roles/names exist via browser accessibility tree inspection tools.
- Native:
  - Windows: UIA clients see and can invoke/toggle/set value on basic controls.
  - macOS: elements/actions visible in Accessibility Inspector.

---

## 16. Work packages (AI team tasks)

### WP-A — Semantics core (canvas-core)
Deliver:
- roles/states/props/actions
- `SemanticsTree` + `SemanticsDelta`
- hit testing + `inspect`
- `ActionRouter` + `act`

Accept:
- demo UI operable via `act()` only

### WP-B — Server protocol (canvas-server)
Deliver:
- WS messages for snapshot/delta/events/actions
- correlation IDs, error handling

### WP-C — Web ARIA mirror (web/)
Deliver:
- DOM mirror + ARIA mapping
- DOM events routed to `act`
- basic keyboard patterns for list/tabs/dialogs (APG-aligned)

### WP-D — Native AccessKit (canvas-app / canvas-desktop)
Deliver:
- accesskit_winit adapter integration
- tree translation
- action requests routed to `ActionRouter`

### WP-E — Tooling and inspector
Deliver:
- debug overlay (bounds + role + name + node_id)
- inspector panel (tree browser + node details)
- record/replay harness

### WP-F — Security provenance
Deliver:
- provenance tags
- trusted overlay enforcement
- privileged action gating

---

## 17. Risks and mitigations

1. **Text editing (IME/selection/clipboard) complexity**
   - Keep v0 textbox minimal; build semantics + action plumbing first.

2. **Window visibility requirement on Windows**
   - Test early; workaround if `with_visible(false)` is ignored.

3. **ARIA misuse in DOM mirror**
   - Validate role/attribute combos; follow APG patterns and ARIA-in-HTML constraints.

4. **Node ID instability**
   - Require node_id from producers; add debug lint checks.

---

## 18. Open questions (resolve before implementation start)

1. NodeId type: string (recommended) vs u64.
2. Coordinate system: origin top-left; define DPI scaling and input normalisation.
3. Virtualised lists: represent only realised items vs range metadata; define scroll semantics.
4. Trust affordance: exact visual language and enforcement mechanism.

---

## 19. Deliverables checklist (definition of done for v0)

- [ ] `canvas-core::semantics` implemented with tests
- [ ] WS protocol supports semantics snapshot + deltas + actions
- [ ] `canvas-mcp` exposes semantics + act + inspect + subscribe
- [ ] Web client has ARIA mirror and routes events back
- [ ] Native desktop integrates AccessKit and passes smoke tests
- [ ] Demo UI operable via `act()` with golden traces
- [ ] Provenance tags and trusted overlay rules enforced

---

## 20. References (primary)
- WAI-ARIA 1.2 (roles/states/properties)
- ARIA Authoring Practices Guide (APG)
- Developing a Keyboard Interface (APG)
- ARIA in HTML
- Core Accessibility API Mappings (Core-AAM)
- HTML Accessibility API Mappings (HTML-AAM)
- Accessible Name & Description Computation (ACCNAME)
- Microsoft UI Automation Control Patterns / Overview
- AccessKit (project, docs.rs) and accesskit_winit adapter documentation
- winit issue: `WindowAttributes.with_visible(false)` ignored on Windows
