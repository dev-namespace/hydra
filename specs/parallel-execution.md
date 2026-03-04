# Parallel Execution

Two modes for concurrent execution via the `/hydra` global skill: **parallel-plans** (run independent plan files concurrently) and **parallel-tasks** (run tasks within a single plan concurrently by wave).

## Parallel Plans Mode

Run all `.md` plan files in a folder concurrently via background subagents.

### User Capabilities

#### Invoking
- Users run `/hydra plans/ --parallel-plans 4` to execute all plans in a folder with concurrency 4
- Users run `/hydra plans/ --parallel-plans` to use the default concurrency of 3

#### Monitoring Progress
- Users see a live progress line each time a plan completes (e.g., `[2/7] auth-plan.md completed (exit 0)`)
- Users see a final summary table when all plans finish showing: plan name, status (pass/fail), and errors if any
- Users are informed of failures as they happen but execution continues

#### Failure Handling
- Failed plans (non-zero exit) are logged and reported but do not stop remaining plans
- The final summary highlights all failures with their exit codes
- Users can re-run individual failed plans manually after reviewing

#### Resuming Interrupted Runs
- Progress is tracked in `<plan-folder>/.hydra-parallel-progress` (JSONL dotfile)
- Each completed plan is recorded as a JSON line: `{"plan":"name.md","status":"PASS|FAIL","exit":0}`
- On re-run, previously completed plans are skipped automatically
- Users see which plans are being skipped and which are queued for execution
- If all plans are already recorded as complete, the skill shows the summary and stops
- Users delete `.hydra-parallel-progress` manually to force a full re-run
- The progress file is never auto-deleted

### Constraints

#### Argument Parsing
- `$0`: folder path containing plan `.md` files (required)
- `--parallel-plans N`: max concurrent plans (optional, default: 3)
- Skill globs `<folder>/*.md` to discover plan files
- If folder is empty or has no `.md` files, report and exit

#### Progress File
- Location: `<plan-folder>/.hydra-parallel-progress`
- Format: JSONL (one JSON object per line, append-only)
- Each line: `{"plan":"<filename>","status":"PASS"|"FAIL","exit":<code>}`
- Read on startup to determine which plans to skip
- Appended after each plan completes (crash-safe)
- Never auto-deleted — user removes manually to start fresh

#### Execution Model
- One `general-purpose` background subagent per active plan
- Sliding window: max N subagents running at once
- When a subagent completes, the next plan from the queue is launched
- Each subagent runs `hydra <plan> --headless` via Bash
- Subagents absorb verbose hydra output in their own context window
- Subagents return only: plan name, exit code, and errors/concerns (if any)

#### Orchestrator Behavior
- Orchestrator stays lean — never reads raw hydra logs directly
- Orchestrator tracks: queue, active slots, completed results
- Orchestrator prints live progress as plans complete
- Orchestrator prints final summary table when all plans finish
- Orchestrator does NOT run in a subagent — it IS the main Claude session

---

## Parallel Tasks Mode

Run tasks within a single implementation plan concurrently by analyzing dependencies and grouping into sequential waves.

### User Capabilities

#### Invoking
- Users run `/hydra plan.md --parallel-tasks 3` to execute a plan's tasks with concurrency 3
- Users run `/hydra plan.md --parallel-tasks` to use the default concurrency of 3
- Users can add `--worktree` for git worktree isolation per task (default: optimistic, same worktree)

#### Workflow
1. A dedicated analysis subagent reads the plan and produces a wave plan
2. The wave plan is written to `.hydra/logs/<plan-name>/wave-plan.md` for user inspection
3. Tasks within each wave run in parallel; waves run sequentially
4. Between waves, per-task scratchpads are merged into the main scratchpad
5. After all waves complete, a single plan review runs on the original plan

#### Monitoring Progress
- Users see wave-level progress: `Wave 1/3: [2/3] task-name — exit 0 (pass)`
- Users see per-wave summaries as each wave completes
- Users see a final summary table across all waves when done

#### Failure Handling
- A failed task does not stop other tasks in the same wave
- Tasks in later waves that depend on a failed task are blocked (skipped)
- Tasks in later waves that do NOT depend on the failed task still run
- The final summary shows: completed, failed, and blocked tasks

#### Resuming
- Progress tracked in `.hydra/logs/<plan-name>/progress.jsonl`
- Each completed task recorded by 1-indexed position: `{"task":1,"description":"<first 80 chars>","wave":1,"status":"PASS"|"FAIL","exit":0}`
- Task identity is based on position (1-indexed) in the original plan, not description hash — this is predictable and survives minor description edits
- On re-run, the analysis subagent re-analyzes but completed tasks are skipped
- Users delete `progress.jsonl` to force a full re-run

#### Scratchpad
- Each task gets its own scratchpad: `.hydra/scratchpad/<plan-name>/task-N.md`
- Between waves, task scratchpads are merged into the main scratchpad with wave-stamped sections:
  ```
  ## Wave 1 Results

  ### Task 1: <task description>
  <content from task-1.md>

  ### Task 2: <task description>
  <content from task-2.md>
  ```
- Later waves can read the merged scratchpad to see prior results

#### Logs
- Per-task logs: `.hydra/logs/<plan-name>/task-N-YYYYMMDD-HHMMSS.log`
- All logs for a plan share the `.hydra/logs/<plan-name>/` subdirectory
- The wave plan is also saved here: `.hydra/logs/<plan-name>/wave-plan.md`

