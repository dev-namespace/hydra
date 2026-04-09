use crate::config::Config;
use crate::error::{HydraError, Result};
use crate::harness::Harness;
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

/// Trait implemented by per-harness stream-json parsers so the headless
/// runner can iterate over child stdout without knowing which harness is
/// producing the events.
trait HarnessStreamParser {
    /// Process a single newline-delimited JSON line from the harness.
    /// Returns `Some(text)` when text content was extracted for logging,
    /// `None` otherwise.
    fn process_line(&mut self, line: &str) -> Option<String>;

    /// Inspect the accumulated text and return an `IterationResult` if a
    /// stop signal (`###TASK_COMPLETE###` / `###ALL_TASKS_COMPLETE###`)
    /// has been observed.
    fn check_stop_signal(&self) -> Option<IterationResult>;
}

/// Default stop-signal scanner used by both parsers. `ALL_TASKS_COMPLETE`
/// takes priority when both signals appear in the same accumulator.
fn scan_stop_signal(accumulator: &str) -> Option<IterationResult> {
    if accumulator.contains("###ALL_TASKS_COMPLETE###") {
        Some(IterationResult::AllComplete)
    } else if accumulator.contains("###TASK_COMPLETE###") {
        Some(IterationResult::TaskComplete)
    } else {
        None
    }
}

/// Parse stream-json output from `claude -p --output-format stream-json`.
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
}

impl HarnessStreamParser for StreamJsonParser {
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

    fn check_stop_signal(&self) -> Option<IterationResult> {
        scan_stop_signal(&self.text_accumulator)
    }
}

/// Parse stream-json output from `pi -p --mode json`.
///
/// Pi's JSON event stream is newline-delimited. Hydra only cares about
/// assistant text events — specifically `message_update` events whose
/// `assistantMessageEvent.type` is `text_delta`, where `delta` carries
/// the new text chunk. Thinking deltas, tool-call deltas, session headers,
/// `agent_start`/`agent_end`, and `text_start`/`text_end` bookends are all
/// ignored for logging purposes (we already captured the deltas).
///
/// See `packages/coding-agent/docs/json.md` in the pi-mono repo and
/// `AssistantMessageEvent` in `packages/ai/src/types.ts` for the full
/// schema.
struct PiStreamJsonParser {
    /// Accumulated text_delta chunks across the entire iteration.
    text_accumulator: String,
}

impl PiStreamJsonParser {
    fn new() -> Self {
        Self {
            text_accumulator: String::new(),
        }
    }
}

impl HarnessStreamParser for PiStreamJsonParser {
    fn process_line(&mut self, line: &str) -> Option<String> {
        let value: serde_json::Value = serde_json::from_str(line).ok()?;

        // Only message_update events carry assistant deltas.
        if value.get("type")?.as_str()? != "message_update" {
            return None;
        }

        let event = value.get("assistantMessageEvent")?;
        if event.get("type")?.as_str()? != "text_delta" {
            return None;
        }

        let delta = event.get("delta")?.as_str()?;
        if delta.is_empty() {
            return None;
        }

        self.text_accumulator.push_str(delta);
        Some(delta.to_string())
    }

    fn check_stop_signal(&self) -> Option<IterationResult> {
        scan_stop_signal(&self.text_accumulator)
    }
}

/// Headless runner that invokes a coding-agent harness in print/pipe mode
/// instead of via a PTY.
pub struct HeadlessRunner {
    config: Config,
    prompt: ResolvedPrompt,
    should_stop: Arc<AtomicBool>,
    logger: Option<SessionLogger>,
    plan_name: Option<String>,
    scratchpad_path: Option<PathBuf>,
    harness: Harness,
}

