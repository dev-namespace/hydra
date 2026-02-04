//! TUI mode - Multi-tab interface for running parallel Claude PTY instances
//!
//! Entry point via `hydra tui [PLAN]`. Uses ratatui for rendering with crossterm backend.

mod app;
mod input;
mod ui;

use crate::config::Config;
use crate::error::{HydraError, Result};
use crate::prompt::ResolvedPrompt;
use app::App;
use crossterm::event::{self, Event};
use crossterm::terminal::{self, disable_raw_mode, enable_raw_mode};
use input::{KeyAction, handle_key_event};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, Write};
use std::time::Duration;

/// Run the TUI mode with the given configuration and optional resolved prompt
pub fn run_tui(config: Config, prompt: ResolvedPrompt) -> Result<()> {
    // Setup terminal
    enable_raw_mode()
        .map_err(|e| HydraError::io("enabling raw mode", io::Error::other(e.to_string())))?;

    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )
    .map_err(|e| HydraError::io("entering alternate screen", io::Error::other(e.to_string())))?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)
        .map_err(|e| HydraError::io("creating terminal", io::Error::other(e.to_string())))?;

    // Get initial terminal size and calculate content area
    let term_size = terminal
        .size()
        .map_err(|e| HydraError::io("getting terminal size", io::Error::other(e.to_string())))?;
    let (rows, cols) = calculate_content_area(term_size.width, term_size.height);

    // Create application state and spawn initial tab with correct dimensions
    let mut app = App::new(config, prompt, rows, cols)?;

    // Main event loop
    let result = run_event_loop(&mut terminal, &mut app);

    // Cleanup terminal
    cleanup_terminal(&mut terminal);

    result
}

/// Calculate the content area dimensions (inside the content block borders)
fn calculate_content_area(terminal_width: u16, terminal_height: u16) -> (u16, u16) {
    // Layout: tab bar (3 lines including borders) + content (remaining)
    let content_height = terminal_height.saturating_sub(3); // Subtract tab bar height

    // Content area is inside a Block with borders, so subtract 2 for top/bottom borders
    let inner_height = content_height.saturating_sub(2);
    // Subtract 2 for left/right borders
    let inner_width = terminal_width.saturating_sub(2);

    (inner_height.max(1), inner_width.max(1))
}

/// Main event loop - polls keyboard events and PTY output
fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let poll_timeout = Duration::from_millis(10);

    // Track previous content area size for resize detection
    let mut prev_content_size: Option<(u16, u16)> = None;

    loop {
        // Get current terminal size and calculate content area
        let term_size = terminal.size().map_err(|e| {
            HydraError::io("getting terminal size", io::Error::other(e.to_string()))
        })?;
        let content_size = calculate_content_area(term_size.width, term_size.height);

        // Resize vt100 parsers if content area changed
        if prev_content_size != Some(content_size) {
            app.resize_all(content_size.0, content_size.1);
            prev_content_size = Some(content_size);
        }

        // Render current state
        terminal
            .draw(|frame| ui::render(frame, app))
            .map_err(|e| HydraError::io("drawing frame", io::Error::other(e.to_string())))?;

        // Poll for events
        if event::poll(poll_timeout)
            .map_err(|e| HydraError::io("polling events", io::Error::other(e.to_string())))?
        {
            match event::read()
                .map_err(|e| HydraError::io("reading event", io::Error::other(e.to_string())))?
            {
                Event::Key(key_event) => {
                    // Handle key event (may create/close/switch tabs or forward to PTY)
                    match handle_key_event(app, key_event)? {
                        KeyAction::Exit => return Ok(()),
                        KeyAction::Continue => {}
                    }

                    // Check if all tabs are closed
                    if app.is_empty() {
                        return Ok(());
                    }
                }
                Event::Resize(width, height) => {
                    // Terminal was resized - update content area size
                    let new_content_size = calculate_content_area(width, height);
                    if prev_content_size != Some(new_content_size) {
                        app.resize_all(new_content_size.0, new_content_size.1);
                        prev_content_size = Some(new_content_size);
                    }
                }
                _ => {} // Ignore other events (mouse, focus, paste)
            }
        }

        // Poll all PTYs for output
        app.poll_pty_output()?;
    }
}

/// Restore terminal to normal state
fn cleanup_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) {
    let _ = disable_raw_mode();
    let _ = crossterm::execute!(
        terminal.backend_mut(),
        terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    );
    let _ = terminal.show_cursor();

    // Comprehensive reset sequence
    let reset_sequence = concat!(
        "\x1b[?2004l", // Disable bracketed paste mode
        "\x1b[?1049l", // Exit alternate screen buffer (fallback)
        "\x1b[?25h",   // Show cursor
        "\x1b[0m",     // Reset attributes
    );

    let mut stdout = io::stdout();
    let _ = stdout.write_all(reset_sequence.as_bytes());
    let _ = stdout.flush();
}
