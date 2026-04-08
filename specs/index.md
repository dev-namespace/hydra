# Specs Index

Quick reference to all system specs. Search-optimized with keywords.

---

## [Hydra](./hydra.md)

Automated Claude Code task runner, iteration loop, task automation, prompt resolution, implementation plan, plan injection, positional arguments, stop signals, TASK_COMPLETE, ALL_TASKS_COMPLETE, PTY streaming, signal handling, SIGINT, SIGTERM, dry-run, verbose, max iterations, session logging, .hydra directory, config.toml, default-prompt.md, portable-pty, crossterm, raw mode, terminal input, keyboard handling, interactive mode, process group, child process.

**Source**: `src/` (Rust: main.rs, runner.rs, pty.rs, signal.rs, config.rs, prompt.rs)

---

## [Skill Setup](./skill-setup.md)

Claude Code skills, local-dev-guide, deploy-and-check, hydra init extension, skill creation, skill templates, interactive prompts, PTY spawning, project setup, development workflow, deployment workflow, build commands, dev server, docker-compose, SSH, production verification, browser automation, agent-browser, CLAUDE.md, specs.

**Source**: `src/main.rs` (init_command), `src/skill.rs`, `src/pty.rs`, `templates/skill-prompts/`

---

## [Parallel Execution](./parallel-execution.md)

Parallel plan execution, parallel tasks, folder of plans, wave-based execution, dependency analysis, concurrency sliding window, --parallel-plans flag, --parallel-tasks flag, --worktree, background subagents, orchestrator skill, /parallel-hydra skill, global skill, batch plans, --no-review, plan queue, live progress, summary table, concurrent hydra sessions, resume, progress tracking, .hydra-parallel-progress, JSONL, interrupted runs, skip completed plans, mini-plans, wave plan, scratchpad merge, task isolation, git worktree.

**Source**: `~/.claude/skills/parallel-hydra/SKILL.md` (router), `~/.claude/skills/hydra-parallel-plans/SKILL.md`, `~/.claude/skills/hydra-parallel-tasks/SKILL.md`, `src/cli.rs` (--no-review flag), `src/main.rs` (review guard)

---

## [Pi Harness](./pi-harness.md)

Multi-harness support, pi coding agent, --harness flag, harness.json, pi CLI, PiHarness, ClaudeHarness, harness trait, text_delta, stream JSON parser, pi -p, pi @file, alternative agent, pluggable harness, coding agent abstraction.

**Source**: `src/cli.rs`, `src/config.rs`, `src/pty.rs`, `src/headless.rs`, `src/runner.rs`, `src/main.rs`

---

## [Headless Mode](./headless-mode.md)

Non-interactive execution, claude -p, pipe mode, --headless flag, stdin prompt, stream-json parsing, text_delta, automation, CI/CD, batch processing, no PTY, no terminal, no TUI, parallel integration, --dangerously-skip-permissions, clean context per iteration.

**Source**: `src/headless.rs` (to be created), `src/cli.rs` (--headless flag), `src/main.rs` (routing)

---

## [TUI Mode](./tui-mode.md)

Multi-tab interface, parallel Claude instances, ratatui, tab management, Ctrl+O new tab, F8 close tab, F1-F7 switch tabs, F9 exit, tab bar, multiple PTY, concurrent sessions, split view, terminal multiplexer, tmux-like, screen-like.

**Source**: `src/tui/` (mod.rs, app.rs, ui.rs, input.rs)

---
