use crate::config::Config;
use crate::error::{HydraError, Result};
use std::fs;
use std::path::PathBuf;

/// Result of prompt resolution
#[derive(Debug)]
pub struct ResolvedPrompt {
    /// The path where the prompt was found
    pub path: PathBuf,
    /// The content of the prompt file
    pub content: String,
    /// Which priority level matched
    pub source: PromptSource,
}

/// Source of the resolved prompt
#[derive(Debug, Clone, PartialEq)]
pub enum PromptSource {
    /// CLI --prompt flag (highest priority)
    CliOverride,
    /// ./.hydra/prompt.md (project-specific)
    ProjectHydra,
    /// ./prompt.md (current directory)
    CurrentDir,
    /// ~/.hydra/default-prompt.md (global fallback)
    GlobalDefault,
}

impl std::fmt::Display for PromptSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PromptSource::CliOverride => write!(f, "CLI --prompt flag"),
            PromptSource::ProjectHydra => write!(f, "./.hydra/prompt.md"),
            PromptSource::CurrentDir => write!(f, "./prompt.md"),
            PromptSource::GlobalDefault => write!(f, "~/.hydra/default-prompt.md"),
        }
    }
}

/// Resolve prompt file according to priority chain:
/// 1. CLI --prompt flag (highest)
/// 2. ./.hydra/prompt.md (project-specific)
/// 3. ./prompt.md (current directory)
/// 4. ~/.hydra/default-prompt.md (global fallback, lowest)
///
/// If no prompt is found at any level, returns NoPromptFound error.
pub fn resolve_prompt(cli_prompt: Option<&PathBuf>) -> Result<ResolvedPrompt> {
    // Priority 1: CLI --prompt flag
    if let Some(path) = cli_prompt {
        if path.exists() {
            let content = read_prompt_file(path)?;
            return Ok(ResolvedPrompt {
                path: path.clone(),
                content,
                source: PromptSource::CliOverride,
            });
        } else {
            return Err(HydraError::PromptNotFound(path.clone()));
        }
    }

    // Priority 2: ./.hydra/prompt.md (project-specific)
    let project_prompt = Config::local_prompt_path();
    if project_prompt.exists() {
        let content = read_prompt_file(&project_prompt)?;
        return Ok(ResolvedPrompt {
            path: project_prompt,
            content,
            source: PromptSource::ProjectHydra,
        });
    }

    // Priority 3: ./prompt.md (current directory)
    let current_dir_prompt = PathBuf::from("prompt.md");
    if current_dir_prompt.exists() {
        let content = read_prompt_file(&current_dir_prompt)?;
        return Ok(ResolvedPrompt {
            path: current_dir_prompt,
            content,
            source: PromptSource::CurrentDir,
        });
    }

    // Priority 4: ~/.hydra/default-prompt.md (global fallback)
    let global_prompt = Config::global_default_prompt_path();
    if global_prompt.exists() {
        let content = read_prompt_file(&global_prompt)?;
        return Ok(ResolvedPrompt {
            path: global_prompt,
            content,
            source: PromptSource::GlobalDefault,
        });
    }

    // No prompt found at any level
    Err(HydraError::NoPromptFound)
}

/// Read a prompt file and return its content
fn read_prompt_file(path: &PathBuf) -> Result<String> {
    fs::read_to_string(path)
        .map_err(|e| HydraError::io(format!("reading prompt file {}", path.display()), e))
}

/// Inject plan path reference into prompt content
///
/// Appends a reference to the plan file path to the prompt.
/// Returns the combined content.
pub fn inject_plan_path(prompt_content: &str, plan_path: &std::path::Path) -> String {
    format!(
        "{}\n\n## Implementation Plan\n\nThe implementation plan is located at: {}",
        prompt_content.trim_end(),
        plan_path.display()
    )
}

