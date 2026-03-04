use crate::config::Config;
use crate::error::{HydraError, Result};
use crate::prompt::ResolvedPrompt;
use crate::runner::{IterationResult, RunResult};
use crate::signal;
use chrono::Local;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Iteration instructions prepended to the prompt
const ITERATION_INSTRUCTIONS: &str = include_str!("../templates/iteration-instructions.md");

/// Session logger for headless mode (same format as PTY mode)
struct SessionLogger {
    path: PathBuf,
    file: File,
}

impl SessionLogger {
    fn new(plan_name: Option<&str>) -> Result<Self> {
        let logs_dir = Config::logs_dir();
        if !logs_dir.exists() {
            fs::create_dir_all(&logs_dir).map_err(|e| {
                HydraError::io(format!("creating logs directory {}", logs_dir.display()), e)
            })?;
        }

        let timestamp = Local::now().format("%Y%m%d-%H%M%S");
        let filename = match plan_name {
            Some(name) => format!("{}-{}.log", name, timestamp),
            None => format!("hydra-{}.log", timestamp),
        };
        let path = logs_dir.join(filename);

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)
            .map_err(|e| HydraError::io(format!("creating log file {}", path.display()), e))?;

        Ok(Self { path, file })
    }

    fn log(&mut self, message: &str) -> Result<()> {
        let timestamp = Local::now().format("%H:%M:%S");
        writeln!(self.file, "[{}] {}", timestamp, message)
            .map_err(|e| HydraError::io("writing to log file", e))?;
        self.file
            .flush()
            .map_err(|e| HydraError::io("flushing log file", e))?;
        Ok(())
    }

    fn append_content(&mut self, content: &str) -> Result<()> {
        write!(self.file, "{}", content)
            .map_err(|e| HydraError::io("writing content to log file", e))?;
        self.file
            .flush()
            .map_err(|e| HydraError::io("flushing log file", e))?;
        Ok(())
    }

    fn log_iteration_start(&mut self, iteration: u32, max: u32) -> Result<()> {
        let separator = "=".repeat(80);
        self.append_content(&format!("\n{}\n", separator))?;
        self.log(&format!("ITERATION {}/{} START", iteration, max))?;
        self.append_content(&format!("{}\n\n", separator))?;
        Ok(())
    }

    fn log_iteration_end(&mut self, iteration: u32, result: &IterationResult) -> Result<()> {
        let result_str = match result {
            IterationResult::TaskComplete => "TASK_COMPLETE",
            IterationResult::AllComplete => "ALL_COMPLETE",
            IterationResult::NoSignal => "NO_SIGNAL",
            IterationResult::Terminated => "TERMINATED",
            IterationResult::Timeout => "TIMEOUT",
        };
        self.log(&format!("ITERATION {} END: {}", iteration, result_str))?;
        Ok(())
    }
}

/// Parse stream-json output from `claude -p --output-format stream-json`
///
/// Claude Code's stream-json format emits newline-delimited JSON objects.
/// Assistant messages have `{"type":"assistant","message":{"content":[...]}}`.
/// Content blocks are either `{"text":"..."}` or `{"type":"tool_use",...}`.
/// We extract text from assistant content blocks and scan for stop signals.
struct StreamJsonParser {
    /// Accumulated text from all assistant messages
    text_accumulator: String,
}

impl StreamJsonParser {
    fn new() -> Self {
        Self {
            text_accumulator: String::new(),
        }
    }

    /// Process a single line of stream-json output.
    /// Returns Some(text) if text content was extracted, None otherwise.
    fn process_line(&mut self, line: &str) -> Option<String> {
        let value: serde_json::Value = serde_json::from_str(line).ok()?;

        // Filter for assistant messages
        if value.get("type")?.as_str()? != "assistant" {
            return None;
        }

        // Extract text from message.content array
        let content = value.get("message")?.get("content")?.as_array()?;
        let mut extracted = String::new();
        for block in content {
            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                extracted.push_str(text);
            }
        }

