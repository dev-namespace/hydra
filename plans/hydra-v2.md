# Hydra v2 Implementation Plan

## Summary

Rewrite hydra as a Rust CLI tool that wraps the existing bash PTY logic. Single binary, no runtime dependencies, portable across projects. ([spec: Hydra](../specs/hydra.md))

## Tasks

- [ ] Initialize Cargo project and add dependencies (clap, serde, toml, dirs, chrono, tempfile, thiserror, ctrlc, nix)
- [ ] Implement CLI argument parsing with clap derive (subcommands: run, init, install; flags: --max, --dry-run, --verbose, --prompt)
- [ ] Implement error types with thiserror and map to exit codes (0, 1, 2)
- [ ] Implement TOML config loading from `~/.hydra/config.toml` with defaults merging
- [ ] Implement prompt resolution with priority chain (CLI flag > .hydra/prompt.md > ./prompt.md > global default)
- [ ] Extract PTY logic from hydra.sh and embed via include_str!, write to temp file at runtime
- [ ] Implement signal handling (SIGINT for immediate exit, SIGTERM for graceful shutdown, .hydra-stop file check)
- [ ] Implement core runner loop (spawn bash script, monitor for stop signals, respect max iterations)
- [ ] Implement session logging to `.hydra/logs/hydra-YYYYMMDD-HHMMSS.log`
- [ ] Implement `hydra init` command (create .hydra/ structure, update .gitignore)
- [ ] Implement `hydra --install` command (copy to ~/.local/bin, set permissions)
- [ ] Wire up main.rs entry point with command routing and error handling

## Verification

- [ ] `hydra --help` and `hydra --version` work
- [ ] `hydra --dry-run` shows config without running Claude
- [ ] `hydra init` creates `.hydra/` and updates `.gitignore`
- [ ] `hydra` runs Claude loop with correct prompt resolution
- [ ] Stop signals (`###TASK_COMPLETE###`, `###ALL_TASKS_COMPLETE###`) work correctly
- [ ] Ctrl+C exits immediately, SIGTERM/stop file exit gracefully
- [ ] Logs are written to `.hydra/logs/`
- [ ] `hydra --install` installs to `~/.local/bin`
