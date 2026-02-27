mod cli;
mod config;
mod error;
mod prompt;
mod pty;
mod runner;
mod signal;
mod skill;
mod tui;

use clap::Parser;
use cli::Cli;
use config::Config;
use error::{EXIT_SUCCESS, HydraError, Result};
use prompt::{inject_plan_path, inject_scratchpad_path, resolve_prompt};
use runner::{RunResult, Runner};
use skill::{SkillType, create_skill_with_claude, prompt_yes_no, spawn_claude_interactive};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, Write};
use std::path::PathBuf;

/// Debug log to file (since terminal may be frozen)
fn debug_log(msg: &str) {
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/hydra-debug.log")
    {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let _ = writeln!(f, "[{}] main: {}", timestamp, msg);
    }
}

fn main() {
    debug_log("main started");
    let cli = Cli::parse();
    let verbose = cli.verbose;

    if verbose {
        eprintln!("CLI parsed: {:?}", cli);
    }

    let result = run(cli);
    debug_log(&format!("run() returned: {:?}", result.is_ok()));

    match result {
        Ok(()) => {
            debug_log("exiting with success");
            std::process::exit(EXIT_SUCCESS)
        }
        Err(e) => {
            let exit_code = e.exit_code();
            debug_log(&format!("exiting with error code {}", exit_code));
            if verbose || exit_code != error::EXIT_STOPPED {
                eprintln!("hydra: {}", e);
            }
            std::process::exit(exit_code);
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    // Load config from ~/.hydra/config.toml (with defaults if not present)
    let mut config = Config::load()?;

    // Merge CLI options over config (CLI takes precedence)
    config.merge_cli(
        if cli.max != 20 { Some(cli.max) } else { None },
        cli.verbose,
        if cli.timeout != 1200 {
            Some(cli.timeout)
        } else {
            None
        },
    );

    if config.verbose {
        eprintln!("Config loaded: {:?}", config);
    }

    // Route to appropriate command handler
    if cli.is_install() {
        install_command()
    } else if cli.is_init() {
        init_command(config.verbose)
    } else if cli.is_tui() {
        // TUI mode
        let mut resolved = resolve_prompt(cli.prompt.as_ref())?;

        // If a plan file is provided in tui subcommand, inject it
        if let Some(plan_path) = cli.tui_plan() {
            if !plan_path.exists() {
                return Err(HydraError::PlanNotFound(plan_path.clone()));
            }
            resolved.content = inject_plan_path(&resolved.content, plan_path);
        }

        tui::run_tui(config, resolved)
    } else {
        // Resolve prompt file according to priority chain
        let mut resolved = resolve_prompt(cli.prompt.as_ref())?;

        if config.verbose {
            eprintln!("Prompt resolved from: {}", resolved.source);
            eprintln!("Prompt path: {}", resolved.path.display());
        }

        // If a plan file is provided, inject a reference to it in the prompt
        if let Some(ref plan_path) = cli.plan {
            // Verify plan file exists
            if !plan_path.exists() {
                return Err(HydraError::PlanNotFound(plan_path.clone()));
            }

            if config.verbose {
                eprintln!("Plan file: {}", plan_path.display());
            }

            // Inject plan path reference into prompt
            resolved.content = inject_plan_path(&resolved.content, plan_path);

            // Create scratchpad file for cross-iteration notes
            let scratchpad_dir = Config::scratchpad_dir();
            if let Err(e) = fs::create_dir_all(&scratchpad_dir) {
                eprintln!(
                    "[hydra] Warning: Could not create scratchpad directory: {}",
                    e
                );
            } else {
                let plan_stem = plan_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("scratchpad");
                let scratchpad_path = scratchpad_dir.join(format!("{}.md", plan_stem));
                if !scratchpad_path.exists() {
                    let header = format!("# Scratchpad — {}\n\nCross-iteration notes for this plan.\n", plan_stem);
                    if let Err(e) = fs::write(&scratchpad_path, header) {
                        eprintln!(
                            "[hydra] Warning: Could not create scratchpad file: {}",
                            e
                        );
                    }
                }
                // Inject scratchpad path into prompt
                resolved.content = inject_scratchpad_path(&resolved.content, &scratchpad_path);
            }
        }

        if cli.dry_run {
            // Dry run: show configuration without executing
            println!("Configuration (dry-run):");
            println!("  max_iterations: {}", config.max_iterations);
            println!(
                "  timeout_seconds: {} ({} minutes)",
                config.timeout_seconds,
                config.timeout_seconds / 60
            );
            println!("  verbose: {}", config.verbose);
            println!("  stop_file: {}", config.stop_file);
            println!("  prompt_source: {}", resolved.source);
            println!("  prompt_path: {}", resolved.path.display());
            if let Some(ref plan_path) = cli.plan {
                println!("  plan_path: {}", plan_path.display());
            }
            println!("\nPrompt content ({} bytes):", resolved.content.len());
            println!("---");
            // Show first 500 chars of prompt for preview
            if resolved.content.len() > 500 {
                println!("{}...", &resolved.content[..500]);
            } else {
                println!("{}", resolved.content);
            }
            println!("---");
            Ok(())
        } else {
            // Print banner and version
            println!("{}", BANNER);
            println!(
                "                                  hydra v{}",
                env!("CARGO_PKG_VERSION")
            );
            println!();

            // Print the prompt content so user knows what they're sending
            println!("─── prompt ({}) ───", resolved.source);
            println!();
            for line in resolved.content.lines() {
                println!("  {}", line);
            }
            println!();
            println!("─────────────────────────────────────────");
            println!();

            // Extract plan name from plan path (file stem without extension)
            let plan_name = cli.plan.as_ref().and_then(|p| {
                p.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            });

            // Create the runner
            let mut runner = Runner::new(config.clone(), resolved, plan_name);

            // Install signal handlers with the runner's stop flag
            let stop_flag = runner.stop_flag();
            if let Err(e) = signal::install_handlers(stop_flag) {
                eprintln!("[hydra] Warning: Failed to install signal handlers: {}", e);
            }

            // Run the main loop
            let result = runner.run()?;

            // Launch plan review if all tasks completed and a plan was provided
            if matches!(result, RunResult::AllTasksComplete { .. }) {
                if let Some(ref plan_path) = cli.plan {
                    println!();
                    println!("[hydra] Launching plan review...");
                    println!();

                    // Build the review prompt — scratchpad is read by the skill itself
                    let review_prompt = format!("/plan-review {}", plan_path.display());
                    let temp_dir = std::env::temp_dir();
                    let review_file = temp_dir.join("hydra-plan-review.md");
                    if let Err(e) = fs::write(&review_file, &review_prompt) {
                        eprintln!("[hydra] Warning: Could not create review prompt file: {}", e);
                    } else {
                        if let Err(e) = spawn_claude_interactive(&review_file, config.verbose) {
                            eprintln!("[hydra] Warning: Plan review failed: {}", e);
                        }
                        let _ = fs::remove_file(&review_file);
                    }
                }
            }

            // Convert run result to appropriate exit
            match result {
                RunResult::AllTasksComplete { .. } => Ok(()),
                RunResult::MaxIterations { .. } => Ok(()),
                RunResult::Timeout { .. } => Ok(()), // Timeout is success - we just move to next iteration
                RunResult::Stopped { .. } => Err(HydraError::GracefulStop),
                RunResult::Interrupted => Err(HydraError::Interrupted),
            }
        }
    }
}

/// ASCII art banner displayed on startup
const BANNER: &str = r#"
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⡀⠀⠀⠀⠀⢠⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠻⣦⡀⠀⢸⣆⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⣠⣦⣤⣀⣀⣤⣤⣀⡀⠀⣀⣠⡆⠀⠀⠀⠀⠀⠀⠤⠒⠛⣛⣛⣻⣿⣶⣾⣿⣦⣄⢿⣆⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠸⠿⢿⣿⣿⣿⣯⣭⣿⣿⣿⣿⣋⣀⠀⠀⠀⠀⠀⠀⣠⣶⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷⣤⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠙⢿⣿⣿⡿⢿⣿⣿⣿⣿⣿⣓⠢⠄⢠⡾⢻⣿⣿⣿⣿⡟⠁⠀⠀⠈⠙⢿⣿⣿⣯⡻⣿⡄⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠉⠉⠀⠀⠀⠙⢿⣿⣿⣿⣷⣄⠁⠀⣿⣿⣿⣿⣿⡇⠀⠀⠀⠀⠀⢸⣿⣿⣿⣿⣿⣷⣄⡀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⣿⣿⣿⣷⣌⢧⠀⣿⣿⣿⣿⣿⣿⣄⠀⠀⠀⠀⢀⠉⠙⠛⠛⠿⣿⣿⣿⡆⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣿⣿⣿⣿⣿⡀⠠⢻⡟⢿⣿⣿⣿⣿⣧⣄⣀⠀⠘⢶⣄⣀⠀⠀⠈⢻⠿⠁⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣸⣿⣿⣿⣿⣾⠀⠀⠀⠻⣈⣙⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡿⣷⣦⡀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠈⠲⣄⠀⠀⣀⡤⠤⠀⠀⠀⢠⣿⣿⣿⡿⣿⠇⠀⠀⠐⠺⢉⣡⣴⣿⣿⣿⣿⣿⣿⣿⡿⢿⣿⣿⣿⣶⣿⣿⣿⣶⣶⡀⠀⠀⠀
⠀⠀⠀⠀⢠⣿⣴⣿⣷⣶⣦⣤⡀⠀⢸⣿⣿⣿⠇⠏⠀⠀⠀⢀⣴⣿⣿⣿⣿⣿⠟⢿⣿⣿⣿⣷⠀⠹⣿⣿⠿⠿⠛⠻⠿⣿⠇⠀⠀⠀
⠀⠀⠀⣠⣿⣿⣿⣿⣿⣿⣿⣷⣯⡂⢸⣿⣿⣿⠀⠀⠀⠀⢀⠾⣻⣿⣿⣿⠟⠀⠀⠈⣿⣿⣿⣿⡇⠀⠀⣀⣀⡀⠀⢠⡞⠉⠀⠀⠀⠀
⠀⠀⢸⣟⣽⣿⣯⠀⠀⢹⣿⣿⣿⡟⠼⣿⣿⣿⣇⠀⠀⠀⠠⢰⣿⣿⣿⣿⡄⠀⠀⠀⣸⣿⣿⣿⡇⠀⢀⣤⣼⣿⣷⣾⣷⡀⠀⠀⠀⠀
⠀⢀⣾⣿⡿⠟⠋⠀⠀⢸⣿⣿⣿⣿⡀⢿⣿⣿⣿⣦⠀⠀⠀⢺⣿⣿⣿⣿⣿⣄⠀⠀⣿⣿⣿⣿⡇⠐⣿⣿⣿⣿⠿⣿⣿⡿⣦⠀⠀⠀
⠀⢻⣿⠏⠀⠀⠀⠀⢠⣿⣿⣿⡟⡿⠀⠀⢻⣿⣿⣿⣷⣤⡀⠘⣷⠻⣿⣿⣿⣿⣷⣼⣿⣿⣿⣿⣇⣾⣿⣿⣿⠁⠀⢼⣿⣿⣿⣆⠀⠀
⠀⠀⠈⠀⠀⠀⠀⠀⢸⣿⣿⣿⡗⠁⠀⠀⠀⠙⢿⣿⣿⣿⣿⣷⣾⣆⡙⣿⣿⣿⣿⣿⣿⣿⣿⣿⠌⣾⣿⣿⣿⣆⠀⠀⠀⠉⠻⣿⡷⠀
⠀⠀⠀⠀⠀⠀⠀⠀⢸⣿⣿⣿⣷⣄⠀⠀⠀⠀⠀⠈⠻⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡏⠀⠘⣟⣿⣿⣿⡆⠀⠀⠀⠀⠙⠁⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠻⣿⣿⣿⣿⣿⣶⣤⣤⣤⣀⣠⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡿⠀⠀⠀⢈⣿⣿⣿⡇⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠙⠿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣟⣠⣤⣤⣶⣿⣿⣿⠟⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⢀⣠⣤⣄⠀⠠⢶⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣟⡁⠀⠀⠀⠀⠀⠀⠀⠀⠀
⢀⣀⠀⣠⣀⡠⠞⣿⣿⣿⣿⣶⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣴⣿⣷⣦⣄⣀⢿⡽⢻⣦
⠻⠶⠾⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠋
"#;

/// Default template content for project-specific prompt.md
/// Used as fallback when ~/.hydra/prompt-template.md doesn't exist
const DEFAULT_PROJECT_PROMPT_TEMPLATE: &str = include_str!("../templates/default-prompt.md");

/// Load the project prompt template from ~/.hydra/prompt-template.md
/// Falls back to DEFAULT_PROJECT_PROMPT_TEMPLATE if the file doesn't exist
fn load_prompt_template() -> String {
    let template_path = Config::global_prompt_template_path();
    if template_path.exists() {
        fs::read_to_string(&template_path)
            .unwrap_or_else(|_| DEFAULT_PROJECT_PROMPT_TEMPLATE.to_string())
    } else {
        DEFAULT_PROJECT_PROMPT_TEMPLATE.to_string()
    }
}

/// Initialize project with optional skill setup and .hydra/ directory creation
fn init_command(verbose: bool) -> Result<()> {
    // Prompt for skill setup first
    setup_skills(verbose)?;

    // Prompt for .hydra/ directory creation at the end
    println!();
    if prompt_yes_no("Create .hydra/ directory with prompt template?")? {
        create_hydra_directory(verbose)?;
    }

    Ok(())
}

/// Create .hydra/ directory structure and prompt template
fn create_hydra_directory(verbose: bool) -> Result<()> {
    let hydra_dir = Config::local_hydra_dir();
    let logs_dir = Config::logs_dir();
    let prompt_path = Config::local_prompt_path();

    // Ensure global ~/.hydra/ directory and template exist
    ensure_global_template(verbose)?;

    // Check if .hydra already exists
    if hydra_dir.exists() {
        println!(".hydra/ directory already exists");
    } else {
        fs::create_dir_all(&hydra_dir)
            .map_err(|e| HydraError::io(format!("creating {}", hydra_dir.display()), e))?;
        println!("Created {}", hydra_dir.display());
    }

    // Create logs/ subdirectory
    if !logs_dir.exists() {
        fs::create_dir_all(&logs_dir)
            .map_err(|e| HydraError::io(format!("creating {}", logs_dir.display()), e))?;
        if verbose {
            println!("Created {}", logs_dir.display());
        }
    }

    // Create prompt.md from template if it doesn't exist
    if !prompt_path.exists() {
        let template_content = load_prompt_template();
        fs::write(&prompt_path, template_content)
            .map_err(|e| HydraError::io(format!("writing {}", prompt_path.display()), e))?;
        println!("Created {} (from template)", prompt_path.display());
    } else if verbose {
        println!("{} already exists", prompt_path.display());
    }

    // Update .gitignore
    update_gitignore(verbose)?;

    println!("\nEdit .hydra/prompt.md with your task instructions.");
    println!(
        "Customize the template at: {}",
        Config::global_prompt_template_path().display()
    );

    Ok(())
}

/// Prompt for and optionally set up Claude Code permissions and skills
fn setup_skills(verbose: bool) -> Result<()> {
    // Prompt for permissions setup first (per spec: permissions → local-dev-guide → deploy-and-check → precommit)
    if prompt_yes_no(SkillType::Permissions.prompt_text())? {
        create_skill_with_claude(SkillType::Permissions, verbose)?;
    }

    // Prompt for local-dev-guide skill
    if prompt_yes_no(SkillType::LocalDevGuide.prompt_text())? {
        create_skill_with_claude(SkillType::LocalDevGuide, verbose)?;
    }

    // Prompt for deploy-and-check skill
    if prompt_yes_no(SkillType::DeployAndCheck.prompt_text())? {
        create_skill_with_claude(SkillType::DeployAndCheck, verbose)?;
    }

    // Prompt for precommit hooks setup
    if prompt_yes_no(SkillType::Precommit.prompt_text())? {
        create_skill_with_claude(SkillType::Precommit, verbose)?;
    }

    // Prompt for CLAUDE.md instructions (browser automation, specs, etc.)
    if prompt_yes_no("Add CLAUDE.md instructions (browser automation, specs)?")? {
        add_claude_md_instructions(verbose)?;
    }

    Ok(())
}

/// Sections to append to CLAUDE.md
const CLAUDE_MD_SECTIONS: &[(&str, &str)] = &[
    (
        "## Browser Automation",
        "\n## Browser Automation\n\n\
         Use `agent-browser` for web automation. Run `agent-browser --help` for all commands.\n\n\
         Core workflow:\n\
         1. `agent-browser open <url>` - Navigate to page\n\
         2. `agent-browser snapshot -i` - Get interactive elements with refs (@e1, @e2)\n\
         3. `agent-browser click @e1` / `fill @e2 \"text\"` - Interact using refs\n\
         4. Re-snapshot after page changes\n",
    ),
    (
        "## Specs",
        "\n## Specs\n\n\
         You can use `/spec study` to review existing systems before implementing.\n",
    ),
];

/// Append standard instructions to CLAUDE.md (browser automation, specs, etc.)
fn add_claude_md_instructions(verbose: bool) -> Result<()> {
    let claude_md_path = PathBuf::from("CLAUDE.md");

    let existing_content = if claude_md_path.exists() {
        fs::read_to_string(&claude_md_path).map_err(|e| HydraError::io("reading CLAUDE.md", e))?
    } else {
        String::new()
    };

    // Collect sections that don't already exist
    let mut to_append = String::new();
    for (heading, section) in CLAUDE_MD_SECTIONS {
        if !existing_content.contains(heading) {
            to_append.push_str(section);
        } else if verbose {
            println!("{} section already in CLAUDE.md", heading);
        }
    }

    if to_append.is_empty() {
        if verbose {
            println!("All sections already present in CLAUDE.md");
        }
        return Ok(());
    }

    if claude_md_path.exists() {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&claude_md_path)
            .map_err(|e| HydraError::io("opening CLAUDE.md for append", e))?;

        if !existing_content.ends_with('\n') {
            writeln!(file).map_err(|e| HydraError::io("writing to CLAUDE.md", e))?;
        }

        write!(file, "{}", to_append).map_err(|e| HydraError::io("writing to CLAUDE.md", e))?;
    } else {
        fs::write(&claude_md_path, format!("# Project\n{}", to_append))
            .map_err(|e| HydraError::io("creating CLAUDE.md", e))?;
    }

    println!("Added instructions to CLAUDE.md");
    Ok(())
}

/// Ensure the global hydra directory and prompt template exist
fn ensure_global_template(verbose: bool) -> Result<()> {
    let hydra_dir = Config::global_hydra_dir();
    if !hydra_dir.exists() {
        fs::create_dir_all(&hydra_dir)
            .map_err(|e| HydraError::io(format!("creating {}", hydra_dir.display()), e))?;
        if verbose {
            println!("Created {}", hydra_dir.display());
        }
    }

    let template_path = Config::global_prompt_template_path();
    if !template_path.exists() {
        fs::write(&template_path, DEFAULT_PROJECT_PROMPT_TEMPLATE)
            .map_err(|e| HydraError::io(format!("writing {}", template_path.display()), e))?;
        if verbose {
            println!("Created {}", template_path.display());
        }
    }

    Ok(())
}

/// Update .gitignore to include .hydra/ if not already present
fn update_gitignore(verbose: bool) -> Result<()> {
    let gitignore_path = PathBuf::from(".gitignore");
    let hydra_entry = ".hydra/";

    // Check if .gitignore exists and if it already contains .hydra/
    if gitignore_path.exists() {
        let file =
            fs::File::open(&gitignore_path).map_err(|e| HydraError::io("reading .gitignore", e))?;
        let reader = std::io::BufReader::new(file);

        // Check if .hydra/ is already in .gitignore
        for line in reader.lines() {
            let line = line.map_err(|e| HydraError::io("reading .gitignore line", e))?;
            let trimmed = line.trim();
            if trimmed == hydra_entry || trimmed == ".hydra" {
                if verbose {
                    println!(".hydra/ already in .gitignore");
                }
                return Ok(());
            }
        }

        // Append .hydra/ to existing .gitignore
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&gitignore_path)
            .map_err(|e| HydraError::io("opening .gitignore for append", e))?;

        // Check if file ends with newline
        let content = fs::read_to_string(&gitignore_path)
            .map_err(|e| HydraError::io("reading .gitignore", e))?;
        let needs_newline = !content.is_empty() && !content.ends_with('\n');

        if needs_newline {
            writeln!(file).map_err(|e| HydraError::io("writing to .gitignore", e))?;
        }
        writeln!(file, "{}", hydra_entry)
            .map_err(|e| HydraError::io("writing to .gitignore", e))?;
        println!("Added {} to .gitignore", hydra_entry);
    } else {
        // Create new .gitignore with .hydra/
        fs::write(&gitignore_path, format!("{}\n", hydra_entry))
            .map_err(|e| HydraError::io("creating .gitignore", e))?;
        println!("Created .gitignore with {}", hydra_entry);
    }

    Ok(())
}

