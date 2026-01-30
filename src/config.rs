use crate::error::{HydraError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration for hydra, loaded from TOML file with defaults
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Maximum number of iterations to run
    pub max_iterations: u32,

    /// Enable verbose/debug output
    pub verbose: bool,

    /// Name of the stop file to check for graceful shutdown
    pub stop_file: String,

    /// Timeout per iteration in seconds (default: 1200 = 20 minutes)
    pub timeout_seconds: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            verbose: false,
            stop_file: ".hydra-stop".to_string(),
            timeout_seconds: 1200, // 20 minutes
        }
    }
}

impl Config {
    /// Load config from the global config file (~/.hydra/config.toml)
    /// Returns default config if file doesn't exist
    pub fn load() -> Result<Self> {
        let config_path = Self::global_config_path();

        if !config_path.exists() {
            return Ok(Self::default());
        }

        Self::load_from_path(&config_path)
    }

    /// Load config from a specific path
    pub fn load_from_path(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            HydraError::io(format!("reading config file {}", path.display()), e)
        })?;

        toml::from_str(&content).map_err(|e| HydraError::ConfigParse {
            path: path.clone(),
            source: e,
        })
    }

    /// Get the path to the global config file
    pub fn global_config_path() -> PathBuf {
        Self::global_hydra_dir().join("config.toml")
    }

    /// Get the path to the global hydra directory (~/.hydra)
    pub fn global_hydra_dir() -> PathBuf {
        dirs::home_dir()
            .expect("Could not determine home directory")
            .join(".hydra")
    }

    /// Get the path to the global default prompt file
    pub fn global_default_prompt_path() -> PathBuf {
        Self::global_hydra_dir().join("default-prompt.md")
    }

    /// Get the path to the local hydra directory (./.hydra)
    pub fn local_hydra_dir() -> PathBuf {
        PathBuf::from(".hydra")
    }

    /// Get the path to the local project prompt file (./.hydra/prompt.md)
    pub fn local_prompt_path() -> PathBuf {
        Self::local_hydra_dir().join("prompt.md")
    }

    /// Get the path to the logs directory (./.hydra/logs)
    pub fn logs_dir() -> PathBuf {
        Self::local_hydra_dir().join("logs")
    }

    /// Merge CLI options over config values
    /// CLI options take precedence when provided
    pub fn merge_cli(&mut self, max: Option<u32>, verbose: bool, timeout: Option<u64>) {
        if let Some(m) = max {
            self.max_iterations = m;
        }
        // verbose is additive: true from either source enables it
        if verbose {
            self.verbose = true;
        }
        if let Some(t) = timeout {
            self.timeout_seconds = t;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.max_iterations, 10);
        assert!(!config.verbose);
        assert_eq!(config.stop_file, ".hydra-stop");
        assert_eq!(config.timeout_seconds, 1200);
    }

    #[test]
    fn test_load_from_toml() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        fs::write(
            &config_path,
            r#"
max_iterations = 20
verbose = true
stop_file = ".custom-stop"
timeout_seconds = 600
"#,
        )
        .unwrap();

        let config = Config::load_from_path(&config_path).unwrap();
        assert_eq!(config.max_iterations, 20);
        assert!(config.verbose);
        assert_eq!(config.stop_file, ".custom-stop");
        assert_eq!(config.timeout_seconds, 600);
    }

    #[test]
    fn test_partial_toml_uses_defaults() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        // Only specify one field, others should use defaults
        fs::write(&config_path, "max_iterations = 5\n").unwrap();

        let config = Config::load_from_path(&config_path).unwrap();
        assert_eq!(config.max_iterations, 5);
        assert!(!config.verbose); // default
        assert_eq!(config.stop_file, ".hydra-stop"); // default
    }

    #[test]
    fn test_merge_cli() {
        let mut config = Config::default();
        assert_eq!(config.max_iterations, 10);
        assert!(!config.verbose);
        assert_eq!(config.timeout_seconds, 1200);

        // Merge with CLI options
        config.merge_cli(Some(25), true, Some(300));
        assert_eq!(config.max_iterations, 25);
        assert!(config.verbose);
        assert_eq!(config.timeout_seconds, 300);

        // Merge with None keeps existing value
        config.merge_cli(None, false, None);
        assert_eq!(config.max_iterations, 25);
        assert!(config.verbose); // verbose stays true once enabled
        assert_eq!(config.timeout_seconds, 300);
    }

    #[test]
    fn test_invalid_toml_error() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        fs::write(&config_path, "invalid toml {{{{").unwrap();

        let result = Config::load_from_path(&config_path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, HydraError::ConfigParse { .. }));
    }

    #[test]
    fn test_path_helpers() {
        // Just verify they return non-empty paths
        assert!(!Config::global_hydra_dir().as_os_str().is_empty());
        assert!(Config::global_config_path().ends_with("config.toml"));
        assert!(Config::global_default_prompt_path().ends_with("default-prompt.md"));
        assert_eq!(Config::local_hydra_dir(), PathBuf::from(".hydra"));
        assert!(Config::local_prompt_path().ends_with("prompt.md"));
        assert!(Config::logs_dir().ends_with("logs"));
    }
}
