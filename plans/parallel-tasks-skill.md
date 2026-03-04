# Parallel Tasks Skill Implementation Plan

## Summary

Rewrite the `/hydra` global skill (`~/.claude/skills/hydra/SKILL.md`) to support two modes: the existing parallel-plans mode (renamed from `--parallel` to `--parallel-plans`) and a new parallel-tasks mode (`--parallel-tasks`) that analyzes a single plan's task dependencies, groups them into waves, and executes each wave concurrently.

([spec: Parallel Execution](../specs/parallel-execution.md)) | ([spec: Headless Mode](../specs/headless-mode.md))

## Tasks

- [x] Restructure `SKILL.md` argument parsing and mode detection: parse `$ARGUMENTS` to detect whether `$0` is a directory (parallel-plans) or file (parallel-tasks), extract `--parallel-plans N` and `--parallel-tasks N` flags, `--worktree`, and `--no-review`. Print usage if invalid. Keep the existing parallel-plans logic intact but rename `--parallel` to `--parallel-plans`.
  + ([spec: Mode Detection](../specs/parallel-execution.md#mode-detection))
  + ([spec: Argument Parsing — parallel-plans](../specs/parallel-execution.md#argument-parsing))
  + ([spec: Argument Parsing — parallel-tasks](../specs/parallel-execution.md#argument-parsing-1))
- [ ] Add the dependency analysis subagent flow: when in parallel-tasks mode, spawn a `general-purpose` background subagent that reads the plan file, analyzes task dependencies (prioritizing conflict avoidance over greedy parallelism), and outputs a structured wave plan. The orchestrator waits for the subagent, reads the wave plan, creates the `.hydra/logs/<plan-name>/` directory, and writes `wave-plan.md` there. Include the full analysis prompt in the skill with the wave plan format and conflict-avoidance instructions.
  + ([spec: Dependency Analysis](../specs/parallel-execution.md#dependency-analysis))
  + ([spec: Logs](../specs/parallel-execution.md#logs))
- [ ] Add wave plan validation and mini-plan generation: after receiving the wave plan, validate it (all tasks accounted for, no duplicates, no circular deps, valid dependency references). If invalid, report and stop. Then for each task in the current wave, generate a mini-plan file at `.hydra/waves/<plan-name>/wave-N-task-M.md` containing: plan summary, task description, spec links from the original plan, and a sibling tasks section listing other tasks in the same wave.
  + ([spec: Wave Plan Validation](../specs/parallel-execution.md#wave-plan-validation))
  + ([spec: Mini-Plan Generation](../specs/parallel-execution.md#mini-plan-generation))
- [ ] Add the wave execution loop with progress tracking and failure handling: execute waves sequentially, running tasks within each wave in parallel using the sliding window pattern (background subagents running `hydra <mini-plan> --headless`). Cap concurrency at `min(N, tasks_in_wave)`. Track progress in `.hydra/logs/<plan-name>/progress.jsonl` with 1-indexed task positions. Handle failures: continue the wave but block dependent tasks in later waves. Print wave-level progress lines and per-wave summaries. On resume, skip completed tasks.
  + ([spec: Execution Model](../specs/parallel-execution.md#execution-model-1))
  + ([spec: Failure Handling](../specs/parallel-execution.md#failure-handling-1))
  + ([spec: Resuming](../specs/parallel-execution.md#resuming))
  + ([spec: Monitoring Progress](../specs/parallel-execution.md#monitoring-progress-1))
- [ ] Add scratchpad merge, plan review, and final summary: between waves, read each task's scratchpad from `.hydra/scratchpad/<plan-name>/task-N.md` and merge into the main scratchpad with wave-stamped sections (`## Wave N Results` → `### Task M: <description>`). After all waves complete, trigger a single plan review on the original plan (unless `--no-review`). Print a final summary table across all waves showing completed, failed, and blocked tasks.
  + ([spec: Scratchpad](../specs/parallel-execution.md#scratchpad))
  + ([spec: Plan Review](../specs/parallel-execution.md#plan-review))
- [ ] Add worktree isolation mode: when `--worktree` is passed, each task in a wave runs in its own git worktree (created in `.claude/worktrees/`). After a wave completes, merge worktree changes back sequentially in task-index order. Mark tasks as FAIL if merge conflicts occur, noting which files conflicted. Without `--worktree`, tasks run in the same working directory (optimistic mode).
  + ([spec: Worktree Isolation Mode](../specs/parallel-execution.md#worktree-isolation-mode---worktree))

## Verification

- [ ] `/hydra plans/ --parallel-plans 3` runs the existing parallel-plans flow correctly
- [ ] `/hydra plan.md --parallel-tasks 3` analyzes dependencies, generates wave plan, and executes waves
- [ ] Wave plan is written to `.hydra/logs/<plan-name>/wave-plan.md` and is readable
- [ ] Mini-plans include sibling tasks section
- [ ] Failed tasks block dependents in later waves but don't stop the current wave
- [ ] Scratchpad merges between waves with wave-stamped sections
- [ ] Resume skips completed tasks via `progress.jsonl`
- [ ] `--worktree` creates isolated worktrees and merges back in task-index order
- [ ] `--no-review` skips plan review in both modes
- [ ] Invalid arguments print helpful usage message