/// Inject scratchpad path reference into prompt content
///
/// Appends a `## Scratchpad` section with the file path to the prompt.
/// Returns the combined content.
pub fn inject_scratchpad_path(prompt_content: &str, scratchpad_path: &std::path::Path) -> String {
    format!(
        "{}\n\n## Scratchpad\n\nShared notes file across iterations: {}",
        prompt_content.trim_end(),
        scratchpad_path.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::Mutex;
    use tempfile::TempDir;

    // Mutex to serialize tests that change current directory
    static DIR_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_prompt_source_display() {
        assert_eq!(PromptSource::CliOverride.to_string(), "CLI --prompt flag");
        assert_eq!(PromptSource::ProjectHydra.to_string(), "./.hydra/prompt.md");
        assert_eq!(PromptSource::CurrentDir.to_string(), "./prompt.md");
        assert_eq!(
            PromptSource::GlobalDefault.to_string(),
            "~/.hydra/default-prompt.md"
        );
    }

    #[test]
    fn test_cli_prompt_takes_priority() {
        let temp_dir = TempDir::new().unwrap();
        let cli_prompt = temp_dir.path().join("cli-prompt.md");
        fs::write(&cli_prompt, "CLI prompt content").unwrap();

        let result = resolve_prompt(Some(&cli_prompt)).unwrap();
        assert_eq!(result.source, PromptSource::CliOverride);
        assert_eq!(result.content, "CLI prompt content");
        assert_eq!(result.path, cli_prompt);
    }

    #[test]
    fn test_cli_prompt_not_found_error() {
        let nonexistent = PathBuf::from("/nonexistent/path/prompt.md");
        let result = resolve_prompt(Some(&nonexistent));
        assert!(result.is_err());
        match result.unwrap_err() {
            HydraError::PromptNotFound(path) => {
                assert_eq!(path, nonexistent);
            }
            _ => panic!("Expected PromptNotFound error"),
        }
    }

    #[test]
    fn test_project_hydra_prompt() {
        let _lock = DIR_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let original_dir = env::current_dir().unwrap();
        env::set_current_dir(temp_dir.path()).unwrap();

        // Create .hydra/prompt.md
        let hydra_dir = temp_dir.path().join(".hydra");
        fs::create_dir_all(&hydra_dir).unwrap();
        fs::write(hydra_dir.join("prompt.md"), "Project hydra prompt").unwrap();

        let result = resolve_prompt(None).unwrap();
        assert_eq!(result.source, PromptSource::ProjectHydra);
        assert_eq!(result.content, "Project hydra prompt");

        env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_current_dir_prompt() {
        let _lock = DIR_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let original_dir = env::current_dir().unwrap();
        env::set_current_dir(temp_dir.path()).unwrap();

        // Create prompt.md in current directory
        fs::write(temp_dir.path().join("prompt.md"), "Current dir prompt").unwrap();

        let result = resolve_prompt(None).unwrap();
        assert_eq!(result.source, PromptSource::CurrentDir);
        assert_eq!(result.content, "Current dir prompt");

        env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_priority_order() {
        let _lock = DIR_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let original_dir = env::current_dir().unwrap();
        env::set_current_dir(temp_dir.path()).unwrap();

        // Create all prompt files
        let hydra_dir = temp_dir.path().join(".hydra");
        fs::create_dir_all(&hydra_dir).unwrap();
        fs::write(hydra_dir.join("prompt.md"), "Project hydra prompt").unwrap();
        fs::write(temp_dir.path().join("prompt.md"), "Current dir prompt").unwrap();

        // Without CLI override, project hydra should take priority
        let result = resolve_prompt(None).unwrap();
        assert_eq!(result.source, PromptSource::ProjectHydra);
        assert_eq!(result.content, "Project hydra prompt");

        // With CLI override, CLI should take priority
        let cli_prompt = temp_dir.path().join("cli-prompt.md");
        fs::write(&cli_prompt, "CLI prompt").unwrap();
        let result = resolve_prompt(Some(&cli_prompt)).unwrap();
        assert_eq!(result.source, PromptSource::CliOverride);
        assert_eq!(result.content, "CLI prompt");

        env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_no_prompt_found() {
        let _lock = DIR_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let original_dir = env::current_dir().unwrap();
        env::set_current_dir(temp_dir.path()).unwrap();

        // Don't create any prompt files, but we need to handle
        // the global default which may exist in the real home dir
        // This test just verifies the error path exists
        let result = resolve_prompt(None);
        // Result depends on whether ~/.hydra/default-prompt.md exists

        env::set_current_dir(original_dir).unwrap();

        // If global default exists, we get Ok; if not, we get NoPromptFound
        // Both are valid states depending on the test environment
        match result {
            Ok(resolved) => {
                assert_eq!(resolved.source, PromptSource::GlobalDefault);
            }
            Err(HydraError::NoPromptFound) => {
                // Expected if no global default exists
            }
            Err(e) => panic!("Unexpected error: {}", e),
        }
    }

    #[test]
    fn test_inject_plan_path_basic() {
        use std::path::Path;
        let prompt = "# My Prompt\n\nDo the thing.";
        let plan_path = Path::new("/path/to/plan.md");

        let result = inject_plan_path(prompt, plan_path);

        assert!(result.starts_with("# My Prompt"));
        assert!(result.contains("## Implementation Plan"));
        assert!(result.contains("/path/to/plan.md"));
    }

    #[test]
    fn test_inject_plan_path_format() {
        use std::path::Path;
        let prompt = "Prompt content";
        let plan_path = Path::new("plan.md");

        let result = inject_plan_path(prompt, plan_path);

        // Verify the exact format
        assert_eq!(
            result,
            "Prompt content\n\n## Implementation Plan\n\nThe implementation plan is located at: plan.md"
        );
    }

    #[test]
    fn test_inject_scratchpad_path() {
        use std::path::Path;
        let prompt = "# Prompt\n\n## Implementation Plan\n\nThe implementation plan is located at: plan.md";
        let scratchpad = Path::new(".hydra/scratchpad/my-plan.md");

        let result = inject_scratchpad_path(prompt, scratchpad);

        assert!(result.contains("## Scratchpad"));
        assert!(result.contains(".hydra/scratchpad/my-plan.md"));
        // Scratchpad section comes after plan section
        let plan_pos = result.find("## Implementation Plan").unwrap();
        let scratch_pos = result.find("## Scratchpad").unwrap();
        assert!(scratch_pos > plan_pos);
    }

    #[test]
    fn test_inject_scratchpad_path_format() {
        use std::path::Path;
        let prompt = "Prompt content";
        let scratchpad = Path::new(".hydra/scratchpad/plan.md");

        let result = inject_scratchpad_path(prompt, scratchpad);

        assert_eq!(
            result,
            "Prompt content\n\n## Scratchpad\n\nShared notes file across iterations: .hydra/scratchpad/plan.md"
        );
    }

    #[test]
    fn test_inject_plan_path_trims_trailing_whitespace() {
        use std::path::Path;
        let prompt = "Prompt content\n\n\n";
        let plan_path = Path::new("plan.md");

        let result = inject_plan_path(prompt, plan_path);

        // Should trim trailing whitespace from prompt before adding header
        assert_eq!(
            result,
            "Prompt content\n\n## Implementation Plan\n\nThe implementation plan is located at: plan.md"
        );
    }
}