        if extracted.is_empty() {
            return None;
        }

        self.text_accumulator.push_str(&extracted);
        Some(extracted)
    }

    /// Check if a stop signal has been detected in the accumulated text
    fn check_stop_signal(&self) -> Option<IterationResult> {
        if self.text_accumulator.contains("###ALL_TASKS_COMPLETE###") {
            Some(IterationResult::AllComplete)
        } else if self.text_accumulator.contains("###TASK_COMPLETE###") {
            Some(IterationResult::TaskComplete)
        } else {
            None
        }
    }
}

/// Headless runner that uses `claude -p` instead of PTY
pub struct HeadlessRunner {
    config: Config,
    prompt: ResolvedPrompt,
    should_stop: Arc<AtomicBool>,
    logger: Option<SessionLogger>,
    plan_name: Option<String>,
}

impl HeadlessRunner {
    pub fn new(config: Config, prompt: ResolvedPrompt, plan_name: Option<String>) -> Self {
        let logger = match SessionLogger::new(plan_name.as_deref()) {
            Ok(l) => Some(l),
            Err(e) => {
                eprintln!("[hydra] Warning: Could not create session log: {}", e);
                None
            }
        };

        Self {
            config,
            prompt,
            should_stop: Arc::new(AtomicBool::new(false)),
            logger,
            plan_name,
        }
    }

