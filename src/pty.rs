use crate::error::{HydraError, Result};
use crate::signal::{clear_child_pid, set_child_pid};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{self, disable_raw_mode, enable_raw_mode};
use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Stop signals that Claude outputs to indicate task completion
const TASK_COMPLETE_SIGNAL: &str = "###TASK_COMPLETE###";
const ALL_COMPLETE_SIGNAL: &str = "###ALL_TASKS_COMPLETE###";

/// Strip ANSI escape sequences from a string
/// This is necessary because PTY output contains color codes, cursor movements, etc.
fn strip_ansi_escapes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Start of escape sequence
            match chars.peek() {
                Some('[') => {
                    // CSI sequence: ESC [ ... final_byte
                    chars.next(); // consume '['
                    // Consume parameter bytes (0x30-0x3F) and intermediate bytes (0x20-0x2F)
                    // until we hit the final byte (0x40-0x7E)
                    while let Some(&param) = chars.peek() {
                        if param >= '@' && param <= '~' {
                            chars.next(); // consume final byte
                            break;
                        }
                        chars.next();
                    }
                }
                Some(']') => {
                    // OSC sequence: ESC ] ... (BEL or ESC \)
                    chars.next(); // consume ']'
                    while let Some(c) = chars.next() {
                        if c == '\x07' {
                            break;
                        } // BEL
                        if c == '\x1b' {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                                break;
                            }
                        }
                    }
                }
                Some(&next) if next >= '@' && next <= '_' => {
                    // Fe escape sequence (ESC followed by 0x40-0x5F)
                    chars.next();
                }
                _ => {
                    // Unknown escape or lone ESC, skip it
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Ctrl+C handling state
const CTRL_C_NONE: u8 = 0;
const CTRL_C_FIRST: u8 = 1;
const CTRL_C_SECOND: u8 = 2;

/// Result of the PTY I/O loop
#[derive(Debug, Clone, PartialEq)]
pub enum PtyResult {
    /// Task complete signal detected
    TaskComplete,
    /// All tasks complete signal detected
    AllComplete,
    /// Process exited without signal
    NoSignal,
    /// Terminated by user interrupt
    Terminated,
}

/// Messages sent from the PTY reader thread
enum PtyMessage {
    Data(Vec<u8>),
    Closed,
    Error(String),
}

/// Manages a PTY session for running Claude
pub struct PtyManager {
    pty_pair: PtyPair,
    should_stop: Arc<AtomicBool>,
    ctrl_c_state: Arc<AtomicU8>,
    child_pid: Option<u32>,
}

impl PtyManager {
    /// Create a new PTY manager
    pub fn new(should_stop: Arc<AtomicBool>) -> Result<Self> {
        // Get terminal size
        let (cols, rows) = terminal::size().unwrap_or((80, 24));

        // Create PTY system
        let pty_system = native_pty_system();

        // Create PTY pair with current terminal size
        let pty_pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| HydraError::io("creating PTY pair", io::Error::other(e.to_string())))?;

        Ok(Self {
            pty_pair,
            should_stop,
            ctrl_c_state: Arc::new(AtomicU8::new(CTRL_C_NONE)),
            child_pid: None,
        })
    }

    /// Spawn Claude in the PTY
    pub fn spawn_claude(&mut self, prompt_path: &Path) -> Result<()> {
        // Build command to run Claude
        let mut cmd = CommandBuilder::new("claude");
        cmd.arg("--dangerously-skip-permissions");
        cmd.arg(prompt_path);

        // Set working directory to current directory
        let cwd = std::env::current_dir()
            .map_err(|e| HydraError::io("getting current directory", e))?;
        cmd.cwd(cwd);

        // Spawn the command in the PTY
        let child = self
            .pty_pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| HydraError::io("spawning claude in PTY", io::Error::other(e.to_string())))?;

        // Get the process ID
        if let Some(pid) = child.process_id() {
            self.child_pid = Some(pid);
            set_child_pid(pid);
        }

        // Store the child handle - it will be dropped when PtyManager is dropped
        // which is fine since we track via process_id
        std::mem::forget(child);

        Ok(())
    }

    /// Run the I/O loop, handling input/output and watching for signals
    pub fn run_io_loop(&mut self, output_path: &Path, verbose: bool) -> Result<PtyResult> {
        // Open output file for capturing Claude's output
        let mut output_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(output_path)
            .map_err(|e| HydraError::io("opening output file", e))?;

        // Get PTY reader and writer
        let pty_reader = self
            .pty_pair
            .master
            .try_clone_reader()
            .map_err(|e| HydraError::io("cloning PTY reader", io::Error::other(e.to_string())))?;
        let mut pty_writer = self
            .pty_pair
            .master
            .take_writer()
            .map_err(|e| HydraError::io("taking PTY writer", io::Error::other(e.to_string())))?;

        // Create channel for PTY reader thread to communicate back
        let (tx, rx): (Sender<PtyMessage>, Receiver<PtyMessage>) = mpsc::channel();

        // Spawn a thread to read from PTY (blocking read)
        let reader_should_stop = Arc::clone(&self.should_stop);
        thread::spawn(move || {
            Self::pty_reader_thread(pty_reader, tx, reader_should_stop);
        });

        // Enable raw mode for stdin
        enable_raw_mode().map_err(|e| HydraError::io("enabling raw mode", io::Error::other(e.to_string())))?;

        let result = self.io_loop_inner(
            &mut pty_writer,
            &mut output_file,
            &rx,
            verbose,
        );

        // Disable raw mode
        let _ = disable_raw_mode();

        result
    }

    /// PTY reader thread - reads from PTY and sends data to main thread
    fn pty_reader_thread(
        mut reader: Box<dyn Read + Send>,
        tx: Sender<PtyMessage>,
        should_stop: Arc<AtomicBool>,
    ) {
        let mut buf = [0u8; 4096];

        loop {
            if should_stop.load(Ordering::SeqCst) {
                break;
            }

            match reader.read(&mut buf) {
                Ok(0) => {
                    // EOF - PTY closed
                    let _ = tx.send(PtyMessage::Closed);
                    break;
                }
                Ok(n) => {
                    if tx.send(PtyMessage::Data(buf[..n].to_vec())).is_err() {
                        // Receiver dropped, exit
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(PtyMessage::Error(e.to_string()));
                    break;
                }
            }
        }
    }

    fn io_loop_inner(
        &mut self,
        pty_writer: &mut Box<dyn Write + Send>,
        output_file: &mut File,
        rx: &Receiver<PtyMessage>,
        verbose: bool,
    ) -> Result<PtyResult> {
        let mut stdout = io::stdout();
        let poll_timeout = Duration::from_millis(10);

        // Accumulator for signal detection
        let mut signal_accumulator = String::new();

        loop {
            // Check if we should stop (SIGTERM from external signal)
            if self.should_stop.load(Ordering::SeqCst) {
                self.terminate_child();
                return Ok(PtyResult::Terminated);
            }

            // Check for second Ctrl+C (force quit)
            if self.ctrl_c_state.load(Ordering::SeqCst) >= CTRL_C_SECOND {
                self.force_kill_child();
                eprintln!("\n[hydra] Force quit!");
                std::process::exit(1);
            }

            // Poll for keyboard input
            if event::poll(poll_timeout)
                .map_err(|e| HydraError::io("polling events", io::Error::other(e.to_string())))?
            {
                if let Event::Key(key_event) = event::read()
                    .map_err(|e| HydraError::io("reading event", io::Error::other(e.to_string())))?
                {
                    if let Some(result) = self.handle_key_event(key_event, pty_writer, verbose)? {
                        return Ok(result);
                    }
                }
            }

            // Check for PTY output (non-blocking via try_recv)
            match rx.try_recv() {
                Ok(PtyMessage::Data(data)) => {
                    // Write to stdout
                    stdout
                        .write_all(&data)
                        .map_err(|e| HydraError::io("writing to stdout", e))?;
                    stdout.flush().map_err(|e| HydraError::io("flushing stdout", e))?;

                    // Write to output file
                    output_file
                        .write_all(&data)
                        .map_err(|e| HydraError::io("writing to output file", e))?;
                    output_file
                        .flush()
                        .map_err(|e| HydraError::io("flushing output file", e))?;

                    // Accumulate for signal detection
                    if let Ok(s) = std::str::from_utf8(&data) {
                        signal_accumulator.push_str(s);
                    }

                    // Check for stop signals BEFORE truncation to avoid losing the signal
                    // if there's a lot of output after it in the same chunk
                    let signal_result = self.check_for_signals(&signal_accumulator);

                    // Now truncate to avoid unbounded growth (keep last 8KB for safety)
                    // Claude's TUI can output a lot of formatting after the signal
                    if signal_accumulator.len() > 8192 {
                        let target = signal_accumulator.len() - 4096;
                        // Find the nearest char boundary at or before target
                        let drain_to = (0..=target)
                            .rev()
                            .find(|&i| signal_accumulator.is_char_boundary(i))
                            .unwrap_or(0);
                        signal_accumulator.drain(..drain_to);
                    }
                    if signal_result != PtyResult::NoSignal {
                        if verbose {
                            eprintln!("[hydra:debug] Signal detected, terminating Claude");
                        }
                        println!();
                        match signal_result {
                            PtyResult::AllComplete => {
                                println!("[hydra] All tasks complete signal detected, terminating Claude process...");
                            }
                            PtyResult::TaskComplete => {
                                println!("[hydra] Task complete signal detected, terminating Claude process...");
                            }
                            _ => {}
                        }
                        self.terminate_child();
                        return Ok(signal_result);
                    }
                }
                Ok(PtyMessage::Closed) => {
                    // PTY closed - process exited
                    return Ok(self.check_for_signals(&signal_accumulator));
                }
                Ok(PtyMessage::Error(_)) => {
                    // Error reading from PTY - process likely exited
                    return Ok(self.check_for_signals(&signal_accumulator));
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // No data available, continue
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    // Reader thread exited
                    return Ok(self.check_for_signals(&signal_accumulator));
                }
            }
        }
    }

    /// Handle a key event, returning Some(result) if the loop should exit
    fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        pty_writer: &mut Box<dyn Write + Send>,
        verbose: bool,
    ) -> Result<Option<PtyResult>> {
        // Check for Ctrl+C
        if key_event.code == KeyCode::Char('c')
            && key_event.modifiers.contains(KeyModifiers::CONTROL)
        {
            return Ok(self.handle_ctrl_c(verbose));
        }

        // Check for Ctrl+D
        if key_event.code == KeyCode::Char('d')
            && key_event.modifiers.contains(KeyModifiers::CONTROL)
        {
            // Treat Ctrl+D like Ctrl+C
            return Ok(self.handle_ctrl_c(verbose));
        }

        // Forward other keys to PTY
        let bytes = key_event_to_bytes(&key_event);
        if !bytes.is_empty() {
            pty_writer
                .write_all(&bytes)
                .map_err(|e| HydraError::io("writing to PTY", e))?;
            pty_writer
                .flush()
                .map_err(|e| HydraError::io("flushing PTY", e))?;
        }

        Ok(None)
    }

    /// Handle Ctrl+C press
    fn handle_ctrl_c(&mut self, _verbose: bool) -> Option<PtyResult> {
        let current = self.ctrl_c_state.load(Ordering::SeqCst);

        if current == CTRL_C_NONE {
            // First Ctrl+C - graceful termination
            self.ctrl_c_state.store(CTRL_C_FIRST, Ordering::SeqCst);
            self.should_stop.store(true, Ordering::SeqCst);
            self.terminate_child();
            eprintln!(
                "\n[hydra] Received interrupt, finishing current iteration... (press Ctrl+C again to force quit)"
            );
            Some(PtyResult::Terminated)
        } else {
            // Second Ctrl+C - force quit
            self.ctrl_c_state.store(CTRL_C_SECOND, Ordering::SeqCst);
            self.force_kill_child();
            eprintln!("\n[hydra] Force quit!");
            std::process::exit(1);
        }
    }

    /// Check accumulated output for stop signals
    fn check_for_signals(&self, accumulator: &str) -> PtyResult {
        // Strip ANSI escape sequences before checking for signals
        // PTY output contains color codes and other escape sequences that
        // can be interspersed in the signal text
        let clean = strip_ansi_escapes(accumulator);

        // Check for ALL_COMPLETE first (more specific)
        if clean.contains(ALL_COMPLETE_SIGNAL) {
            return PtyResult::AllComplete;
        }
        if clean.contains(TASK_COMPLETE_SIGNAL) {
            return PtyResult::TaskComplete;
        }
        PtyResult::NoSignal
    }

    /// Terminate the child process gracefully
    fn terminate_child(&self) {
        if let Some(pid) = self.child_pid {
            #[cfg(unix)]
            {
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;
                let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
            }
        }
    }

    /// Force kill the child process
    fn force_kill_child(&self) {
        if let Some(pid) = self.child_pid {
            #[cfg(unix)]
            {
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;
                let _ = kill(Pid::from_raw(pid as i32), Signal::SIGKILL);
            }
        }
    }
}

