# Specs Index

Quick reference to all system specs. Search-optimized with keywords.

---

## [Hydra](./hydra.md)

Automated Claude Code task runner, iteration loop, task automation, prompt resolution, implementation plan, plan injection, positional arguments, stop signals, TASK_COMPLETE, ALL_TASKS_COMPLETE, PTY streaming, signal handling, SIGINT, SIGTERM, dry-run, verbose, max iterations, session logging, .hydra directory, config.toml, default-prompt.md, portable-pty, crossterm, raw mode, terminal input, keyboard handling, interactive mode, process group, child process.

**Source**: `src/` (Rust: main.rs, runner.rs, pty.rs, signal.rs, config.rs, prompt.rs)

---

## [Skill Setup](./skill-setup.md)

Claude Code skills, local-dev-guide, deploy-and-check, hydra init extension, skill creation, skill templates, interactive prompts, PTY spawning, project setup, development workflow, deployment workflow, build commands, dev server, docker-compose, SSH, production verification.

**Source**: `src/main.rs` (init_command), `src/skill.rs`, `src/pty.rs`, `templates/skill-prompts/`

---
