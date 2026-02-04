# TUI PTY Size Mismatch Bug

## Problem

The TUI displays garbled/mispositioned output from Claude because of a size mismatch between:
1. The **vt100 parser** (used to interpret escape sequences for rendering)
2. The **PTY** (what Claude's process sees as its terminal size)

## Root Cause

When Claude runs, it queries its terminal size via the PTY and emits escape sequences (cursor positioning, line wrapping, etc.) based on that size. In the TUI:

1. `PtyManager::new()` creates a PTY with the outer terminal's size (e.g., 80x24 or whatever `terminal::size()` returns)
2. The TUI's content area is smaller (has borders, tab bar taking space)
3. `Tab.resize()` only resizes the vt100 parser, NOT the PTY
4. Claude continues outputting escape sequences for 80x24 but the display area is smaller

This causes:
- Lines appearing at wrong vertical positions
- Text wrapping incorrectly
- Cursor movements landing in wrong places
- General visual corruption

## Screenshot Analysis

From the user's screenshot:
- Multiple `bypass permissions on` lines scattered vertically
- `Transmuting...` status appearing in wrong location
- File paths truncated/mispositioned
- Overall garbled appearance

## Solution

### 1. Add PTY resize to PtyManager

The `portable_pty::MasterPty` trait has a `resize()` method. Expose this:

```rust
impl PtyManager {
    pub fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        if let Some(ref pty_pair) = self.pty_pair {
            pty_pair.master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            }).map_err(...)?;
        }
        Ok(())
    }
}
```

### 2. Update Tab.resize() to resize both vt100 and PTY

```rust
impl Tab {
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.parser.screen_mut().set_size(rows, cols);
        if let Some(ref pty) = self.pty_manager {
            let _ = pty.resize(rows, cols);
        }
    }
}
```

### 3. Pass initial size when creating tabs

Modify `Tab::new()` to accept initial dimensions and resize the PTY immediately after spawning Claude (or create PtyManager with correct size from the start).

Option A: Create PtyManager with TUI content area size
Option B: Resize PTY after creation

Option A is cleaner - add `PtyManager::new_with_size(rows, cols, ...)`.

## Files to Modify

- `src/pty.rs` - Add `resize()` method and optionally `new_with_size()`
- `src/tui/app.rs` - Update `Tab::resize()` and `Tab::new()` to handle PTY sizing

## Testing

1. Run `hydra tui`
2. Verify Claude output renders correctly (no garbled text)
3. Resize the terminal window
4. Verify output continues rendering correctly after resize
