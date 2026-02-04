# TUI Mode Implementation Plan

## Summary

Add a multi-tab TUI mode to Hydra that allows running parallel Claude PTY instances. Entry via `hydra tui [PLAN]`. Uses ratatui for rendering, supports up to 9 tabs with keyboard-driven management.

## Tasks

- [x] Add ratatui dependency to Cargo.toml
  + `ratatui = "0.28"` (uses crossterm backend, already a dependency)

- [x] Create `src/tui/mod.rs` with module structure
  - Export submodules: app, ui, input
  - Define `run_tui()` entry point that takes config and optional plan path

- [x] Create `src/tui/app.rs` with application state
  - `Tab` struct: id (1-9), pty_manager, output_buffer (Vec<u8>), status (Running/Completed/Stopped)
  - `App` struct: tabs (Vec<Tab>), active_tab_index, shared_config, plan_path
  - Methods: new_tab(), close_tab(), switch_tab(), get_active_tab()
  + See spec: [TUI Mode](../specs/tui-mode.md)

- [x] Create `src/tui/ui.rs` with ratatui rendering
  - Tab bar widget: `[1] [2*] [3]` format, active tab highlighted
  - Content area: render active tab's output buffer
  - Layout: tab bar (1 line height) + content (remaining space)

- [x] Create `src/tui/input.rs` with keyboard handling
  - Ctrl+1-9: switch to tab N (if exists)
  - Ctrl+Tab / Ctrl+Shift+Tab: cycle tabs
  - Ctrl+T: create new tab (if < 9 tabs)
  - Ctrl+W: close active tab
  - Ctrl+C: kill active tab's Claude only
  - Ctrl+Q: exit TUI
  - All other input: forward to active tab's PTY
  + See spec: [TUI Mode - Keybindings](../specs/tui-mode.md#constraints)

- [x] Integrate PTY output polling with ratatui event loop
  - Poll all tab PTYs for output in non-blocking manner
  - Append output to respective tab's buffer
  - Detect stop signals (TASK_COMPLETE, ALL_TASKS_COMPLETE) per tab
  - Update tab status when Claude exits

- [x] Add `tui` subcommand to CLI in `src/main.rs`
  - `hydra tui [PLAN] [OPTIONS]`
  - Accepts same options as main command: --prompt, --max, --timeout, --verbose
  - Calls `tui::run_tui()` with parsed config

- [x] Handle terminal cleanup on exit
  - Restore terminal state (disable raw mode, show cursor)
  - Kill all running Claude processes
  - Reuse existing terminal reset sequence from pty.rs
