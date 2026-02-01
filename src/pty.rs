use crate::error::{HydraError, Result};
use crate::signal::{clear_child_pid, set_child_pid};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{self, disable_raw_mode, enable_raw_mode};
use portable_pty::{native_pty_system, Child, CommandBuilder, PtyPair, PtySize};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Debug log to file (since terminal may be frozen)
fn debug_log(msg: &str) {
    use std::fs::OpenOptions;
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/hydra-debug.log")
    {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let _ = writeln!(f, "[{}] {}", timestamp, msg);
    }
}

/// Restore terminal to normal mode with fallback reset sequence
/// This is more robust than just calling disable_raw_mode()
fn restore_terminal(verbose: bool) {
    debug_log("restore_terminal: starting");

    // First, try the normal crossterm disable_raw_mode
    match disable_raw_mode() {
        Ok(_) => debug_log("restore_terminal: disable_raw_mode succeeded"),
        Err(e) => debug_log(&format!("restore_terminal: disable_raw_mode failed: {}", e)),
    }

    // Comprehensive reset sequence that:
    // 1. XON to resume if XOFF stopped the terminal
    // 2. Cancels any partial escape sequence (CAN character)
    // 3. Disables all mouse tracking modes
    // 4. Disables bracketed paste mode
    // 5. Disables focus reporting
    // 6. Exits alternate screen buffer
    // 7. Resets all terminal modes
    let reset_sequence = concat!(
        "\x11",         // XON (Ctrl+Q) - resume if XOFF stopped terminal
        "\x18",         // CAN - cancel any partial escape sequence
        "\x1b[?2026l",  // Disable synchronized output (used by Claude TUI)
        "\x1b[?1000l",  // Disable mouse click tracking
        "\x1b[?1002l",  // Disable mouse button tracking
        "\x1b[?1003l",  // Disable mouse any-event tracking
        "\x1b[?1006l",  // Disable SGR mouse mode
        "\x1b[?1015l",  // Disable urxvt mouse mode
        "\x1b[?2004l",  // Disable bracketed paste mode
        "\x1b[?1004l",  // Disable focus reporting
        "\x1b[<u",      // Disable kitty keyboard protocol
        "\x1b[?1049l",  // Exit alternate screen buffer
        "\x1b[?1l",     // Reset cursor keys mode
        "\x1b[?7h",     // Enable line wrapping
        "\x1b[?25h",    // Show cursor
        "\x1b[0m",      // Reset attributes
        "\x1b[r",       // Reset scroll region
        "\x1b[H",       // Move cursor home
        "\x1bc",        // Full terminal reset (RIS)
    );

    // Try /dev/tty first (direct terminal access)
    #[cfg(unix)]
    {
        if let Ok(mut tty) = OpenOptions::new().write(true).open("/dev/tty") {
            let _ = tty.write_all(reset_sequence.as_bytes());
            let _ = tty.flush();
            debug_log("restore_terminal: wrote reset to /dev/tty");
        } else {
            // Fallback to stdout
            let mut stdout = io::stdout();
            let _ = stdout.write_all(reset_sequence.as_bytes());
            let _ = stdout.flush();
            debug_log("restore_terminal: wrote reset to stdout (fallback)");
        }
    }

    #[cfg(not(unix))]
    {
        let mut stdout = io::stdout();
        let _ = stdout.write_all(reset_sequence.as_bytes());
        let _ = stdout.flush();
        debug_log("restore_terminal: wrote reset to stdout");
    }

    // Run stty sane as fallback
    #[cfg(unix)]
    {
        use std::process::Command;
        let _ = Command::new("stty")
            .arg("sane")
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        debug_log("restore_terminal: stty sane executed");
    }

    debug_log("restore_terminal: complete");
}

/// Stop signals that Claude outputs to indicate task completion
const TASK_COMPLETE_SIGNAL: &str = "###TASK_COMPLETE###";
const ALL_COMPLETE_SIGNAL: &str = "###ALL_TASKS_COMPLETE###";

/// ASCII byte patterns for raw signal detection (fallback when UTF-8 fails)
const TASK_COMPLETE_BYTES: &[u8] = b"###TASK_COMPLETE###";
const ALL_COMPLETE_BYTES: &[u8] = b"###ALL_TASKS_COMPLETE###";

