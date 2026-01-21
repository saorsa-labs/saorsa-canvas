# Project State: Saorsa Canvas

## Current Session
- **Date**: 2026-01-21
- **Milestone**: M3 - Beta Distribution
- **Phase**: 1 - Release Workflow
- **Status**: INITIALIZED

## Objective
Make saorsa-canvas a downloadable, installable application for beta testing with Claude Code.

## Interview Decisions

| Question | Decision | Rationale |
|----------|----------|-----------|
| Platforms | macOS + Linux | Cover most developers, Windows can come later |
| Install method | GitHub Release + script | Simple, no external dependencies |
| Integration | MCP server config | Native Claude Code integration |
| Deployment | Local server | Privacy-first, no hosting needed |
| Server mode | Background daemon | Zero friction - runs silently |
| Install features | Binary + MCP config | One-step setup |
| First run | Auto-configure | Zero questions asked |
| Documentation | Quick Start README | Get running in under 2 minutes |
| Package name | saorsa-canvas | Keep current name |
| Build system | GitHub Actions matrix | Automated cross-platform builds |
| Hosting | GitHub Releases | Free, integrated, trusted |

## Architecture

### Install Flow
```
User runs: curl -fsSL https://saorsa.ai/install.sh | bash
  ↓
Script detects OS/arch (macOS arm64/x64, Linux x64)
  ↓
Downloads binary from GitHub Release
  ↓
Installs to ~/.local/bin/saorsa-canvas
  ↓
Creates launchd plist (macOS) or systemd unit (Linux)
  ↓
Configures Claude Code MCP (~/.claude.d/mcp.json or similar)
  ↓
Starts daemon, opens browser to localhost:9473
```

### Directory Structure
```
~/.local/bin/saorsa-canvas          # Binary
~/.local/share/saorsa-canvas/       # Data directory
  ├── web/                          # Static web assets
  ├── config.toml                   # Configuration (auto-generated)
  └── logs/                         # Log files
~/Library/LaunchAgents/             # macOS daemon (com.saorsa.canvas.plist)
~/.config/systemd/user/             # Linux daemon (saorsa-canvas.service)
```

## Phases

### Phase 1: Release Workflow
Create GitHub Actions workflow for cross-platform binary builds.

### Phase 2: Install Script
Create install.sh with OS detection and daemon setup.

### Phase 3: MCP Integration
Auto-configure Claude Code MCP server settings.

### Phase 4: Documentation
Quick Start README with 3-step install.

### Phase 5: Beta Release
Cut v0.2.0-beta.1 release and test end-to-end.

## Current Position
- Milestone: M3 - Beta Distribution
- Phase: 1 - Release Workflow (COMPLETED)
- Phase 2 - Install Script (NEXT)

## Blockers
- None currently

## Context for Next Session
Starting M3 to make saorsa-canvas downloadable for Claude Code beta testing.
All interview decisions recorded above. Ready to implement Phase 1.