#### Plan Review
- Single review of the original plan file after all waves complete
- Review agent sees the full merged scratchpad for context
- Review output saved to `.hydra/reviews/<plan-name>.md`

### Constraints

#### Argument Parsing
- `$0`: path to a single `.md` plan file (required)
- `--parallel-tasks N`: max concurrent tasks per wave (optional, default: 3)
- `--worktree`: use git worktrees for task isolation (optional, default: false/optimistic)

#### Dependency Analysis
- A dedicated `general-purpose` background subagent reads the plan
- The subagent analyzes task descriptions, phase structure (as hints), and logical dependencies
- **Conflict avoidance over parallelism**: The analyzer must prioritize avoiding file conflicts over maximizing parallelism. If two tasks might touch the same files, modules, or shared infrastructure (e.g., barrel exports, config files, shared types), they go in separate waves even if they have no logical dependency. A conservative wave plan that succeeds is far more valuable than an aggressive one that causes merge conflicts or broken builds.
- Output: structured markdown wave plan written to `.hydra/logs/<plan-name>/wave-plan.md`
- Wave plan format:
  ```markdown
  # Wave Plan: <plan-name>

  ## Wave 1
  - Task 1: <description> (from: Phase 1)
  - Task 2: <description> (from: Phase 1)
  - Task 3: <description> (from: Phase 1)

  ## Wave 2
  - Task 4: <description> (from: Phase 2, depends on: Task 1)
  - Task 5: <description> (from: Phase 2, depends on: Task 2, Task 3)

  ## Wave 3 (Verification)
  - All tests pass
  - Feature-specific verification
  ```
- Phases in the original plan are hints, not hard boundaries — the subagent may split or merge across phases
- Verification tasks always go in the final wave

#### Mini-Plan Generation
- For each task, the orchestrator generates a mini-plan file in `.hydra/waves/<plan-name>/wave-N-task-M.md`
- Each mini-plan contains:
  - The plan summary (from original plan) for context
  - The specific task description
  - Relevant spec links (from original plan)
  - A "Sibling tasks" section listing other tasks running in the same wave (description only, no instructions to work on them) — this lets agents avoid stepping on each other's files
- Mini-plans are temporary — used for the hydra invocation then available for debugging

#### Execution Model
- Waves execute sequentially: wave 1 completes fully before wave 2 starts
- Within a wave: one `general-purpose` background subagent per task
- Concurrency capped at min(N, tasks_in_wave) — small waves don't waste slots
- Each subagent runs `hydra <mini-plan> --headless` via Bash
- Subagents return: task name, exit code, and errors/concerns (if any)

#### Worktree Isolation Mode (`--worktree`)
- Each task in a wave runs in its own git worktree
- Worktrees created in `.claude/worktrees/` (standard Claude Code location)
- After a wave completes, worktree changes are merged back sequentially in task-index order (task 1 first, task 2 second, etc.) — deterministic ordering avoids ambiguous merge results
- Merge conflicts cause the task to be marked as FAIL with a note about which files conflicted
- Without `--worktree`: all tasks run in the same working directory (optimistic mode — user is responsible for non-overlapping file edits)

#### Wave Plan Validation
- After receiving the wave plan from the analysis subagent, the orchestrator validates it before executing:
  - All tasks from the original plan are accounted for (none missing, none duplicated)
  - No task appears in multiple waves
  - Dependencies reference valid task indices
  - No circular dependencies exist
- If validation fails, the orchestrator reports the issue and stops — does not execute a broken wave plan

#### Orchestrator Behavior
- Orchestrator spawns analysis subagent first, waits for wave plan
- Orchestrator validates the wave plan (see Wave Plan Validation)
- Orchestrator generates mini-plans for each wave
- Orchestrator runs each wave using the sliding window pattern
- Between waves: merges scratchpads, updates progress file
- After all waves: triggers plan review on original plan
- Orchestrator does NOT run in a subagent — it IS the main Claude session

---

## Shared Constraints

### Skill Location
- Global skill at `~/.claude/skills/hydra/SKILL.md`
- Available in all projects without project-specific setup

### Mode Detection
- If `$0` is a **directory**: parallel-plans mode
- If `$0` is a **file** with `--parallel-tasks`: parallel-tasks mode
- If `$0` is a **file** without `--parallel-tasks`: error — suggest using `hydra <plan>` directly or adding `--parallel-tasks`
- `--parallel-plans` and `--parallel-tasks` are mutually exclusive

### No-Review Mode
- Users can add `--no-review` to skip the plan review step in either mode
- In headless mode, the review runs non-interactively via `claude -p`
- `--no-review` skips the review entirely (both interactive and headless)

### Hydra CLI Flags (unchanged)
- `--no-review`: skip plan review after ALL_TASKS_COMPLETE
- `--headless`: use `claude -p` pipe mode instead of PTY

## Related specs

- [Hydra](./hydra.md) - core task runner, PTY management, stop signals
- [Headless Mode](./headless-mode.md) - non-interactive execution used by both modes
- [Skill Setup](./skill-setup.md) - skill creation infrastructure

## Source

- `~/.claude/skills/hydra/SKILL.md` - global skill (both modes)
- [src/cli.rs](../src/cli.rs) - CLI argument definitions (`--no-review`, `--headless`)
- [src/main.rs](../src/main.rs) - plan review conditional