/// Buffer retention size after truncation (16KB to handle split signals)
const BUFFER_RETENTION_SIZE: usize = 16384;
/// Threshold for triggering truncation (32KB)
const BUFFER_TRUNCATION_THRESHOLD: usize = 32768;

/// Strip ANSI escape sequences from bytes using the strip-ansi-escapes crate
/// Returns a String using lossy UTF-8 conversion to avoid dropping data
fn strip_ansi_escapes_from_bytes(data: &[u8]) -> String {
    let stripped = strip_ansi_escapes::strip(data);
    String::from_utf8_lossy(&stripped).into_owned()
}

/// Check if raw bytes contain a signal pattern (fallback for non-UTF8 data)
fn bytes_contain_signal(data: &[u8], pattern: &[u8]) -> bool {
    data.windows(pattern.len()).any(|window| window == pattern)
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
    /// Iteration timed out
    Timeout,
}

/// Messages sent from the PTY reader thread
enum PtyMessage {
    Data(Vec<u8>),
    Closed,
    Error(String),
}

/// Manages a PTY session for running Claude
pub struct PtyManager {
    pty_pair: Option<PtyPair>,
    should_stop: Arc<AtomicBool>,
    ctrl_c_state: Arc<AtomicU8>,
    child_pid: Option<u32>,
    child: Option<Box<dyn Child + Send + Sync>>,
    reader_thread: Option<JoinHandle<()>>,
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
            pty_pair: Some(pty_pair),
            should_stop,
            ctrl_c_state: Arc::new(AtomicU8::new(CTRL_C_NONE)),
            child_pid: None,
            child: None,
            reader_thread: None,
        })
    }

    /// Spawn Claude in the PTY
    pub fn spawn_claude(&mut self, prompt_path: &Path) -> Result<()> {
        let pty_pair = self.pty_pair.as_ref()
            .ok_or_else(|| HydraError::io("PTY already consumed", io::Error::other("PTY pair is None")))?;

        // Build command to run Claude
        let mut cmd = CommandBuilder::new("claude");
        cmd.arg("--dangerously-skip-permissions");
        cmd.arg(prompt_path);

        // Set working directory to current directory
        let cwd = std::env::current_dir()
            .map_err(|e| HydraError::io("getting current directory", e))?;
        cmd.cwd(cwd);

        // Spawn the command in the PTY
        let child = pty_pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| HydraError::io("spawning claude in PTY", io::Error::other(e.to_string())))?;

        // Get the process ID
        if let Some(pid) = child.process_id() {
            self.child_pid = Some(pid);
            set_child_pid(pid);
        }

        // Store the child handle for proper cleanup
        self.child = Some(child);

        Ok(())
    }

    /// Run the I/O loop, handling input/output and watching for signals
    pub fn run_io_loop(&mut self, output_path: &Path, verbose: bool, timeout_seconds: u64) -> Result<PtyResult> {
        // Open output file for capturing Claude's output
        let mut output_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(output_path)
            .map_err(|e| HydraError::io("opening output file", e))?;

        let pty_pair = self.pty_pair.as_ref()
            .ok_or_else(|| HydraError::io("PTY already consumed", io::Error::other("PTY pair is None")))?;

        // Get PTY reader and writer
        let pty_reader = pty_pair
            .master
            .try_clone_reader()
            .map_err(|e| HydraError::io("cloning PTY reader", io::Error::other(e.to_string())))?;
        let mut pty_writer = pty_pair
            .master
            .take_writer()
            .map_err(|e| HydraError::io("taking PTY writer", io::Error::other(e.to_string())))?;

        // Create channel for PTY reader thread to communicate back
        let (tx, rx): (Sender<PtyMessage>, Receiver<PtyMessage>) = mpsc::channel();

        // Spawn a thread to read from PTY (blocking read)
        let reader_should_stop = Arc::clone(&self.should_stop);
        let reader_handle = thread::spawn(move || {
            Self::pty_reader_thread(pty_reader, tx, reader_should_stop);
        });
        self.reader_thread = Some(reader_handle);

        // Enable raw mode for stdin
        debug_log("run_io_loop: enabling raw mode");
        enable_raw_mode().map_err(|e| HydraError::io("enabling raw mode", io::Error::other(e.to_string())))?;
        debug_log("run_io_loop: raw mode enabled");

        let result = self.io_loop_inner(
            &mut pty_writer,
            &mut output_file,
            &rx,
            verbose,
            timeout_seconds,
        );

        debug_log(&format!("run_io_loop: io_loop_inner returned {:?}", result));

        // Clean up: drop PTY to close file descriptors and wake up reader thread
        // Do this BEFORE disabling raw mode so the reader thread can exit
        self.cleanup(verbose);

        // Disable raw mode with fallback terminal reset
        restore_terminal(verbose);

        debug_log("run_io_loop: returning result");
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
        timeout_seconds: u64,
    ) -> Result<PtyResult> {
        use std::time::Instant;

        // Write to /dev/tty instead of stdout to avoid potential issues
        // with stdout buffering or redirection
        let mut tty_output: Box<dyn Write> = match OpenOptions::new().write(true).open("/dev/tty") {
            Ok(tty) => Box::new(tty),
            Err(_) => Box::new(io::stdout()),
        };
        let poll_timeout = Duration::from_millis(10);
        let timeout_duration = Duration::from_secs(timeout_seconds);
        let start_time = Instant::now();

        // Raw byte accumulator for signal detection (handles non-UTF8 data)
        let mut raw_accumulator: Vec<u8> = Vec::new();

        loop {
            // Check for timeout
            if start_time.elapsed() >= timeout_duration {
                println!();
                println!("[hydra] Iteration timeout ({} seconds) reached without stop signal, terminating Claude process...", timeout_seconds);
                self.terminate_child();
                return Ok(PtyResult::Timeout);
            }

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
                    // Write to tty/stdout
                    tty_output
                        .write_all(&data)
                        .map_err(|e| HydraError::io("writing to tty", e))?;
                    tty_output.flush().map_err(|e| HydraError::io("flushing tty", e))?;

                    // Write to output file
                    output_file
                        .write_all(&data)
                        .map_err(|e| HydraError::io("writing to output file", e))?;
                    output_file
                        .flush()
                        .map_err(|e| HydraError::io("flushing output file", e))?;

                    // Accumulate raw bytes for signal detection
                    raw_accumulator.extend_from_slice(&data);

                    // Check for stop signals BEFORE truncation
                    let signal_result = self.check_for_signals_in_bytes(&raw_accumulator, verbose);

                    // Truncate to avoid unbounded growth (keep last 16KB)
                    if raw_accumulator.len() > BUFFER_TRUNCATION_THRESHOLD {
                        let drain_to = raw_accumulator.len() - BUFFER_RETENTION_SIZE;
                        raw_accumulator.drain(..drain_to);
                        if verbose {
                            eprintln!(
                                "[hydra:debug] Truncated accumulator to {} bytes",
                                raw_accumulator.len()
                            );
                        }
                    }

                    if signal_result != PtyResult::NoSignal {
                        if verbose {
                            eprintln!("[hydra:debug] Signal detected, terminating Claude");
                        }

                        // Immediately send terminal reset to try to recover before freeze
                        // This is sent BEFORE terminating Claude
                        let emergency_reset = concat!(
                            "\x18",         // CAN - cancel partial escape
                            "\x1b[?1000l",  // Disable mouse
                            "\x1b[?2004l",  // Disable bracketed paste
                            "\x1b[?1049l",  // Exit alt screen
                            "\x1b[0m",      // Reset attributes
                            "\x1b[?25h",    // Show cursor
                        );
                        let _ = tty_output.write_all(emergency_reset.as_bytes());
                        let _ = tty_output.flush();
                        debug_log("io_loop: emergency reset sent before termination");

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
                    if verbose {
                        eprintln!("[hydra:debug] PTY closed, checking final buffer for signals");
                    }
                    return Ok(self.check_for_signals_in_bytes(&raw_accumulator, verbose));
                }
                Ok(PtyMessage::Error(e)) => {
                    // Error reading from PTY - process likely exited
                    if verbose {
                        eprintln!("[hydra:debug] PTY error: {}, checking final buffer", e);
                    }
                    return Ok(self.check_for_signals_in_bytes(&raw_accumulator, verbose));
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // No data available, continue
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    // Reader thread exited
                    if verbose {
                        eprintln!("[hydra:debug] PTY reader disconnected, checking final buffer");
                    }
                    return Ok(self.check_for_signals_in_bytes(&raw_accumulator, verbose));
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

    /// Check accumulated output for stop signals using multiple detection strategies
    fn check_for_signals_in_bytes(&self, accumulator: &[u8], verbose: bool) -> PtyResult {
        // Strategy 1: Check raw bytes directly (fastest, handles case where signal has no ANSI codes)
        if bytes_contain_signal(accumulator, ALL_COMPLETE_BYTES) {
            if verbose {
                eprintln!("[hydra:debug] Signal found via raw byte search: ALL_TASKS_COMPLETE");
            }
            return PtyResult::AllComplete;
        }
        if bytes_contain_signal(accumulator, TASK_COMPLETE_BYTES) {
            if verbose {
                eprintln!("[hydra:debug] Signal found via raw byte search: TASK_COMPLETE");
            }
            return PtyResult::TaskComplete;
        }

        // Strategy 2: Strip ANSI codes and check (handles interspersed escape sequences)
        let clean = strip_ansi_escapes_from_bytes(accumulator);

        if clean.contains(ALL_COMPLETE_SIGNAL) {
            if verbose {
                eprintln!("[hydra:debug] Signal found after ANSI stripping: ALL_TASKS_COMPLETE");
            }
            return PtyResult::AllComplete;
        }
        if clean.contains(TASK_COMPLETE_SIGNAL) {
            if verbose {
                eprintln!("[hydra:debug] Signal found after ANSI stripping: TASK_COMPLETE");
            }
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

    /// Clean up resources after I/O loop completes
    /// This closes the PTY to wake up the reader thread and waits for child process
    fn cleanup(&mut self, verbose: bool) {
        debug_log("cleanup: starting");

        // Signal reader thread to stop first
        self.should_stop.store(true, Ordering::SeqCst);
        debug_log("cleanup: should_stop set to true");

        // Force kill the child process immediately - don't wait for graceful exit
        // This is the fastest way to ensure the PTY slave closes and reader gets EOF
        if verbose {
            eprintln!("[hydra:debug] Force killing child process...");
        }
        self.force_kill_child();
        debug_log("cleanup: force_kill_child called");

        // Brief wait for child to actually die
        if let Some(ref mut child) = self.child {
            debug_log("cleanup: waiting for child to die");
            let wait_start = std::time::Instant::now();
            while wait_start.elapsed() < Duration::from_millis(500) {
                match child.try_wait() {
                    Ok(Some(_)) => {
                        debug_log("cleanup: child exited");
                        break;
                    }
                    Ok(None) => thread::sleep(Duration::from_millis(10)),
                    Err(_) => break,
                }
            }
        }
        self.child = None;
        debug_log("cleanup: child = None");

        // Drop the PTY pair to close file descriptors
        // This should cause the reader thread to get EOF (eventually)
        if verbose {
            eprintln!("[hydra:debug] Closing PTY...");
        }
        self.pty_pair = None;
        debug_log("cleanup: pty_pair = None");

        // Wait briefly for reader thread to notice the PTY closed
        // The cloned FD might keep the read() blocking, but child death should cause EOF
        if let Some(handle) = self.reader_thread.take() {
            debug_log("cleanup: waiting for reader thread (max 500ms)");
            if verbose {
                eprintln!("[hydra:debug] Waiting for reader thread (max 500ms)...");
            }

            // Spawn a thread to join, with timeout
            let (tx, rx) = mpsc::channel();
            let join_thread = thread::spawn(move || {
                let result = handle.join();
                let _ = tx.send(result);
            });

            // Wait with timeout
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(_) => {
                    debug_log("cleanup: reader thread exited cleanly");
                    if verbose {
                        eprintln!("[hydra:debug] Reader thread exited cleanly");
                    }
                }
                Err(_) => {
                    debug_log("cleanup: reader thread timeout, detaching");
                    if verbose {
                        eprintln!("[hydra:debug] Reader thread did not exit in time, detaching");
                    }
                    // Detach the join thread - the reader will die when process exits
                    drop(join_thread);
                }
            }
        }
        debug_log("cleanup: complete");
    }
}

impl Drop for PtyManager {
    fn drop(&mut self) {
        // Clear the child PID from signal handler
        clear_child_pid();

        // Signal reader thread to stop
        self.should_stop.store(true, Ordering::SeqCst);

        // Clean up any remaining resources (in case cleanup wasn't called)
        // Force kill child if still running
        if self.child.is_some() {
            self.force_kill_child();
        }
        self.child = None;

        // Drop PTY to close file descriptors
        self.pty_pair = None;

        // DON'T wait for reader thread - just detach it
        // Trying to join can hang because the reader has a cloned FD
        if let Some(_handle) = self.reader_thread.take() {
            // Dropping detaches the thread - it will be killed on process exit
        }

        // Make sure terminal is restored to normal mode
        restore_terminal(false);
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

    fn create_test_manager() -> PtyManager {
        PtyManager {
            pty_pair: Some({
                let pty_system = native_pty_system();
                pty_system
                    .openpty(PtySize {
                        rows: 24,
                        cols: 80,
                        pixel_width: 0,
                        pixel_height: 0,
                    })
                    .unwrap()
            }),
            should_stop: Arc::new(AtomicBool::new(false)),
            ctrl_c_state: Arc::new(AtomicU8::new(CTRL_C_NONE)),
            child_pid: None,
            child: None,
            reader_thread: None,
        }
    }

    #[test]
    fn test_pty_result_signal_detection() {
        let manager = create_test_manager();

        assert_eq!(
            manager.check_for_signals_in_bytes(b"some output ###TASK_COMPLETE### more", false),
            PtyResult::TaskComplete
        );
        assert_eq!(
            manager.check_for_signals_in_bytes(b"###ALL_TASKS_COMPLETE###", false),
            PtyResult::AllComplete
        );
        assert_eq!(
            manager.check_for_signals_in_bytes(b"no signals here", false),
            PtyResult::NoSignal
        );
    }

    #[test]
    fn test_strip_ansi_escapes_from_bytes() {
        // Plain text should pass through unchanged
        assert_eq!(strip_ansi_escapes_from_bytes(b"hello world"), "hello world");

        // CSI sequences (colors, cursor movement) should be stripped
        assert_eq!(strip_ansi_escapes_from_bytes(b"\x1b[32mgreen\x1b[0m"), "green");
        assert_eq!(strip_ansi_escapes_from_bytes(b"\x1b[1;31mbold red\x1b[0m"), "bold red");

        // Multiple sequences
        assert_eq!(
            strip_ansi_escapes_from_bytes(b"\x1b[32m###\x1b[0mTASK_COMPLETE\x1b[32m###\x1b[0m"),
            "###TASK_COMPLETE###"
        );

        // OSC sequences (title setting, etc.)
        assert_eq!(strip_ansi_escapes_from_bytes(b"\x1b]0;title\x07text"), "text");

        // Cursor movement
        assert_eq!(strip_ansi_escapes_from_bytes(b"\x1b[Hstart\x1b[10;20H"), "start");
    }

    #[test]
    fn test_bytes_contain_signal() {
        assert!(bytes_contain_signal(b"###TASK_COMPLETE###", TASK_COMPLETE_BYTES));
        assert!(bytes_contain_signal(b"prefix###TASK_COMPLETE###suffix", TASK_COMPLETE_BYTES));
        assert!(!bytes_contain_signal(b"###TASK_INCOMPLET###", TASK_COMPLETE_BYTES));
        assert!(bytes_contain_signal(b"###ALL_TASKS_COMPLETE###", ALL_COMPLETE_BYTES));
    }

    #[test]
    fn test_signal_detection_with_ansi_codes() {
        let manager = create_test_manager();

        // Signal with color codes around it
        assert_eq!(
            manager.check_for_signals_in_bytes(b"\x1b[32m###TASK_COMPLETE###\x1b[0m", false),
            PtyResult::TaskComplete
        );

        // Signal with color codes interspersed
        assert_eq!(
            manager.check_for_signals_in_bytes(b"output\x1b[1m###ALL_TASKS_COMPLETE###\x1b[0m\n", false),
            PtyResult::AllComplete
        );

        // Mixed content with cursor movements
        assert_eq!(
            manager.check_for_signals_in_bytes(b"\x1b[H\x1b[2JDone!\n\x1b[32m###TASK_COMPLETE###\x1b[0m", false),
            PtyResult::TaskComplete
        );
    }

    #[test]
    fn test_signal_detection_with_invalid_utf8() {
        let manager = create_test_manager();

        // Signal mixed with invalid UTF-8 bytes
        let mut data = Vec::new();
        data.extend_from_slice(b"prefix");
        data.push(0xFF); // Invalid UTF-8
        data.push(0xFE); // Invalid UTF-8
        data.extend_from_slice(b"###TASK_COMPLETE###");
        data.push(0x80); // Invalid UTF-8
        data.extend_from_slice(b"suffix");

        // Should still detect the signal via raw byte search
        assert_eq!(
            manager.check_for_signals_in_bytes(&data, false),
            PtyResult::TaskComplete
        );
    }

    #[test]
    fn test_signal_detection_with_dcs_sequence() {
        let manager = create_test_manager();

        // DCS sequence (ESC P ... ESC \) that the old parser didn't handle
        let data = b"\x1bP+q\x1b\\###TASK_COMPLETE###";
        assert_eq!(
            manager.check_for_signals_in_bytes(data, false),
            PtyResult::TaskComplete
        );
    }

    #[test]
    fn test_cleanup_does_not_wait_for_reader_thread() {
        use std::time::Instant;

        // Create a PtyManager with a simulated blocking reader thread
        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_clone = Arc::clone(&should_stop);

        // Spawn a thread that would block for a long time
        let blocking_thread = thread::spawn(move || {
            // This thread will block for 10 seconds unless stopped
            for _ in 0..1000 {
                if should_stop_clone.load(Ordering::SeqCst) {
                    return;
                }
                thread::sleep(Duration::from_millis(10));
            }
        });

        let mut manager = PtyManager {
            pty_pair: None, // No actual PTY needed for this test
            should_stop: Arc::clone(&should_stop),
            ctrl_c_state: Arc::new(AtomicU8::new(CTRL_C_NONE)),
            child_pid: None,
            child: None,
            reader_thread: Some(blocking_thread),
        };

        // Time the cleanup - it should complete almost immediately since we don't wait
        let start = Instant::now();
        manager.cleanup(false);
        let elapsed = start.elapsed();

        // Cleanup should complete in under 1 second (just the child kill wait)
        // We no longer wait for the reader thread at all
        assert!(
            elapsed < Duration::from_secs(1),
            "Cleanup took {:?}, expected < 1s - cleanup should not wait for reader",
            elapsed
        );
    }

    #[test]
    fn test_drop_does_not_wait_for_reader_thread() {
        use std::time::Instant;

        // Create a PtyManager with a simulated blocking reader thread
        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_clone = Arc::clone(&should_stop);

        // Spawn a thread that would block for a long time
        let blocking_thread = thread::spawn(move || {
            for _ in 0..1000 {
                if should_stop_clone.load(Ordering::SeqCst) {
                    return;
                }
                thread::sleep(Duration::from_millis(10));
            }
        });

        let manager = PtyManager {
            pty_pair: None,
            should_stop: Arc::clone(&should_stop),
            ctrl_c_state: Arc::new(AtomicU8::new(CTRL_C_NONE)),
            child_pid: None,
            child: None,
            reader_thread: Some(blocking_thread),
        };

        // Time the drop - it should complete almost immediately
        let start = Instant::now();
        drop(manager);
        let elapsed = start.elapsed();

        // Drop should complete in under 100ms - we just detach the thread
        assert!(
            elapsed < Duration::from_millis(100),
            "Drop took {:?}, expected < 100ms - drop should not wait for reader",
            elapsed
        );
    }

    #[test]
    fn test_cleanup_sets_should_stop_flag() {
        let should_stop = Arc::new(AtomicBool::new(false));

        let mut manager = PtyManager {
            pty_pair: None,
            should_stop: Arc::clone(&should_stop),
            ctrl_c_state: Arc::new(AtomicU8::new(CTRL_C_NONE)),
            child_pid: None,
            child: None,
            reader_thread: None,
        };

        assert!(!should_stop.load(Ordering::SeqCst));
        manager.cleanup(false);
        assert!(should_stop.load(Ordering::SeqCst));
    }
}
