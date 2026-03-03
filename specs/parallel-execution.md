# Parallel Execution

Run all plans in a folder concurrently via a Claude Code global skill that orchestrates background subagents.

## User Capabilities

### Invoking Parallel Execution
- Users can run `/hydra plans/ --parallel 4` to execute all plans in a folder with concurrency 4
- Users can omit `--parallel` to use the default concurrency of 3
- Users can run `/hydra plans/` as shorthand for `/hydra plans/ --parallel 3`

### Monitoring Progress
- Users see a live progress line each time a plan completes (e.g., `[2/7] auth-plan.md completed (exit 0)`)
- Users see a final summary table when all plans finish showing: plan name, status (pass/fail), and errors if any
- Users are informed of failures as they happen but execution continues

### Failure Handling
- Failed plans (non-zero exit) are logged and reported but do not stop remaining plans
- The final summary highlights all failures with their exit codes
- Users can re-run individual failed plans manually after reviewing

### No-Review Mode
- Users can run `hydra <plan> --no-review` to skip the interactive plan review step
- The parallel skill uses `--no-review` internally so subagent hydra processes don't block on review
- `--no-review` is also useful standalone when users want fast iteration without review

## Constraints

### Skill Location
- Global skill at `~/.claude/skills/hydra/SKILL.md`
- Available in all projects without project-specific setup

### Argument Parsing
- `$0`: folder path containing plan `.md` files (required)
- `--parallel N`: max concurrent plans (optional, default: 3)
- Skill globs `<folder>/*.md` to discover plan files
- If folder is empty or has no `.md` files, report and exit

### Execution Model
- One `general-purpose` background subagent per active plan
- Sliding window: max N subagents running at once
- When a subagent completes, the next plan from the queue is launched
- Each subagent runs `hydra <plan> --no-review` via Bash
- Subagents absorb verbose hydra output in their own context window
- Subagents return only: plan name, exit code, and errors/concerns (if any)

### Orchestrator Behavior
- Orchestrator stays lean — never reads raw hydra logs directly
- Orchestrator tracks: queue, active slots, completed results
- Orchestrator prints live progress as plans complete
- Orchestrator prints final summary table when all plans finish
- Orchestrator does NOT run in a subagent — it IS the main Claude session

### Hydra CLI Change
- New flag: `--no-review` (long only, no short form)
- When set, skip the plan review launch after ALL_TASKS_COMPLETE
- No effect when no plan file is provided
- Default: false (review still launches by default)

## Related specs

- [Hydra](./hydra.md) - core task runner, PTY management, stop signals
- [Skill Setup](./skill-setup.md) - skill creation infrastructure

## Source

- `~/.claude/skills/hydra/SKILL.md` - global parallel execution skill (to be created)
- [src/cli.rs](../src/cli.rs) - CLI argument definitions (`--no-review` flag)
- [src/main.rs](../src/main.rs) - plan review conditional (guard with `--no-review`)