impl HeadlessRunner {
    pub fn new(
        config: Config,
        prompt: ResolvedPrompt,
        plan_name: Option<String>,
        scratchpad_path: Option<PathBuf>,
        harness: Harness,
    ) -> Self {
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
            scratchpad_path,
            harness,
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

    /// Append a timeout note to the scratchpad so the next iteration knows to check logs
    fn append_timeout_to_scratchpad(&self, iteration: u32) {
        let Some(ref scratchpad_path) = self.scratchpad_path else {
            return;
        };
        let log_path = self
            .logger
            .as_ref()
            .map(|l| l.path.display().to_string())
            .unwrap_or_else(|| "the session log".to_string());
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        let note = format!(
            "\n## ⚠ Timeout — Iteration {} ({})\n\n\
             The previous iteration (#{}) was terminated due to timeout ({}s limit).\n\
             **Next iteration**: Check the logs at `{}` to understand what was in progress \
             and resume or retry the interrupted work.\n",
            iteration, timestamp, iteration, self.config.timeout_seconds, log_path,
        );
        if let Err(e) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(scratchpad_path)
            .and_then(|mut f| f.write_all(note.as_bytes()))
        {
            eprintln!(
                "[hydra] Warning: Could not write timeout note to scratchpad: {}",
                e
            );
        }
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
            "[hydra] Iteration {}/{}... [{}]",
            iteration,
            self.config.max_iterations,
            Local::now().format("%Y-%m-%d %H:%M:%S")
        );

        let combined_prompt = self.create_combined_prompt();

        // Spawn the configured harness in print/pipe mode with stream-json
        // output. The Harness abstraction provides the command name, the
        // argument list, and any env vars that must be cleared before
        // spawning a nested session.
        let mut cmd = Command::new(self.harness.command());
        cmd.args(self.harness.headless_args());
        for var in self.harness.env_removals() {
            cmd.env_remove(var);
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| HydraError::io(format!("spawning {} -p", self.harness.command()), e))?;

        // Track child PID for signal handling
        let child_id = child.id();
        signal::set_child_pid(child_id);

        // Write prompt to stdin, then close it
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(combined_prompt.as_bytes()).map_err(|e| {
                HydraError::io(
                    format!("writing prompt to {} stdin", self.harness.command()),
                    e,
                )
            })?;
            // stdin is dropped here, closing the pipe
        }

        // Read stdout through the harness-specific parser
        let stdout = child.stdout.take().ok_or_else(|| {
            HydraError::io(
                format!("taking {} stdout", self.harness.command()),
                std::io::Error::other("no stdout"),
            )
        })?;

        let reader = BufReader::new(stdout);
        // Pick the parser matching the active harness. Both implement
        // HarnessStreamParser so the loop body below stays identical.
        let mut parser: Box<dyn HarnessStreamParser> = match self.harness {
            Harness::Claude => Box::new(StreamJsonParser::new()),
            Harness::Pi => Box::new(PiStreamJsonParser::new()),
        };
        let mut result = IterationResult::NoSignal;

        // Set up timeout
        let timeout_secs = self.config.timeout_seconds;
        let start_time = std::time::Instant::now();

