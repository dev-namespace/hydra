use std::path::PathBuf;
use thiserror::Error;

/// Exit codes as per spec
pub const EXIT_SUCCESS: i32 = 0;
pub const EXIT_STOPPED: i32 = 1;
pub const EXIT_ERROR: i32 = 2;

/// All possible errors in hydra
#[derive(Error, Debug)]
pub enum HydraError {
    /// No prompt file found at any priority level
    #[error(
        "No prompt file found. Searched:\n  - CLI --prompt flag\n  - ./.hydra/prompt.md\n  - ./prompt.md\n  - ~/.hydra/default-prompt.md"
    )]
    NoPromptFound,

    /// Prompt file specified but doesn't exist
    #[error("Prompt file not found: {0}")]
    PromptNotFound(PathBuf),

    /// Plan file specified but doesn't exist
    #[error(
        "Plan file not found: {0}\n\nMake sure the implementation plan file exists at the specified path."
    )]
    PlanNotFound(PathBuf),

    /// Config file parse error
    #[error("Failed to parse config file {path}: {source}")]
    ConfigParse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    /// IO error with context
    #[error("IO error: {context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    /// User interrupted with SIGINT
    #[error("Interrupted by user (Ctrl+C)")]
    Interrupted,

    /// Graceful shutdown via SIGTERM or stop file
    #[error("Stopped gracefully")]
    GracefulStop,

    /// Max iterations reached
    #[allow(dead_code)]
    #[error("Max iterations ({0}) reached")]
    MaxIterations(u32),

    /// Failed to spawn subprocess
    #[allow(dead_code)]
    #[error("Failed to spawn subprocess: {0}")]
    SpawnFailed(#[source] std::io::Error),

    /// Subprocess exited with error
    #[allow(dead_code)]
    #[error("Subprocess exited with code {0}")]
    SubprocessFailed(i32),
}

impl HydraError {
    /// Map error to appropriate exit code per spec
    pub fn exit_code(&self) -> i32 {
        match self {
            // Exit 0: Success conditions
            HydraError::MaxIterations(_) => EXIT_SUCCESS,

            // Exit 1: Stopped by user/signal
            HydraError::Interrupted => EXIT_STOPPED,
            HydraError::GracefulStop => EXIT_STOPPED,

            // Exit 2: Errors
            HydraError::NoPromptFound => EXIT_ERROR,
            HydraError::PromptNotFound(_) => EXIT_ERROR,
            HydraError::PlanNotFound(_) => EXIT_ERROR,
            HydraError::ConfigParse { .. } => EXIT_ERROR,
            HydraError::Io { .. } => EXIT_ERROR,
            HydraError::SpawnFailed(_) => EXIT_ERROR,
            HydraError::SubprocessFailed(_) => EXIT_ERROR,
        }
    }

    /// Helper to create IO error with context
    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        HydraError::Io {
            context: context.into(),
            source,
        }
    }
}

/// Result type alias for hydra operations
pub type Result<T> = std::result::Result<T, HydraError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exit_codes() {
        // Success conditions
        assert_eq!(HydraError::MaxIterations(10).exit_code(), EXIT_SUCCESS);

        // Stopped conditions
        assert_eq!(HydraError::Interrupted.exit_code(), EXIT_STOPPED);
        assert_eq!(HydraError::GracefulStop.exit_code(), EXIT_STOPPED);

        // Error conditions
        assert_eq!(HydraError::NoPromptFound.exit_code(), EXIT_ERROR);
        assert_eq!(
            HydraError::PromptNotFound(PathBuf::from("test.md")).exit_code(),
            EXIT_ERROR
        );
        assert_eq!(
            HydraError::PlanNotFound(PathBuf::from("plan.md")).exit_code(),
            EXIT_ERROR
        );
    }

    #[test]
    fn test_error_display() {
        let err = HydraError::NoPromptFound;
        assert!(err.to_string().contains("No prompt file found"));

        let err = HydraError::PromptNotFound(PathBuf::from("/foo/bar.md"));
        assert!(err.to_string().contains("/foo/bar.md"));

        let err = HydraError::MaxIterations(5);
        assert!(err.to_string().contains("5"));
    }

    #[test]
    fn test_plan_not_found_error_display() {
        let err = HydraError::PlanNotFound(PathBuf::from("/foo/plan.md"));
        let msg = err.to_string();
        assert!(msg.contains("/foo/plan.md"));
        assert!(msg.contains("Plan file not found"));
    }
}
