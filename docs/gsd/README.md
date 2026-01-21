# GSD Planning

This directory contains GSD (Get Stuff Done) planning documents for Saorsa Canvas.

## Current Focus
**Milestone 2**: Communitas/saorsa-webrtc Migration - Completing the switch from legacy WebRTC signaling to Communitas-backed calls.

## Structure
- `STATE.json` - Machine-readable state for tooling
- `STATE.md` - Human-readable current position
- `ROADMAP.md` - Milestones and phases overview
- `ISSUES.md` - Deferred work backlog
- `plans/` - Phase-specific task plans
- `specs/` - Technical specifications
- `reviews/` - Phase review reports
- `archive/` - Completed milestone archives

## Commands
- `/gsd` - Start/resume GSD session
- `/gsd:status` - View current state
- `/gsd:plan-phase` - Create new phase plan
- `/gsd:execute-phase` - Execute phase tasks
- `/gsd:review` - Review executed work

## Key Files Being Modified
- `canvas-server/src/sync.rs` - Call state management
- `canvas-server/src/main.rs` - Communitas initialization
- `canvas-server/src/communitas.rs` - MCP client APIs
- `web/index.html` - Frontend call controls
