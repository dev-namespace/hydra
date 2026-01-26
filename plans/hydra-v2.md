# Hydra v2 Implementation Plan

## Summary

Rewrite hydra as a Rust CLI tool that wraps the existing bash PTY logic. Single binary, no runtime dependencies, portable across projects. ([spec: Hydra](../specs/hydra.md))

## Tasks

- [x] Initialize Cargo project and add dependencies (clap, serde, toml, dirs, chrono, tempfile, thiserror, ctrlc, nix)
- [x] Implement CLI argument parsing with clap derive (subcommands: run, init, install; flags: --max, --dry-run, --verbose, --prompt)
- [x] Implement error types with thiserror and map to exit codes (0, 1, 2)
- [x] Implement TOML config loading from `~/.hydra/config.toml` with defaults merging
- [x] Implement prompt resolution with priority chain (CLI flag > .hydra/prompt.md > ./prompt.md > global default)
- [x] Extract PTY logic from hydra.sh and embed via include_str!, write to temp file at runtime
- [x] Implement signal handling (SIGINT for immediate exit, SIGTERM for graceful shutdown, .hydra-stop file check)
- [x] Implement core runner loop (spawn bash script, monitor for stop signals, respect max iterations)
- [x] Implement session logging to `.hydra/logs/hydra-YYYYMMDD-HHMMSS.log`
- [x] Implement `hydra init` command (create .hydra/ structure, update .gitignore)
- [x] Implement `hydra --install` command (copy to ~/.local/bin, set permissions)
- [x] Wire up main.rs entry point with command routing and error handling

## Verification

- [x] `hydra --help` and `hydra --version` work
- [x] `hydra --dry-run` shows config without running Claude
- [x] `hydra init` creates `.hydra/` and updates `.gitignore`
- [x] `hydra` runs Claude loop with correct prompt resolution
- [x] Stop signals (`###TASK_COMPLETE###`, `###ALL_TASKS_COMPLETE###`) work correctly
- [x] Ctrl+C exits immediately, SIGTERM/stop file exit gracefully
- [x] Logs are written to `.hydra/logs/`
- [x] `hydra --install` installs to `~/.local/bin`
