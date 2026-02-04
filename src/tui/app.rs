//! Application state for TUI mode
//!
//! Manages tabs, each containing a PTY instance running Claude.

use crate::config::Config;
use crate::error::{HydraError, Result};
use crate::prompt::ResolvedPrompt;
use crate::pty::{PtyManager, PtyResult};
use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::{self, JoinHandle};
use tempfile::NamedTempFile;

/// Maximum number of tabs allowed
pub const MAX_TABS: usize = 7;

/// Status of a tab's Claude process
#[derive(Debug, Clone, PartialEq)]
pub enum TabStatus {
    /// Claude is actively running
    Running,
    /// Claude completed with TaskComplete signal
    TaskComplete,
    /// Claude completed with AllTasksComplete signal
    AllComplete,
    /// Claude process was stopped/killed
    Stopped,
}

/// A single tab containing a PTY session
pub struct Tab {
    /// Tab number (1-9)
    pub id: u8,
    /// vt100 terminal parser for interpreting escape sequences
    pub parser: vt100::Parser,
    /// Current status of the tab
    pub status: TabStatus,
    /// PTY manager for this tab (None after PTY closes)
    pty_manager: Option<PtyManager>,
    /// PTY writer for sending input
    pty_writer: Option<Box<dyn Write + Send>>,
    /// Receiver for PTY output from reader thread
    pty_rx: Option<Receiver<PtyMessage>>,
    /// Reader thread handle
    _reader_thread: Option<JoinHandle<()>>,
    /// Stop flag for this tab's PTY
    stop_flag: Arc<AtomicBool>,
}

/// Messages from PTY reader thread
enum PtyMessage {
    Data(Vec<u8>),
    Closed,
}

impl Tab {
    /// Create a new tab and spawn Claude with specified terminal dimensions
    fn new(id: u8, prompt: &ResolvedPrompt, rows: u16, cols: u16) -> Result<Self> {
        let stop_flag = Arc::new(AtomicBool::new(false));
        // Use specified size so Claude sees correct terminal dimensions
        let mut pty_manager = PtyManager::new_with_size(Arc::clone(&stop_flag), rows, cols)?;

        // Create temp file with prompt content
        let mut prompt_file =
            NamedTempFile::new().map_err(|e| HydraError::io("creating temp prompt file", e))?;
        prompt_file
            .write_all(prompt.content.as_bytes())
            .map_err(|e| HydraError::io("writing prompt content", e))?;

        // Spawn Claude in PTY
        pty_manager.spawn_claude(prompt_file.path())?;

        // Get PTY reader and writer
        let (pty_reader, pty_writer) = pty_manager.take_reader_writer()?;

        // Spawn reader thread
        let (tx, rx) = mpsc::channel();
        let reader_stop_flag = Arc::clone(&stop_flag);
        let reader_thread = thread::spawn(move || {
            Self::reader_thread(pty_reader, tx, reader_stop_flag);
        });

        Ok(Self {
            id,
            // Initialize vt100 parser with same dimensions
            parser: vt100::Parser::new(rows, cols, 0),
            status: TabStatus::Running,
            pty_manager: Some(pty_manager),
            pty_writer: Some(pty_writer),
            pty_rx: Some(rx),
            _reader_thread: Some(reader_thread),
            stop_flag,
        })
    }

    /// PTY reader thread - reads output and sends to main thread
    fn reader_thread(
        mut reader: Box<dyn Read + Send>,
        tx: mpsc::Sender<PtyMessage>,
        stop_flag: Arc<AtomicBool>,
    ) {
        let mut buf = [0u8; 4096];

        loop {
            if stop_flag.load(Ordering::SeqCst) {
                break;
            }

            match reader.read(&mut buf) {
                Ok(0) => {
                    let _ = tx.send(PtyMessage::Closed);
                    break;
                }
                Ok(n) => {
                    if tx.send(PtyMessage::Data(buf[..n].to_vec())).is_err() {
                        break;
                    }
                }
                Err(_) => {
                    let _ = tx.send(PtyMessage::Closed);
                    break;
                }
            }
        }
    }

    /// Poll for PTY output (non-blocking)
    fn poll_output(&mut self) -> Result<()> {
        if self.status != TabStatus::Running {
            return Ok(());
        }

        if self.pty_rx.is_none() {
            return Ok(());
        }

        // Collect messages first to avoid borrow issues
        let mut messages = Vec::new();
        let mut should_break = false;

        if let Some(ref rx) = self.pty_rx {
            loop {
                match rx.try_recv() {
                    Ok(msg) => messages.push(msg),
                    Err(TryRecvError::Empty) => {
                        should_break = true;
                        break;
                    }
                    Err(TryRecvError::Disconnected) => {
                        messages.push(PtyMessage::Closed);
                        break;
                    }
                }
            }
        }

        // Process messages
        for msg in messages {
            match msg {
                PtyMessage::Data(data) => {
                    // Feed data through vt100 parser to interpret escape sequences
                    self.parser.process(&data);

                    // Check for stop signals
                    if let Some(result) = self.check_for_signals() {
                        self.status = match result {
                            PtyResult::TaskComplete => TabStatus::TaskComplete,
                            PtyResult::AllComplete => TabStatus::AllComplete,
                            _ => TabStatus::Stopped,
                        };
                        self.stop_pty();
                    }
                }
                PtyMessage::Closed => {
                    if self.status == TabStatus::Running {
                        self.status = TabStatus::Stopped;
                    }
                }
            }
        }

        let _ = should_break; // suppress unused warning
        Ok(())
    }

