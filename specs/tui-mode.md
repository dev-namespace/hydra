# TUI Mode

Multi-tab interface for running parallel Claude PTY instances.

## User Capabilities

### Entry
- Users can run `hydra tui` to start TUI mode
- Users can run `hydra tui <plan>` to start with a plan file injected (same as `hydra <plan>`)
- TUI starts with one tab already running Claude (with plan if provided)

### Tab Management
- Users can have up to 7 concurrent tabs (numbered 1-7)
- Users can create new tabs with Ctrl+O (spawns Claude with same plan/prompt as initial tab)
- Users can close active tab with F8
- Users can switch tabs with F1 through F7
- Users can cycle tabs with Ctrl+Tab (forward) and Ctrl+Shift+Tab (backward)

### Display
- Minimal top bar shows tabs: `[1] [2*] [3]` with active tab highlighted
- One tab visible at a time (no split panes)
- Tab shows full Claude PTY output in real-time

### Input
- Keyboard input goes to active tab's Claude only
- No broadcast mode - each tab receives input independently

### Lifecycle
- Ctrl+C kills only the active tab's Claude (other tabs unaffected)
- Completed tabs (ALL_TASKS_COMPLETE) stay open until manually closed
- Closed tabs are not restorable

### Configuration
- All tabs share the same prompt, plan, and config
- New tabs inherit the plan file provided at startup (if any)
- No per-tab configuration overrides

## Constraints

### CLI Signature
```
hydra tui [PLAN] [OPTIONS]    # Start TUI mode (plan is optional)
```
Options inherited from main command: `--prompt`, `--max`, `--timeout`, `--verbose`

### Limits
- Maximum 7 tabs
- Tab bar always visible at top
- Keybindings:
  - F1-F7: Switch to tab N
  - Ctrl+Tab: Next tab
  - Ctrl+Shift+Tab: Previous tab
  - Ctrl+O: New tab
  - F8: Close active tab
  - Ctrl+C: Kill active tab's Claude (not exit TUI)
- Exit TUI: Close all tabs or F9

## Architecture

- Built on ratatui (uses crossterm backend, already a dependency)
- Each tab owns one PtyManager instance
- Tab state: number, PtyManager, output buffer, status (running/completed/stopped)
- Main loop: poll keyboard events + poll all PTY outputs
- Render: tab bar + active tab's output buffer

## Related specs

- [Hydra](./hydra.md) - core task runner, PTY management, signal handling

## Source

- [src/tui/mod.rs](../src/tui/mod.rs) - TUI mode entry point
- [src/tui/app.rs](../src/tui/app.rs) - Application state and tab management
- [src/tui/ui.rs](../src/tui/ui.rs) - ratatui rendering (tab bar, content area)
- [src/tui/input.rs](../src/tui/input.rs) - Keyboard event handling
