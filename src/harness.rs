//! Harness abstraction for hydra.
//!
//! Hydra supports multiple coding-agent harnesses (Claude Code, Pi, ...).
//! Everything that differs between harnesses — command name, arguments,
//! stream-json parsing, env var cleanup, review command building — is
//! encapsulated behind this abstraction so the rest of hydra can stay
//! harness-agnostic.
//!
//! Milestone 1 of the pi-harness roadmap introduced the abstraction layer
//! with ClaudeHarness as the only fully-implemented backend. Milestone 2
//! implements the pi PTY code-path (`pi @<prompt-file>`). Milestone 3
//! implements the pi headless code-path (`pi -p --mode json`) with a
//! dedicated stream-json parser for pi's `text_delta` events. Plan review
//! for pi lands in milestone 4.

use crate::error::{HydraError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Which coding agent harness to drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Harness {
    /// Claude Code (`claude` CLI). Default.
    Claude,
    /// Pi coding agent (`pi` CLI).
    Pi,
}

impl Harness {
    /// Parse a harness name. Accepts `claude` or `pi` (case-insensitive).
    pub fn parse(name: &str) -> Result<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "claude" => Ok(Harness::Claude),
            "pi" => Ok(Harness::Pi),
            other => Err(HydraError::io(
                format!("unknown harness '{}' (expected 'claude' or 'pi')", other),
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid harness name"),
            )),
        }
    }

    /// Human-readable name (matches the CLI / config value).
    pub fn name(self) -> &'static str {
        match self {
            Harness::Claude => "claude",
            Harness::Pi => "pi",
        }
    }

    /// Binary to invoke for this harness.
    pub fn command(self) -> &'static str {
        match self {
            Harness::Claude => "claude",
            // Pi full implementation lands in milestone 2/3. The binary name
            // is recorded here so the abstraction is in place.
            Harness::Pi => "pi",
        }
    }

    /// Arguments to pass when spawning the harness inside a PTY for an
    /// interactive iteration that reads a prompt file.
    ///
    /// Returns the list of arguments to append after the binary name.
    pub fn pty_args(self, prompt_path: &Path) -> Vec<String> {
        match self {
            Harness::Claude => vec![
                "--dangerously-skip-permissions".to_string(),
                format!("read instructions here: {}", prompt_path.display()),
            ],
            Harness::Pi => {
                // Pi accepts `@<file>` as a file argument whose contents
                // become the initial message, auto-submitted before the
                // interactive loop starts. Pi manages its own tool
                // permissions, so no `--dangerously-skip-permissions`
                // equivalent is needed.
                vec![format!("@{}", prompt_path.display())]
            }
        }
    }

    /// Arguments to pass when spawning the harness in headless / print mode
    /// (prompt delivered via stdin).
    pub fn headless_args(self) -> Vec<String> {
        match self {
            Harness::Claude => vec![
                "-p".to_string(),
                "--dangerously-skip-permissions".to_string(),
                "--output-format".to_string(),
                "stream-json".to_string(),
                "--verbose".to_string(),
            ],
            Harness::Pi => {
                // Pi's JSON event-stream mode. `-p` enables non-interactive
                // print mode and `--mode json` selects newline-delimited
                // JSON output. Pi reads its initial prompt from stdin when
                // no positional prompt is provided, matching the pipe-based
                // delivery hydra already uses for Claude. Pi manages its
                // own tool permissions, so no skip-permissions flag.
                vec!["-p".to_string(), "--mode".to_string(), "json".to_string()]
            }
        }
    }

    /// Arguments to pass when spawning the harness for an interactive plan
    /// review (PTY, same shape as `pty_args` but without iteration
    /// instructions wrapping).
    ///
    /// Reserved for milestone 4 (plan review + parallel passthrough) which
    /// wires the interactive review path through the harness abstraction.
    #[allow(dead_code)]
    pub fn review_pty_args(self, prompt_path: &Path) -> Vec<String> {
        match self {
            Harness::Claude => vec![
                "--dangerously-skip-permissions".to_string(),
                format!("read instructions here: {}", prompt_path.display()),
            ],
            // Pi's interactive review is the same shape as its normal PTY
            // invocation: `pi @<prompt-file>` loads the review prompt as
            // the initial message and auto-submits it.
            Harness::Pi => vec![format!("@{}", prompt_path.display())],
        }
    }

    /// Arguments to pass when spawning the harness for a headless plan
    /// review (`claude -p` style, prompt delivered via stdin).
    pub fn review_headless_args(self) -> Vec<String> {
        match self {
            Harness::Claude => vec![
                "-p".to_string(),
                "--dangerously-skip-permissions".to_string(),
            ],
            Harness::Pi => vec![
                "-p".to_string(),
                "--dangerously-skip-permissions".to_string(),
            ],
        }
    }

    /// Environment variables that must be removed before spawning the
    /// harness. Returned as a list of names to `env_remove`.
    pub fn env_removals(self) -> &'static [&'static str] {
        match self {
            // Clear CLAUDECODE so nested claude sessions aren't short-circuited.
            Harness::Claude => &["CLAUDECODE"],
            // Pi doesn't need any env cleanup today.
            Harness::Pi => &[],
        }
    }
}

