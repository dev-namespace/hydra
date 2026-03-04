# Headless Mode

Non-interactive execution mode using `claude -p` instead of PTY. Designed for automation, CI/CD, and parallel execution.

## User Capabilities

### Running Headless
- Users can run `hydra <plan> --headless` to execute without a terminal
- Users can combine with other flags: `hydra <plan> --headless --no-review --max 10`
- Users can use headless mode in CI/CD pipelines, scripts, and automated workflows
- Users can use headless mode standalone or let the parallel skill use it internally

### Output Behavior
- Hydra prints its own minimal status lines to stdout (iteration count, signal detected, completion)
- Full Claude output is written to the log file (same `.hydra/logs/` location as PTY mode)
- No TUI rendering, no terminal escape sequences, no raw mode
- Stream-json from Claude is parsed internally — never exposed to stdout

### Iteration Model
- Each iteration is a fresh `claude -p` invocation (clean context, no `--continue`)
- Prompt is piped via stdin: `echo "$prompt" | claude -p --dangerously-skip-permissions --output-format stream-json`
- Claude runs to natural completion (no mid-stream killing needed)
- Hydra parses stream-json text deltas for stop signals
- Next iteration starts fresh if TASK_COMPLETE detected
- Loop ends on ALL_TASKS_COMPLETE, max iterations, or stop signal

## Constraints

### CLI Flag
- `--headless` (long only, no short form)
- No effect on `init`, `tui`, or `--install` commands
- Compatible with all existing flags: `--max`, `--timeout`, `--verbose`, `--no-review`, `--prompt`, `--reset-plan`, `--dry-run`
- Default: false (PTY mode remains the default)

### Claude Invocation
- Command: `claude -p --dangerously-skip-permissions --output-format stream-json --verbose`
- Prompt delivery: piped via stdin (avoids shell argument length limits)
- Each iteration is a separate process (no `--continue`, clean context per task)
- Working directory: inherited from hydra's cwd

### Stream-JSON Parsing
- Hydra reads newline-delimited JSON from Claude's stdout
- Filters for `assistant` messages (`{"type":"assistant","message":{"content":[...]}}`)
- Extracts text from content blocks (`{"text":"..."}`) and appends to a text accumulator
- Ignores tool_use content blocks, system events, user messages, and result events
- Scans accumulator for `###TASK_COMPLETE###` and `###ALL_TASKS_COMPLETE###`
- Text content is simultaneously written to the session log file
- No ANSI stripping needed (stream-json text content is plain text)

### Timeout Handling
- Same `--timeout` flag applies (default: 1200s)
- If Claude's process exceeds timeout, hydra sends SIGTERM then SIGKILL
- Timeout triggers next iteration (same behavior as PTY mode)

### Signal Handling
- Same SIGINT/SIGTERM handling as PTY mode (reuses `signal.rs`)
- First Ctrl+C: graceful stop (kill child, finish iteration)
- Second Ctrl+C: force quit
- Stop file (`.hydra-stop`): checked between iterations
- Child PID tracked for process group termination

### Logging
- Same log file location: `.hydra/logs/<plan>-YYYYMMDD-HHMMSS.log`
- Log contains extracted text content from stream-json (not raw JSON)
- Iteration markers logged same as PTY mode

### Plan Review in Headless Mode
- When all tasks complete and `--no-review` is not set, plan review runs non-interactively
- Review uses `claude -p --dangerously-skip-permissions` with the review prompt piped via stdin
- Review output is saved to `.hydra/reviews/<plan-name>.md` for the user to read later
- This allows parallel subagents to get automatic quality reviews without blocking

### What Headless Mode Skips
- No PTY allocation (`portable-pty` not used)
- No terminal raw mode (`crossterm` not used)
- No terminal reset sequences
- No keyboard input forwarding
- No TUI rendering

### Parallel Skill Integration
- The `/hydra` parallel skill always passes `--headless` when running plans via subagents
- Plan review runs non-interactively via `claude -p` (no need for `--no-review`)
- Eliminates PTY-in-non-terminal issues entirely

## Related specs

- [Hydra](./hydra.md) - core task runner, iteration loop, stop signals
- [Parallel Execution](./parallel-execution.md) - parallel skill that uses headless internally

## Source

- [src/cli.rs](../src/cli.rs) - `--headless` flag definition
- `src/headless.rs` - headless execution module (to be created)
- [src/runner.rs](../src/runner.rs) - shared iteration logic
- [src/main.rs](../src/main.rs) - routing to headless vs PTY mode
- `~/.claude/skills/hydra/SKILL.md` - parallel skill (update to use `--headless`)