/// Install hydra to ~/.local/bin
fn install_command() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    // Get the path to the currently running executable
    let current_exe = std::env::current_exe()
        .map_err(|e| HydraError::io("getting current executable path", e))?;

    // Get ~/.local/bin directory
    let home_dir = dirs::home_dir().ok_or_else(|| {
        HydraError::io(
            "getting home directory",
            std::io::Error::new(std::io::ErrorKind::NotFound, "HOME directory not found"),
        )
    })?;
    let local_bin = home_dir.join(".local").join("bin");

    // Create ~/.local/bin if it doesn't exist
    if !local_bin.exists() {
        fs::create_dir_all(&local_bin)
            .map_err(|e| HydraError::io(format!("creating {}", local_bin.display()), e))?;
        println!("Created {}", local_bin.display());
    }

    // Destination path
    let dest_path = local_bin.join("hydra");

    // Check if source and dest are the same file (already installed and running from there)
    if current_exe == dest_path {
        println!("hydra is already installed at {}", dest_path.display());
        return Ok(());
    }

    // Copy the binary
    fs::copy(&current_exe, &dest_path)
        .map_err(|e| HydraError::io(format!("copying to {}", dest_path.display()), e))?;

    // Set executable permissions (rwxr-xr-x = 0o755)
    let mut perms = fs::metadata(&dest_path)
        .map_err(|e| HydraError::io(format!("reading metadata of {}", dest_path.display()), e))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&dest_path, perms).map_err(|e| {
        HydraError::io(format!("setting permissions on {}", dest_path.display()), e)
    })?;

    // On macOS, re-sign the binary with ad-hoc signature to satisfy Gatekeeper
    // Copying invalidates the original code signature, causing "killed" on execution
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("codesign")
            .args(["--force", "--sign", "-", dest_path.to_str().unwrap()])
            .status()
            .map_err(|e| HydraError::io("running codesign", e))?;

        if !status.success() {
            eprintln!(
                "Warning: codesign failed. The binary may not run due to Gatekeeper restrictions."
            );
        }
    }

    println!("Installed hydra to {}", dest_path.display());

    // Check if ~/.local/bin is in PATH
    if let Ok(path_var) = std::env::var("PATH") {
        let local_bin_str = local_bin.to_string_lossy();
        if !path_var.split(':').any(|p| p == local_bin_str) {
            println!("\nNote: {} is not in your PATH.", local_bin.display());
            println!("Add this to your shell config (~/.bashrc, ~/.zshrc, etc.):");
            println!("  export PATH=\"$HOME/.local/bin:$PATH\"");
        }
    }

    Ok(())
}