impl std::fmt::Display for Harness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Contents of `.hydra/harness.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessConfig {
    pub harness: String,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self {
            harness: "claude".to_string(),
        }
    }
}

impl HarnessConfig {
    /// Path to the local harness config file (`./.hydra/harness.json`).
    pub fn local_path() -> PathBuf {
        crate::config::Config::local_hydra_dir().join("harness.json")
    }

    /// Load the harness config from `./.hydra/harness.json`.
    /// Returns `Ok(None)` if the file doesn't exist (caller falls back to
    /// the built-in default).
    pub fn load() -> Result<Option<Self>> {
        Self::load_from_path(&Self::local_path())
    }

    /// Load the harness config from an explicit path. Returns `Ok(None)`
    /// when the path doesn't exist.
    pub fn load_from_path(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| HydraError::io(format!("reading harness config {}", path.display()), e))?;
        let config: HarnessConfig = serde_json::from_str(&content).map_err(|e| {
            HydraError::io(
                format!("parsing harness config {}", path.display()),
                std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
            )
        })?;
        Ok(Some(config))
    }

    /// Write this config to `./.hydra/harness.json`, creating the parent
    /// directory if needed.
    pub fn write_default(path: &Path) -> Result<()> {
        if let Some(parent) = path.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent)
                .map_err(|e| HydraError::io(format!("creating {}", parent.display()), e))?;
        }
        let content = serde_json::to_string_pretty(&HarnessConfig::default()).map_err(|e| {
            HydraError::io(
                "serializing harness config",
                std::io::Error::other(e.to_string()),
            )
        })?;
        std::fs::write(path, format!("{}\n", content))
            .map_err(|e| HydraError::io(format!("writing harness config {}", path.display()), e))?;
        Ok(())
    }

    /// Resolve the harness to use given an optional CLI override.
    ///
    /// Priority (highest to lowest):
    /// 1. CLI override (`--harness`)
    /// 2. `.hydra/harness.json`
    /// 3. Built-in default (`claude`)
    pub fn resolve(cli_override: Option<&str>) -> Result<Harness> {
        if let Some(name) = cli_override {
            return Harness::parse(name);
        }
        if let Some(cfg) = Self::load()? {
            return Harness::parse(&cfg.harness);
        }
        Ok(Harness::Claude)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_parse_valid_harness_names() {
        assert_eq!(Harness::parse("claude").unwrap(), Harness::Claude);
        assert_eq!(Harness::parse("pi").unwrap(), Harness::Pi);
        assert_eq!(Harness::parse("CLAUDE").unwrap(), Harness::Claude);
        assert_eq!(Harness::parse("  pi  ").unwrap(), Harness::Pi);
    }

    #[test]
    fn test_parse_invalid_harness_name() {
        assert!(Harness::parse("gpt").is_err());
        assert!(Harness::parse("").is_err());
    }

    #[test]
    fn test_harness_name_roundtrip() {
        for h in [Harness::Claude, Harness::Pi] {
            assert_eq!(Harness::parse(h.name()).unwrap(), h);
        }
    }

    #[test]
    fn test_claude_command_and_args() {
        let h = Harness::Claude;
        assert_eq!(h.command(), "claude");
        let args = h.pty_args(Path::new("/tmp/prompt.md"));
        assert_eq!(args[0], "--dangerously-skip-permissions");
        assert!(args[1].contains("/tmp/prompt.md"));
    }

    #[test]
    fn test_pi_command_and_pty_args() {
        let h = Harness::Pi;
        assert_eq!(h.command(), "pi");
        let args = h.pty_args(Path::new("/tmp/prompt.md"));
        // Pi takes a single @<file> argument; the file contents become
        // the auto-submitted initial message.
        assert_eq!(args, vec!["@/tmp/prompt.md".to_string()]);
        // Pi manages its own permissions — no skip-permissions flag.
        assert!(!args.iter().any(|a| a.contains("skip-permissions")));
    }

    #[test]
    fn test_pi_review_pty_args_use_at_file() {
        let args = Harness::Pi.review_pty_args(Path::new("/tmp/review.md"));
        assert_eq!(args, vec!["@/tmp/review.md".to_string()]);
    }

    #[test]
    fn test_claude_headless_args_include_stream_json() {
        let args = Harness::Claude.headless_args();
        assert!(args.iter().any(|a| a == "-p"));
        assert!(args.iter().any(|a| a == "stream-json"));
        assert!(args.iter().any(|a| a == "--dangerously-skip-permissions"));
    }

    #[test]
    fn test_pi_headless_args_use_mode_json() {
        let args = Harness::Pi.headless_args();
        assert!(args.iter().any(|a| a == "-p"));
        assert!(args.iter().any(|a| a == "--mode"));
        assert!(args.iter().any(|a| a == "json"));
        // Pi manages its own permissions and has no stream-json Claude flag.
        assert!(!args.iter().any(|a| a.contains("skip-permissions")));
        assert!(!args.iter().any(|a| a == "stream-json"));
    }

    #[test]
    fn test_claude_env_removal_contains_claudecode() {
        assert!(Harness::Claude.env_removals().contains(&"CLAUDECODE"));
    }

    #[test]
    fn test_pi_env_removal_is_empty() {
        assert!(Harness::Pi.env_removals().is_empty());
    }

    #[test]
    fn test_harness_config_load_missing_returns_none() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("harness.json");
        assert!(HarnessConfig::load_from_path(&path).unwrap().is_none());
    }

    #[test]
    fn test_harness_config_write_and_load() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("harness.json");
        HarnessConfig::write_default(&path).unwrap();
        let loaded = HarnessConfig::load_from_path(&path).unwrap().unwrap();
        assert_eq!(loaded.harness, "claude");
    }

    #[test]
    fn test_harness_config_load_invalid_json_errors() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("harness.json");
        std::fs::write(&path, "not json {{{").unwrap();
        assert!(HarnessConfig::load_from_path(&path).is_err());
    }

    #[test]
    fn test_resolve_cli_override_wins() {
        let h = HarnessConfig::resolve(Some("pi")).unwrap();
        assert_eq!(h, Harness::Pi);
    }

    #[test]
    fn test_resolve_cli_invalid_errors() {
        assert!(HarnessConfig::resolve(Some("gpt")).is_err());
    }
}