impl Drop for PtyManager {
    fn drop(&mut self) {
        // Clear the child PID from signal handler
        clear_child_pid();

        // Make sure raw mode is disabled
        let _ = disable_raw_mode();
    }
}

/// Convert a key event to bytes to send to PTY
fn key_event_to_bytes(event: &KeyEvent) -> Vec<u8> {
    let mut bytes = Vec::new();

    match event.code {
        KeyCode::Char(c) => {
            if event.modifiers.contains(KeyModifiers::CONTROL) {
                // Control characters
                if c >= 'a' && c <= 'z' {
                    bytes.push((c as u8) - b'a' + 1);
                } else if c >= 'A' && c <= 'Z' {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_event_to_bytes_char() {
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(&event), b"a");
    }

    #[test]
    fn test_key_event_to_bytes_ctrl_c() {
        let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(key_event_to_bytes(&event), vec![3]); // ETX
    }

    #[test]
    fn test_key_event_to_bytes_enter() {
        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(&event), vec![b'\r']);
    }

    #[test]
    fn test_key_event_to_bytes_arrow() {
        let event = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(&event), b"\x1b[A");
    }

    #[test]
    fn test_pty_result_signal_detection() {
        let manager = PtyManager {
            pty_pair: {
                let pty_system = native_pty_system();
                pty_system
                    .openpty(PtySize {
                        rows: 24,
                        cols: 80,
                        pixel_width: 0,
                        pixel_height: 0,
                    })
                    .unwrap()
            },
            should_stop: Arc::new(AtomicBool::new(false)),
            ctrl_c_state: Arc::new(AtomicU8::new(CTRL_C_NONE)),
            child_pid: None,
        };

        assert_eq!(
            manager.check_for_signals("some output ###TASK_COMPLETE### more"),
            PtyResult::TaskComplete
        );
        assert_eq!(
            manager.check_for_signals("###ALL_TASKS_COMPLETE###"),
            PtyResult::AllComplete
        );
        assert_eq!(
            manager.check_for_signals("no signals here"),
            PtyResult::NoSignal
        );
    }

    #[test]
    fn test_strip_ansi_escapes() {
        // Plain text should pass through unchanged
        assert_eq!(strip_ansi_escapes("hello world"), "hello world");

        // CSI sequences (colors, cursor movement) should be stripped
        assert_eq!(strip_ansi_escapes("\x1b[32mgreen\x1b[0m"), "green");
        assert_eq!(strip_ansi_escapes("\x1b[1;31mbold red\x1b[0m"), "bold red");

        // Multiple sequences
        assert_eq!(
            strip_ansi_escapes("\x1b[32m###\x1b[0mTASK_COMPLETE\x1b[32m###\x1b[0m"),
            "###TASK_COMPLETE###"
        );

        // OSC sequences (title setting, etc.)
        assert_eq!(strip_ansi_escapes("\x1b]0;title\x07text"), "text");

        // Cursor movement
        assert_eq!(strip_ansi_escapes("\x1b[Hstart\x1b[10;20H"), "start");
    }

    #[test]
    fn test_signal_detection_with_ansi_codes() {
        let manager = PtyManager {
            pty_pair: {
                let pty_system = native_pty_system();
                pty_system
                    .openpty(PtySize {
                        rows: 24,
                        cols: 80,
                        pixel_width: 0,
                        pixel_height: 0,
                    })
                    .unwrap()
            },
            should_stop: Arc::new(AtomicBool::new(false)),
            ctrl_c_state: Arc::new(AtomicU8::new(CTRL_C_NONE)),
            child_pid: None,
        };

        // Signal with color codes around it
        assert_eq!(
            manager.check_for_signals("\x1b[32m###TASK_COMPLETE###\x1b[0m"),
            PtyResult::TaskComplete
        );

        // Signal with color codes interspersed
        assert_eq!(
            manager.check_for_signals("output\x1b[1m###ALL_TASKS_COMPLETE###\x1b[0m\n"),
            PtyResult::AllComplete
        );

        // Mixed content with cursor movements
        assert_eq!(
            manager.check_for_signals("\x1b[H\x1b[2JDone!\n\x1b[32m###TASK_COMPLETE###\x1b[0m"),
            PtyResult::TaskComplete
        );
    }
}
