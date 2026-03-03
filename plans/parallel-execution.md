# Parallel Execution Implementation Plan

## Summary

Add `--no-review` flag to hydra CLI and create a global `/hydra` Claude Code skill that orchestrates parallel plan execution via background subagents with a concurrency sliding window.

## Tasks

- [x] Add `--no-review` CLI flag and guard the plan review step
  - Add `no_review: bool` field to `Cli` struct in `src/cli.rs`
  - Guard the review block in `src/main.rs` (around line 258) with `!cli.no_review`
  - + ([spec: No-Review Mode](../specs/parallel-execution.md#no-review-mode))
  - + ([spec: Hydra CLI Change](../specs/parallel-execution.md#hydra-cli-change))
- [ ] Create global `/hydra` skill at `~/.claude/skills/hydra/SKILL.md`
  - Skill parses `$ARGUMENTS` to extract folder path and `--parallel N` (default 3)
  - Instructs orchestrator to glob `<folder>/*.md` for plan files
  - Orchestrator spawns up to N `general-purpose` background subagents, each running `hydra <plan> --no-review` via Bash
  - Subagents return only: plan name, exit code, and errors/concerns
  - When a subagent completes, orchestrator launches next plan from queue
  - Orchestrator prints live progress per completion and a final summary table
  - `disable-model-invocation: true` (manual invoke only)
  - + ([spec: Execution Model](../specs/parallel-execution.md#execution-model))
  - + ([spec: Orchestrator Behavior](../specs/parallel-execution.md#orchestrator-behavior))
- [ ] Create `plans/test-parallel/` folder with 3-4 trivial debug plans
  - Each plan should have 1-2 fast tasks (e.g., create a temp file, echo a message, read a file)
  - Plans must complete in under 1 minute each
  - Include one plan that intentionally fails to test failure reporting
  - These are throwaway test plans, not real feature work
- [ ] Verify end-to-end: run `/hydra` skill against `plans/test-parallel/` folder
  - Confirm sliding window respects concurrency limit
  - Confirm failed plans don't stop the queue
  - Confirm final summary is accurate
  - + ([spec: Failure Handling](../specs/parallel-execution.md#failure-handling))
  - + ([spec: Monitoring Progress](../specs/parallel-execution.md#monitoring-progress))
