use crate::config::Config;
use crate::error::{HydraError, Result};
use crate::prompt::ResolvedPrompt;
use crate::pty::{PtyManager, PtyResult};
use chrono::Local;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tempfile::NamedTempFile;

/// Iteration instructions prepended to the prompt
const ITERATION_INSTRUCTIONS: &str = r#"╔══════════════════════════════════════════════════════════════════════════════╗
║                           hydra ITERATION INSTRUCTIONS                       ║
╚══════════════════════════════════════════════════════════════════════════════╝

You are running inside hydra, an automated task runner.

YOUR TASK:
1. Review the implementation plan referenced in the prompt below
2. Pick the highest-leverage task that is not yet complete
3. Complete that ONE task thoroughly
4. Mark the task as completed in the plan
4. Signal completion with the appropriate stop sequence

STOP SEQUENCES (output on its own line when done):

  ###TASK_COMPLETE###
  Use this when you have completed the current task but MORE tasks remain.
  hydra will start a new iteration for the next task.

  ###ALL_TASKS_COMPLETE###
  Use this when ALL tasks in the implementation plan are complete.
  hydra will end the session.

IMPORTANT:
- Complete only ONE task per iteration
- Always output exactly one of the two stop sequences when finished
- Mark the task as completed in the plan when finished
- Work AUTONOMOUSLY - do NOT ask the user for input or confirmation
- Make decisions yourself and proceed with the implementation
- Do NOT use AskUserQuestion or similar tools that require user input

────────────────────────────────────────────────────────────────────────────────
"#;

/// Result of a single iteration
#[derive(Debug, Clone, PartialEq)]
pub enum IterationResult {
    /// Task complete signal detected, more tasks remain
    TaskComplete,
    /// All tasks complete signal detected
    AllComplete,
    /// No signal detected, process ended naturally
    NoSignal,
    /// Process was terminated (by signal or stop file)
    Terminated,
}

/// Result of the entire run loop
#[derive(Debug)]
pub enum RunResult {
    /// All tasks completed successfully
    AllTasksComplete { iterations: u32 },
    /// Max iterations reached
    MaxIterations { iterations: u32 },
    /// Stopped gracefully (SIGTERM or stop file)
    Stopped { iterations: u32 },
    /// Interrupted (SIGINT)
    Interrupted,
}

/// Session logger for writing output to `.hydra/logs/`
struct SessionLogger {
    /// Path to the log file
    path: PathBuf,
    /// Open file handle for appending
    file: File,
}

impl SessionLogger {
    /// Create a new session logger with timestamp-based filename
    fn new() -> Result<Self> {
        let logs_dir = Config::logs_dir();

        // Create logs directory if it doesn't exist
        if !logs_dir.exists() {
            fs::create_dir_all(&logs_dir)
                .map_err(|e| HydraError::io(format!("creating logs directory {}", logs_dir.display()), e))?;
        }

        // Generate timestamp-based filename: hydra-YYYYMMDD-HHMMSS.log
        let timestamp = Local::now().format("%Y%m%d-%H%M%S");
        let filename = format!("hydra-{}.log", timestamp);
        let path = logs_dir.join(filename);

        // Open file for writing (create if doesn't exist)
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)
            .map_err(|e| HydraError::io(format!("creating log file {}", path.display()), e))?;

        Ok(Self { path, file })
    }

    /// Get the path to the log file
    fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Write a message to the log
    fn log(&mut self, message: &str) -> Result<()> {
        let timestamp = Local::now().format("%H:%M:%S");
        writeln!(self.file, "[{}] {}", timestamp, message)
            .map_err(|e| HydraError::io("writing to log file", e))?;
        self.file.flush()
            .map_err(|e| HydraError::io("flushing log file", e))?;
        Ok(())
    }

    /// Append raw content to the log (for iteration output)
    fn append_content(&mut self, content: &str) -> Result<()> {
        write!(self.file, "{}", content)
            .map_err(|e| HydraError::io("writing content to log file", e))?;
        self.file.flush()
            .map_err(|e| HydraError::io("flushing log file", e))?;
        Ok(())
    }

    /// Write iteration header to the log
    fn log_iteration_start(&mut self, iteration: u32, max: u32) -> Result<()> {
        let separator = "=".repeat(80);
        self.append_content(&format!("\n{}\n", separator))?;
        self.log(&format!("ITERATION {}/{} START", iteration, max))?;
        self.append_content(&format!("{}\n\n", separator))?;
        Ok(())
    }

    /// Write iteration end to the log
    fn log_iteration_end(&mut self, iteration: u32, result: &IterationResult) -> Result<()> {
        let result_str = match result {
            IterationResult::TaskComplete => "TASK_COMPLETE",
            IterationResult::AllComplete => "ALL_COMPLETE",
            IterationResult::NoSignal => "NO_SIGNAL",
            IterationResult::Terminated => "TERMINATED",
        };
        self.log(&format!("ITERATION {} END: {}", iteration, result_str))?;
        Ok(())
    }
}