    /// Get a clone of the stop flag for signal handlers
    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.should_stop)
    }

    /// Create the combined prompt string (iteration instructions + user prompt)
    fn create_combined_prompt(&self) -> String {
        format!("{}\n{}", ITERATION_INSTRUCTIONS, self.prompt.content)
    }

    /// Check if the stop file exists
    fn check_stop_file(&self) -> bool {
        let stop_path = PathBuf::from(&self.config.stop_file);
        if stop_path.exists() {
            let _ = fs::remove_file(&stop_path);
            true
        } else {
            false
        }
    }

    /// Run a single headless iteration
    fn run_iteration(&mut self, iteration: u32) -> Result<IterationResult> {
        println!(
            "[hydra] Iteration {}/{}...",
            iteration, self.config.max_iterations
        );

        let combined_prompt = self.create_combined_prompt();

        // Spawn claude -p with stream-json output
        // Clear CLAUDECODE env var to allow nested Claude sessions (headless mode
        // is specifically designed to spawn claude -p as a subprocess)
        let mut child = Command::new("claude")
            .args([
                "-p",
                "--dangerously-skip-permissions",
                "--output-format",
                "stream-json",
                "--verbose",
            ])
            .env_remove("CLAUDECODE")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| HydraError::io("spawning claude -p", e))?;

        // Track child PID for signal handling
        let child_id = child.id();
        signal::set_child_pid(child_id);

        // Write prompt to stdin, then close it
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(combined_prompt.as_bytes())
                .map_err(|e| HydraError::io("writing prompt to claude stdin", e))?;
            // stdin is dropped here, closing the pipe
        }

        // Read stdout through StreamJsonParser
        let stdout = child.stdout.take().ok_or_else(|| {
            HydraError::io("taking claude stdout", std::io::Error::other("no stdout"))
        })?;

        let reader = BufReader::new(stdout);
        let mut parser = StreamJsonParser::new();
        let mut result = IterationResult::NoSignal;

        // Set up timeout
        let timeout_secs = self.config.timeout_seconds;
        let start_time = std::time::Instant::now();

        for line in reader.lines() {
            // Check timeout
            if start_time.elapsed().as_secs() >= timeout_secs {
                eprintln!(
                    "[hydra] Iteration timeout ({timeout_secs}s), terminating claude process"
                );
                // Kill the child process group
                let pid = child_id as i32;
                let _ = nix::sys::signal::killpg(
                    nix::unistd::Pid::from_raw(pid),
                    nix::sys::signal::Signal::SIGTERM,
                );
                std::thread::sleep(std::time::Duration::from_millis(500));
                let _ = nix::sys::signal::killpg(
                    nix::unistd::Pid::from_raw(pid),
                    nix::sys::signal::Signal::SIGKILL,
                );
                result = IterationResult::Timeout;
                break;
            }

            // Check if we should stop (signal received)
            if self.should_stop.load(Ordering::SeqCst) {
                result = IterationResult::Terminated;
                break;
            }

            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };

            // Process the line through the parser
            if let Some(text) = parser.process_line(&line) {
                // Write extracted text to log
                if let Some(ref mut logger) = self.logger {
                    let _ = logger.append_content(&text);
                }
            }

            // Check for stop signals after each line
            if let Some(signal_result) = parser.check_stop_signal() {
                result = signal_result;
                break;
            }
        }

        // Wait for child to exit
        let _ = child.wait();
        signal::clear_child_pid();

        // Print status based on result
        match &result {
            IterationResult::TaskComplete => {
                println!("[hydra] TASK_COMPLETE detected");
            }
            IterationResult::AllComplete => {
                println!("[hydra] ALL_TASKS_COMPLETE — done");
            }
            IterationResult::Timeout => {
                println!("[hydra] Iteration timed out");
            }
            IterationResult::Terminated => {
                println!("[hydra] Iteration terminated");
            }
            IterationResult::NoSignal => {
                println!("[hydra] No stop signal detected");
            }
        }

        Ok(result)
    }

    /// Run the main headless loop
    pub fn run(&mut self) -> Result<RunResult> {
        let max = self.config.max_iterations;

        println!("[hydra] Starting headless mode");
        println!("[hydra] Using prompt: {}", self.prompt.path.display());
        if let Some(ref logger) = self.logger {
            println!("[hydra] Session log: {}", logger.path.display());
        }

        // Log session start
        if let Some(ref mut logger) = self.logger {
            let _ = logger.log(&format!(
                "Session started (headless) - max iterations: {}",
                max
            ));
            let _ = logger.log(&format!("Prompt file: {}", self.prompt.path.display()));
            if let Some(ref plan) = self.plan_name {
                let _ = logger.log(&format!("Plan: {}", plan));
            }
        }

        for iteration in 1..=max {
            // Check for stop file
            if self.check_stop_file() {
                println!("[hydra] Stop file detected, exiting gracefully");
                if let Some(ref mut logger) = self.logger {
                    let _ = logger.log("Session ended: stop file detected");
                }
                return Ok(RunResult::Stopped {
                    iterations: iteration - 1,
                });
            }

            // Check for graceful stop request
            if self.should_stop.load(Ordering::SeqCst) {
                println!("[hydra] Graceful shutdown complete");
                if let Some(ref mut logger) = self.logger {
                    let _ = logger.log("Session ended: graceful shutdown");
                }
                return Ok(RunResult::Stopped {
                    iterations: iteration - 1,
                });
            }

            // Log iteration start
            if let Some(ref mut logger) = self.logger {
                let _ = logger.log_iteration_start(iteration, max);
            }

            // Run the iteration
            let result = self.run_iteration(iteration)?;

            // Log iteration end
            if let Some(ref mut logger) = self.logger {
                let _ = logger.log_iteration_end(iteration, &result);
            }

            match result {
                IterationResult::AllComplete => {
                    println!(
                        "[hydra] All tasks complete! Total iterations: {}",
                        iteration
                    );
                    if let Some(ref mut logger) = self.logger {
                        let _ = logger.log(&format!(
                            "Session ended: all tasks complete after {} iterations",
                            iteration
                        ));
                    }
                    return Ok(RunResult::AllTasksComplete {
                        iterations: iteration,
                    });
                }
                IterationResult::Terminated => {
                    println!("[hydra] Graceful shutdown complete");
                    if let Some(ref mut logger) = self.logger {
                        let _ = logger.log("Session ended: terminated");
                    }
                    return Ok(RunResult::Stopped {
                        iterations: iteration,
                    });
                }
                IterationResult::TaskComplete
                | IterationResult::NoSignal
                | IterationResult::Timeout => {
                    // Reset should_stop flag (may have been set during teardown)
                    self.should_stop.store(false, Ordering::SeqCst);
                    if self.config.verbose {
                        eprintln!("[hydra:debug] Continuing to next iteration");
                    }
                }
            }
        }

        println!("[hydra] Max iterations reached");
        if let Some(ref mut logger) = self.logger {
            let _ = logger.log(&format!("Session ended: max iterations ({}) reached", max));
        }
        Ok(RunResult::MaxIterations { iterations: max })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_json_parser_assistant_text() {
        let mut parser = StreamJsonParser::new();

        // Assistant message with text content block
        let line = r#"{"type":"assistant","message":{"content":[{"text":"Hello "}]}}"#;
        let result = parser.process_line(line);
        assert_eq!(result, Some("Hello ".to_string()));
        assert_eq!(parser.text_accumulator, "Hello ");

        // Another assistant message
        let line = r#"{"type":"assistant","message":{"content":[{"text":"world!"}]}}"#;
        let result = parser.process_line(line);
        assert_eq!(result, Some("world!".to_string()));
        assert_eq!(parser.text_accumulator, "Hello world!");
    }

    #[test]
    fn test_stream_json_parser_ignores_non_assistant() {
        let mut parser = StreamJsonParser::new();

        // System events
        let line = r#"{"type":"system","subtype":"init"}"#;
        assert!(parser.process_line(line).is_none());

        // User messages
        let line = r#"{"type":"user","message":{"content":[{"text":"hello"}]}}"#;
        assert!(parser.process_line(line).is_none());

        // Result events
        let line = r#"{"type":"result","subtype":"success"}"#;
        assert!(parser.process_line(line).is_none());

        // Invalid JSON
        assert!(parser.process_line("not json").is_none());

        assert_eq!(parser.text_accumulator, "");
    }

    #[test]
    fn test_stream_json_parser_tool_use_blocks_ignored() {
        let mut parser = StreamJsonParser::new();

        // Assistant message with only tool_use content (no text)
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_123","name":"Read","input":{"file_path":"foo.rs"}}]}}"#;
        assert!(parser.process_line(line).is_none());
        assert_eq!(parser.text_accumulator, "");
    }

    #[test]
    fn test_stream_json_parser_mixed_content_blocks() {
        let mut parser = StreamJsonParser::new();

        // Assistant message with both text and tool_use blocks
        let line = r#"{"type":"assistant","message":{"content":[{"text":"Reading file..."},{"type":"tool_use","id":"toolu_123","name":"Read","input":{}}]}}"#;
        let result = parser.process_line(line);
        assert_eq!(result, Some("Reading file...".to_string()));
        assert_eq!(parser.text_accumulator, "Reading file...");
    }

    #[test]
    fn test_stream_json_parser_stop_signals() {
        let mut parser = StreamJsonParser::new();

        // No signal yet
        assert!(parser.check_stop_signal().is_none());

        parser.text_accumulator = "Some output\n###TASK_COMPLETE###\n".to_string();
        assert_eq!(
            parser.check_stop_signal(),
            Some(IterationResult::TaskComplete)
        );

        parser.text_accumulator = "Some output\n###ALL_TASKS_COMPLETE###\n".to_string();
        assert_eq!(
            parser.check_stop_signal(),
            Some(IterationResult::AllComplete)
        );
    }

    #[test]
    fn test_stream_json_parser_all_complete_takes_priority() {
        let mut parser = StreamJsonParser::new();

        // If both signals are present, ALL_TASKS_COMPLETE should take priority
        parser.text_accumulator = "###TASK_COMPLETE###\n###ALL_TASKS_COMPLETE###\n".to_string();
        assert_eq!(
            parser.check_stop_signal(),
            Some(IterationResult::AllComplete)
        );
    }
}