    /// Check screen contents for stop signals
    fn check_for_signals(&self) -> Option<PtyResult> {
        const TASK_COMPLETE: &str = "###TASK_COMPLETE###";
        const ALL_COMPLETE: &str = "###ALL_TASKS_COMPLETE###";

        // Get plain text contents from the vt100 screen
        let contents = self.parser.screen().contents();

        if contents.contains(ALL_COMPLETE) {
            return Some(PtyResult::AllComplete);
        }
        if contents.contains(TASK_COMPLETE) {
            return Some(PtyResult::TaskComplete);
        }
        None
    }

    /// Send input to the PTY
    fn send_input(&mut self, data: &[u8]) -> Result<()> {
        if let Some(ref mut writer) = self.pty_writer {
            writer
                .write_all(data)
                .map_err(|e| HydraError::io("writing to PTY", e))?;
            writer
                .flush()
                .map_err(|e| HydraError::io("flushing PTY", e))?;
        }
        Ok(())
    }

    /// Stop this tab's Claude process
    fn stop_pty(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Some(ref pty) = self.pty_manager {
            pty.terminate_child();
        }
        self.pty_writer = None;
        self.pty_rx = None;
    }

    /// Kill this tab's Claude process (Ctrl+C behavior)
    pub fn kill(&mut self) {
        self.stop_pty();
        self.status = TabStatus::Stopped;
    }

    /// Resize the vt100 parser and PTY to match new terminal dimensions
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.parser.screen_mut().set_size(rows, cols);
        // Also resize the PTY so Claude knows the new terminal size
        if let Some(ref pty) = self.pty_manager {
            let _ = pty.resize(rows, cols);
        }
    }
}

impl Drop for Tab {
    fn drop(&mut self) {
        self.stop_pty();
    }
}

/// Main application state
pub struct App {
    /// All tabs
    pub tabs: Vec<Tab>,
    /// Currently active tab index
    pub active_tab_index: usize,
    /// Shared configuration
    config: Config,
    /// Shared prompt (used when creating new tabs)
    prompt: ResolvedPrompt,
    /// Current content area dimensions (rows, cols)
    content_size: (u16, u16),
}

impl App {
    /// Create a new App and spawn the initial tab with specified content area dimensions
    pub fn new(config: Config, prompt: ResolvedPrompt, rows: u16, cols: u16) -> Result<Self> {
        let mut app = Self {
            tabs: Vec::with_capacity(MAX_TABS),
            active_tab_index: 0,
            config,
            prompt,
            content_size: (rows, cols),
        };

        // Spawn initial tab
        app.new_tab()?;

        Ok(app)
    }

    /// Create a new tab (if under limit)
    pub fn new_tab(&mut self) -> Result<bool> {
        if self.tabs.len() >= MAX_TABS {
            return Ok(false);
        }

        let id = (self.tabs.len() + 1) as u8;
        let (rows, cols) = self.content_size;
        let tab = Tab::new(id, &self.prompt, rows, cols)?;
        self.tabs.push(tab);
        self.active_tab_index = self.tabs.len() - 1;

        Ok(true)
    }

    /// Close the currently active tab
    pub fn close_active_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }

        // Remove the tab (Drop will clean up PTY)
        self.tabs.remove(self.active_tab_index);

        // Renumber remaining tabs
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            tab.id = (i + 1) as u8;
        }

        // Adjust active index
        if !self.tabs.is_empty() && self.active_tab_index >= self.tabs.len() {
            self.active_tab_index = self.tabs.len() - 1;
        }
    }

    /// Switch to tab by number (1-9)
    pub fn switch_to_tab(&mut self, num: u8) {
        let index = (num - 1) as usize;
        if index < self.tabs.len() {
            self.active_tab_index = index;
        }
    }

    /// Cycle to next tab
    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_tab_index = (self.active_tab_index + 1) % self.tabs.len();
        }
    }

    /// Cycle to previous tab
    pub fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_tab_index = if self.active_tab_index == 0 {
                self.tabs.len() - 1
            } else {
                self.active_tab_index - 1
            };
        }
    }

    /// Get the active tab (if any)
    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active_tab_index)
    }

    /// Get the active tab mutably (if any)
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active_tab_index)
    }

    /// Check if there are no tabs
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// Poll all PTYs for output
    pub fn poll_pty_output(&mut self) -> Result<()> {
        for tab in &mut self.tabs {
            tab.poll_output()?;
        }
        Ok(())
    }

    /// Send input to the active tab's PTY
    pub fn send_input(&mut self, data: &[u8]) -> Result<()> {
        if let Some(tab) = self.active_tab_mut() {
            tab.send_input(data)?;
        }
        Ok(())
    }

    /// Kill the active tab's Claude process (Ctrl+C behavior)
    pub fn kill_active_tab(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            tab.kill();
        }
    }

    /// Get config reference
    #[allow(dead_code)]
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Resize all tabs' vt100 parsers and PTYs to match new terminal dimensions
    pub fn resize_all(&mut self, rows: u16, cols: u16) {
        // Store size for new tabs
        self.content_size = (rows, cols);
        // Resize existing tabs
        for tab in &mut self.tabs {
            tab.resize(rows, cols);
        }
    }
}