/// The runner that executes Claude in a loop
pub struct Runner {
    config: Config,
    prompt: ResolvedPrompt,
    should_stop: Arc<AtomicBool>,
    logger: Option<SessionLogger>,
}

impl Runner {
    /// Create a new runner with the given configuration and prompt
    pub fn new(config: Config, prompt: ResolvedPrompt) -> Self {
        // Try to create the session logger, but don't fail if it doesn't work
        let logger = match SessionLogger::new() {
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
        }
    }

    /// Get the path to the session log file, if logging is enabled
    pub fn log_path(&self) -> Option<&PathBuf> {
        self.logger.as_ref().map(|l| l.path())
    }

    /// Get a clone of the stop flag for signal handlers
    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.should_stop)
    }

    /// Request the runner to stop after current iteration
    pub fn request_stop(&self) {
        self.should_stop.store(true, Ordering::SeqCst);
    }

    /// Create a combined prompt file with iteration instructions
    fn create_combined_prompt(&self) -> Result<NamedTempFile> {
        let combined = format!("{}\n{}", ITERATION_INSTRUCTIONS, self.prompt.content);

        let mut temp =
            NamedTempFile::new().map_err(|e| HydraError::io("creating temp prompt file", e))?;

        use std::io::Write;
        temp.write_all(combined.as_bytes())
            .map_err(|e| HydraError::io("writing combined prompt", e))?;

        Ok(temp)
    }

    /// Check if the stop file exists
    fn check_stop_file(&self) -> bool {
        let stop_path = PathBuf::from(&self.config.stop_file);
        if stop_path.exists() {
            // Remove the stop file
            let _ = fs::remove_file(&stop_path);
            true
        } else {
            false
        }
    }

    /// Run a single iteration and return the result
    fn run_iteration(&mut self, iteration: u32) -> Result<IterationResult> {
        if self.config.verbose {
            eprintln!("[hydra:debug] Starting iteration {}", iteration);
        }

        println!("[hydra] Run #{} starting...", iteration);

        // Create the combined prompt file
        let prompt_file = self.create_combined_prompt()?;

        // Create output file for capturing Claude's output
        let output_file =
            NamedTempFile::new().map_err(|e| HydraError::io("creating output file", e))?;

        // Create PTY manager and spawn Claude
        let mut pty = PtyManager::new(Arc::clone(&self.should_stop))?;
        pty.spawn_claude(prompt_file.path())?;

        // Run the I/O loop (handles stdin, stdout, and signal detection)
        let output_path = output_file.path().to_path_buf();
        let pty_result = pty.run_io_loop(&output_path, self.config.verbose)?;

        // Convert PtyResult to IterationResult
        let result = match pty_result {
            PtyResult::TaskComplete => IterationResult::TaskComplete,
            PtyResult::AllComplete => IterationResult::AllComplete,
            PtyResult::NoSignal => IterationResult::NoSignal,
            PtyResult::Terminated => IterationResult::Terminated,
        };

        // Copy iteration output to session log
        if let Some(ref mut logger) = self.logger {
            if let Ok(output_content) = fs::read_to_string(&output_path) {
                let _ = logger.append_content(&output_content);
            }
        }

        println!("[hydra] Run #{} complete", iteration);

        Ok(result)
    }

    /// Run the main loop
    pub fn run(&mut self) -> Result<RunResult> {
        let max = self.config.max_iterations;

        println!("[hydra] Starting automated task runner");
        println!("[hydra] Using prompt file: {}", self.prompt.path.display());
        if let Some(ref logger) = self.logger {
            println!("[hydra] Session log: {}", logger.path.display());
        }
        println!("[hydra] Claude controls task selection from implementation plan");

        // Log session start
        if let Some(ref mut logger) = self.logger {
            let _ = logger.log(&format!("Session started - max iterations: {}", max));
            let _ = logger.log(&format!("Prompt file: {}", self.prompt.path.display()));
        }

        for iteration in 1..=max {
            // Check for stop file before each iteration
            if self.check_stop_file() {
                println!("[hydra] Stop file detected, exiting gracefully");
                if let Some(ref mut logger) = self.logger {
                    let _ = logger.log("Session ended: stop file detected");
                }
                return Ok(RunResult::Stopped {
                    iterations: iteration - 1,
                });
            }

            // Check for graceful stop request (SIGTERM)
            if self.should_stop.load(Ordering::SeqCst) {
                println!("[hydra] Graceful shutdown complete");
                if let Some(ref mut logger) = self.logger {
                    let _ = logger.log("Session ended: graceful shutdown");
                }
                return Ok(RunResult::Stopped {
                    iterations: iteration - 1,
                });
            }

            // Display iteration header
            println!();
            println!("=== Iteration {}/{} ===", iteration, max);
            println!();

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
                    println!("[hydra] All tasks complete! Total runs: {}", iteration);
                    if let Some(ref mut logger) = self.logger {
                        let _ = logger.log(&format!("Session ended: all tasks complete after {} iterations", iteration));
                    }
                    return Ok(RunResult::AllTasksComplete { iterations: iteration });
                }
                IterationResult::Terminated => {
                    println!("[hydra] Graceful shutdown complete");
                    if let Some(ref mut logger) = self.logger {
                        let _ = logger.log("Session ended: terminated");
                    }
                    return Ok(RunResult::Stopped { iterations: iteration });
                }
                IterationResult::TaskComplete | IterationResult::NoSignal => {
                    // Continue to next iteration
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
    use crate::prompt::PromptSource;

    fn test_config() -> Config {
        Config {
            max_iterations: 3,
            verbose: false,
            stop_file: ".hydra-stop-test".to_string(),
        }
    }

    fn test_prompt() -> ResolvedPrompt {
        ResolvedPrompt {
            path: PathBuf::from("test-prompt.md"),
            content: "Test prompt content".to_string(),
            source: PromptSource::CurrentDir,
        }
    }

    #[test]
    fn test_runner_creation() {
        let config = test_config();
        let prompt = test_prompt();
        let runner = Runner::new(config, prompt);

        assert!(!runner.should_stop.load(Ordering::SeqCst));
    }

    #[test]
    fn test_stop_flag() {
        let config = test_config();
        let prompt = test_prompt();
        let runner = Runner::new(config, prompt);

        let flag = runner.stop_flag();
        assert!(!flag.load(Ordering::SeqCst));

        runner.request_stop();
        assert!(flag.load(Ordering::SeqCst));
    }

    #[test]
    fn test_iteration_instructions_contains_signals() {
        assert!(ITERATION_INSTRUCTIONS.contains("###TASK_COMPLETE###"));
        assert!(ITERATION_INSTRUCTIONS.contains("###ALL_TASKS_COMPLETE###"));
    }

    #[test]
    fn test_session_logger_log_format() {
        // Test the log message format
        let temp_dir = tempfile::tempdir().unwrap();
        let log_path = temp_dir.path().join("test.log");
        let file = File::create(&log_path).unwrap();

        let mut logger = SessionLogger {
            path: log_path.clone(),
            file,
        };

        logger.log("Test message").unwrap();
        logger.append_content("Raw content\n").unwrap();

        let content = fs::read_to_string(&log_path).unwrap();
        // Log messages have timestamp prefix
        assert!(content.contains("] Test message"));
        assert!(content.contains("Raw content"));
    }

    #[test]
    fn test_session_logger_iteration_markers() {
        let temp_dir = tempfile::tempdir().unwrap();
        let log_path = temp_dir.path().join("test.log");
        let file = File::create(&log_path).unwrap();

        let mut logger = SessionLogger {
            path: log_path.clone(),
            file,
        };

        logger.log_iteration_start(1, 10).unwrap();
        logger.log_iteration_end(1, &IterationResult::TaskComplete).unwrap();

        let content = fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("ITERATION 1/10 START"));
        assert!(content.contains("ITERATION 1 END: TASK_COMPLETE"));
    }

    #[test]
    fn test_session_logger_all_result_types() {
        let temp_dir = tempfile::tempdir().unwrap();
        let log_path = temp_dir.path().join("test.log");
        let file = File::create(&log_path).unwrap();

        let mut logger = SessionLogger {
            path: log_path.clone(),
            file,
        };

        logger.log_iteration_end(1, &IterationResult::TaskComplete).unwrap();
        logger.log_iteration_end(2, &IterationResult::AllComplete).unwrap();
        logger.log_iteration_end(3, &IterationResult::NoSignal).unwrap();
        logger.log_iteration_end(4, &IterationResult::Terminated).unwrap();

        let content = fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("TASK_COMPLETE"));
        assert!(content.contains("ALL_COMPLETE"));
        assert!(content.contains("NO_SIGNAL"));
        assert!(content.contains("TERMINATED"));
    }
}
