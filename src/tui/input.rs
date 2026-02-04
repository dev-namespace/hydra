//! Input handling for TUI mode
//!
//! Handles keyboard events: tab management and input forwarding.

use crate::error::Result;
use crate::tui::app::App;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Result of handling a key event
pub enum KeyAction {
    /// Continue running
    Continue,
    /// Exit the TUI
    Exit,
}

/// Handle a key event
///
/// Returns KeyAction to indicate whether to continue or exit
pub fn handle_key_event(app: &mut App, event: KeyEvent) -> Result<KeyAction> {
    // F-key bindings for tab management
    match event.code {
        // F1-F7: Switch to tab 1-7
        KeyCode::F(n) if (1..=7).contains(&n) => {
            app.switch_to_tab(n);
            return Ok(KeyAction::Continue);
        }

        // F8: Close active tab
        KeyCode::F(8) => {
            app.close_active_tab();
            return Ok(KeyAction::Continue);
        }

        // F9: Exit TUI
        KeyCode::F(9) => {
            return Ok(KeyAction::Exit);
        }

        _ => {}
    }

    // Ctrl+... keybindings
    if event.modifiers.contains(KeyModifiers::CONTROL) {
        match event.code {
            // Ctrl+O: New tab
            KeyCode::Char('o') => {
                let _ = app.new_tab();
                return Ok(KeyAction::Continue);
            }

            // Ctrl+C: Kill active tab's Claude (not exit TUI)
            KeyCode::Char('c') => {
                app.kill_active_tab();
                return Ok(KeyAction::Continue);
            }

            // Ctrl+Tab: Next tab (Note: Some terminals don't support this)
            KeyCode::Tab => {
                if event.modifiers.contains(KeyModifiers::SHIFT) {
                    app.prev_tab();
                } else {
                    app.next_tab();
                }
                return Ok(KeyAction::Continue);
            }

            _ => {}
        }
    }

    // Ctrl+Shift+Tab: Previous tab
    if event
        .modifiers
        .contains(KeyModifiers::CONTROL | KeyModifiers::SHIFT)
        && event.code == KeyCode::BackTab
    {
        app.prev_tab();
        return Ok(KeyAction::Continue);
    }

    // Forward other keys to active tab's PTY
    let bytes = key_event_to_bytes(&event);
    if !bytes.is_empty() {
        app.send_input(&bytes)?;
    }

    Ok(KeyAction::Continue)
}

/// Convert a key event to bytes to send to PTY
fn key_event_to_bytes(event: &KeyEvent) -> Vec<u8> {
    let mut bytes = Vec::new();

    match event.code {
        KeyCode::Char(c) => {
            if event.modifiers.contains(KeyModifiers::CONTROL) {
                // Control characters (already handled above for tab management)
                if c.is_ascii_lowercase() {
                    bytes.push((c as u8) - b'a' + 1);
                } else if c.is_ascii_uppercase() {
                    bytes.push((c as u8) - b'A' + 1);
                }
            } else if event.modifiers.contains(KeyModifiers::ALT) {
                // Alt + char = ESC + char
                bytes.push(0x1b);
                bytes.extend(c.to_string().as_bytes());
            } else {
                bytes.extend(c.to_string().as_bytes());
            }
        }
        KeyCode::Enter => bytes.push(b'\r'),
        KeyCode::Tab => bytes.push(b'\t'),
        KeyCode::Backspace => bytes.push(0x7f),
        KeyCode::Esc => bytes.push(0x1b),
        KeyCode::Up => bytes.extend(b"\x1b[A"),
        KeyCode::Down => bytes.extend(b"\x1b[B"),
        KeyCode::Right => bytes.extend(b"\x1b[C"),
        KeyCode::Left => bytes.extend(b"\x1b[D"),
        KeyCode::Home => bytes.extend(b"\x1b[H"),
        KeyCode::End => bytes.extend(b"\x1b[F"),
        KeyCode::PageUp => bytes.extend(b"\x1b[5~"),
        KeyCode::PageDown => bytes.extend(b"\x1b[6~"),
        KeyCode::Insert => bytes.extend(b"\x1b[2~"),
        KeyCode::Delete => bytes.extend(b"\x1b[3~"),
        KeyCode::F(n) => {
            let seq = match n {
                1 => b"\x1bOP".to_vec(),
                2 => b"\x1bOQ".to_vec(),
                3 => b"\x1bOR".to_vec(),
                4 => b"\x1bOS".to_vec(),
                5 => b"\x1b[15~".to_vec(),
                6 => b"\x1b[17~".to_vec(),
                7 => b"\x1b[18~".to_vec(),
                8 => b"\x1b[19~".to_vec(),
                9 => b"\x1b[20~".to_vec(),
                10 => b"\x1b[21~".to_vec(),
                11 => b"\x1b[23~".to_vec(),
                12 => b"\x1b[24~".to_vec(),
                _ => vec![],
            };
            bytes.extend(seq);
        }
        _ => {}
    }

    bytes
}