        for line in reader.lines() {
            // Check timeout
            if start_time.elapsed().as_secs() >= timeout_secs {
                eprintln!(
                    "[hydra] Iteration timeout ({timeout_secs}s), terminating {} process",
                    self.harness.command()
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
                IterationResult::Timeout => {
                    self.append_timeout_to_scratchpad(iteration);
                    self.should_stop.store(false, Ordering::SeqCst);
                    if self.config.verbose {
                        eprintln!("[hydra:debug] Timeout recorded in scratchpad, continuing");
                    }
                }
                IterationResult::TaskComplete | IterationResult::NoSignal => {
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

    // ----- PiStreamJsonParser tests ---------------------------------------

    #[test]
    fn test_pi_parser_text_delta_extracts_delta() {
        let mut parser = PiStreamJsonParser::new();
        let line = r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":1,"delta":"Hello "}}"#;
        let result = parser.process_line(line);
        assert_eq!(result, Some("Hello ".to_string()));
        assert_eq!(parser.text_accumulator, "Hello ");

        let line = r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":1,"delta":"world!"}}"#;
        let result = parser.process_line(line);
        assert_eq!(result, Some("world!".to_string()));
        assert_eq!(parser.text_accumulator, "Hello world!");
    }

    #[test]
    fn test_pi_parser_ignores_non_text_delta_events() {
        let mut parser = PiStreamJsonParser::new();

        // Session header
        let line = r#"{"type":"session","version":3,"id":"uuid","cwd":"/"}"#;
        assert!(parser.process_line(line).is_none());

        // Lifecycle events
        assert!(parser.process_line(r#"{"type":"agent_start"}"#).is_none());
        assert!(parser.process_line(r#"{"type":"turn_start"}"#).is_none());
        assert!(
            parser
                .process_line(r#"{"type":"message_start","message":{}}"#)
                .is_none()
        );
        assert!(parser.process_line(r#"{"type":"agent_end"}"#).is_none());

        // thinking_delta should be ignored
        let line = r#"{"type":"message_update","assistantMessageEvent":{"type":"thinking_delta","contentIndex":0,"delta":"hmm"}}"#;
        assert!(parser.process_line(line).is_none());

        // toolcall_delta should be ignored
        let line = r#"{"type":"message_update","assistantMessageEvent":{"type":"toolcall_delta","contentIndex":0,"delta":"{\"cmd\":\"ls\"}"}}"#;
        assert!(parser.process_line(line).is_none());

        // text_start / text_end are bookends (no new chunk to append)
        let line = r#"{"type":"message_update","assistantMessageEvent":{"type":"text_start","contentIndex":1}}"#;
        assert!(parser.process_line(line).is_none());
        let line = r#"{"type":"message_update","assistantMessageEvent":{"type":"text_end","contentIndex":1,"content":"final"}}"#;
        assert!(parser.process_line(line).is_none());

        // Invalid JSON
        assert!(parser.process_line("not json").is_none());
        // Missing fields
        assert!(
            parser
                .process_line(r#"{"type":"message_update"}"#)
                .is_none()
        );

        assert_eq!(parser.text_accumulator, "");
    }

    #[test]
    fn test_pi_parser_empty_delta_returns_none() {
        let mut parser = PiStreamJsonParser::new();
        let line = r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":""}}"#;
        assert!(parser.process_line(line).is_none());
        assert_eq!(parser.text_accumulator, "");
    }

    #[test]
    fn test_pi_parser_stop_signals() {
        let mut parser = PiStreamJsonParser::new();
        assert!(parser.check_stop_signal().is_none());

        // Simulate a stream that ends with TASK_COMPLETE. The raw string
        // delimiters use four hashes because the payload contains `"###`
        // sequences (any `"` followed by >= N hashes would otherwise close
        // an `r##`-delimited literal).
        let lines = [
            r####"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":"Working...\n"}}"####,
            r####"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":"###TASK_COMPLETE###"}}"####,
        ];
        for line in lines {
            parser.process_line(line);
        }
        assert_eq!(
            parser.check_stop_signal(),
            Some(IterationResult::TaskComplete)
        );

        // And ALL_TASKS_COMPLETE takes priority
        parser.process_line(
            r####"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":"\n###ALL_TASKS_COMPLETE###"}}"####,
        );
        assert_eq!(
            parser.check_stop_signal(),
            Some(IterationResult::AllComplete)
        );
    }

    #[test]
    fn test_pi_parser_all_complete_takes_priority() {
        // Mirror of test_stream_json_parser_all_complete_takes_priority:
        // when both stop signals appear in the accumulator, ALL_TASKS_COMPLETE
        // must win. Simulate it by feeding two text_delta events.
        let mut parser = PiStreamJsonParser::new();
        parser.process_line(
            r####"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":"###TASK_COMPLETE###\n"}}"####,
        );
        parser.process_line(
            r####"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":"###ALL_TASKS_COMPLETE###\n"}}"####,
        );
        assert_eq!(
            parser.check_stop_signal(),
            Some(IterationResult::AllComplete)
        );
    }

    #[test]
    fn test_pi_parser_interleaved_text_and_tool_deltas() {
        // Mirror of test_stream_json_parser_mixed_content_blocks: when a
        // pi stream interleaves text_delta events with thinking/toolcall
        // deltas, only the text deltas are extracted and accumulated.
        let mut parser = PiStreamJsonParser::new();
        let lines = [
            r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":1,"delta":"Reading file..."}}"#,
            r#"{"type":"message_update","assistantMessageEvent":{"type":"toolcall_start","contentIndex":2,"name":"Read"}}"#,
            r#"{"type":"message_update","assistantMessageEvent":{"type":"toolcall_delta","contentIndex":2,"delta":"{\"path\":\"foo.rs\"}"}}"#,
            r#"{"type":"message_update","assistantMessageEvent":{"type":"toolcall_end","contentIndex":2}}"#,
            r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":3,"delta":" done."}}"#,
        ];
        let mut extracted = Vec::new();
        for line in lines {
            if let Some(t) = parser.process_line(line) {
                extracted.push(t);
            }
        }
        assert_eq!(
            extracted,
            vec!["Reading file...".to_string(), " done.".to_string()]
        );
        assert_eq!(parser.text_accumulator, "Reading file... done.");
    }

    /// Exercise the same `Box<dyn HarnessStreamParser>` dispatch path that
    /// `HeadlessRunner::run_iteration` uses. This is a lightweight
    /// integration test that verifies the runner's per-harness parser
    /// selection is wired correctly: a claude-shaped event only produces
    /// output through the claude parser, and a pi-shaped event only
    /// produces output through the pi parser. Without a real child
    /// process it's the closest thing to a cross-harness integration test
    /// we can run in CI.
    #[test]
    fn test_harness_dispatch_to_correct_parser() {
        let claude_line = r#"{"type":"assistant","message":{"content":[{"text":"hi claude"}]}}"#;
        let pi_line = r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":"hi pi"}}"#;

        for harness in [Harness::Claude, Harness::Pi] {
            let mut parser: Box<dyn HarnessStreamParser> = match harness {
                Harness::Claude => Box::new(StreamJsonParser::new()),
                Harness::Pi => Box::new(PiStreamJsonParser::new()),
            };

            match harness {
                Harness::Claude => {
                    assert_eq!(
                        parser.process_line(claude_line),
                        Some("hi claude".to_string()),
                        "claude parser should extract claude-shaped text"
                    );
                    assert!(
                        parser.process_line(pi_line).is_none(),
                        "claude parser must ignore pi-shaped events"
                    );
                }
                Harness::Pi => {
                    assert_eq!(
                        parser.process_line(pi_line),
                        Some("hi pi".to_string()),
                        "pi parser should extract pi-shaped text"
                    );
                    assert!(
                        parser.process_line(claude_line).is_none(),
                        "pi parser must ignore claude-shaped events"
                    );
                }
            }

            // And both parsers must share stop-signal detection semantics
            // via the common `scan_stop_signal` helper.
            assert!(parser.check_stop_signal().is_none());
        }
    }

    #[test]
    fn test_pi_parser_realistic_sample() {
        // Captured from a real `pi -p --mode json --no-tools` invocation
        // that was asked to reply with just "DONE". The stream includes
        // session header, agent/turn lifecycle events, thinking bookends,
        // and a text_start / text_delta / text_end triplet for the final
        // answer. Only the text_delta should produce output.
        let sample = r#"{"type":"session","version":3,"id":"uuid","timestamp":"...","cwd":"/tmp"}
{"type":"agent_start"}
{"type":"turn_start"}
{"type":"message_start","message":{"role":"user","content":[]}}
{"type":"message_end","message":{"role":"user","content":[]}}
{"type":"message_start","message":{"role":"assistant","content":[]}}
{"type":"message_update","assistantMessageEvent":{"type":"thinking_start","contentIndex":0}}
{"type":"message_update","assistantMessageEvent":{"type":"thinking_end","contentIndex":0,"content":""}}
{"type":"message_update","assistantMessageEvent":{"type":"text_start","contentIndex":1}}
{"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":1,"delta":"DONE"}}
{"type":"message_update","assistantMessageEvent":{"type":"text_end","contentIndex":1,"content":"DONE"}}
{"type":"message_end","message":{"role":"assistant","content":[]}}
{"type":"turn_end","message":{},"toolResults":[]}
{"type":"agent_end","messages":[]}"#;

        let mut parser = PiStreamJsonParser::new();
        let mut extracted = Vec::new();
        for line in sample.lines() {
            if let Some(text) = parser.process_line(line) {
                extracted.push(text);
            }
        }
        assert_eq!(extracted, vec!["DONE".to_string()]);
        assert_eq!(parser.text_accumulator, "DONE");
    }
}
