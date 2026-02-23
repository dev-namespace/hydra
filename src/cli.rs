use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Automated Claude Code task runner
///
/// Run with an optional implementation plan:
///   hydra [PLAN] [OPTIONS]
///
/// Examples:
///   hydra                    Run using prompt only
///   hydra plan.md            Run with plan injected after prompt
///   hydra plan.md --max 5    Run with plan, limit to 5 iterations
#[derive(Parser, Debug)]
#[command(name = "hydra")]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Optional path to implementation plan file (injected after prompt)
    #[arg(value_name = "PLAN")]
    pub plan: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,

    /// Maximum number of iterations to run
    #[arg(short, long, default_value = "20")]
    pub max: u32,

    /// Preview configuration without executing
    #[arg(long)]
    pub dry_run: bool,

    /// Enable debug output
    #[arg(short, long)]
    pub verbose: bool,

    /// Override prompt file path
    #[arg(short, long)]
    pub prompt: Option<PathBuf>,

    /// Install hydra to ~/.local/bin
    #[arg(long)]
    pub install: bool,

    /// Timeout per iteration in seconds (default: 1200 = 20 minutes)
    /// If Claude doesn't output a stop signal within this time, the iteration is terminated
    #[arg(short, long, default_value = "1200")]
    pub timeout: u64,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Initialize .hydra/ directory in current project
    Init,

    /// Start TUI mode with multi-tab interface
    Tui {
        /// Optional path to implementation plan file
        #[arg(value_name = "PLAN")]
        plan: Option<std::path::PathBuf>,
    },
}

impl Cli {
    /// Check if this is an init command
    pub fn is_init(&self) -> bool {
        matches!(self.command, Some(Command::Init))
    }

    /// Check if this is a tui command
    pub fn is_tui(&self) -> bool {
        matches!(self.command, Some(Command::Tui { .. }))
    }

    /// Get the plan path from the tui subcommand (if any)
    pub fn tui_plan(&self) -> Option<&std::path::PathBuf> {
        if let Some(Command::Tui { plan }) = &self.command {
            plan.as_ref()
        } else {
            None
        }
    }

    /// Check if this is an install command
    pub fn is_install(&self) -> bool {
        self.install
    }
}
