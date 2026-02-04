mod cli;
mod config;
mod error;
mod prompt;
mod pty;
mod runner;
mod signal;
mod skill;

use clap::Parser;
use cli::Cli;
use config::Config;
use error::{EXIT_SUCCESS, HydraError, Result};
use prompt::{inject_plan_path, resolve_prompt};
use runner::{RunResult, Runner};
use skill::{SkillType, create_skill_with_claude, prompt_yes_no};
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
        if cli.max != 10 { Some(cli.max) } else { None },
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

            // Create the runner
            let mut runner = Runner::new(config.clone(), resolved);

            // Install signal handlers with the runner's stop flag
            let stop_flag = runner.stop_flag();
            if let Err(e) = signal::install_handlers(stop_flag) {
                eprintln!("[hydra] Warning: Failed to install signal handlers: {}", e);
            }

            // Run the main loop
            let result = runner.run()?;

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

/// Initialize .hydra/ directory structure in current project
fn init_command(verbose: bool) -> Result<()> {
    let hydra_dir = Config::local_hydra_dir();
    let logs_dir = Config::logs_dir();
    let prompt_path = Config::local_prompt_path();

    // Ensure global ~/.hydra/ directory and template exist
    ensure_global_template(verbose)?;

    // Check if .hydra already exists
    if hydra_dir.exists() {
        println!(".hydra/ directory already exists");
    } else {
        // Create .hydra/ directory
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

    println!("\nInitialization complete. Edit .hydra/prompt.md with your task instructions.");
    println!(
        "Customize the template at: {}",
        Config::global_prompt_template_path().display()
    );

    // Prompt for skill setup
    println!();
    setup_skills(verbose)?;

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
