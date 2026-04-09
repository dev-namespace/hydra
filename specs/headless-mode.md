# Headless Mode

Non-interactive execution mode using the coding-agent harness in print/pipe mode (`claude -p` or `pi -p --mode json`) instead of a PTY. Designed for automation, CI/CD, and parallel execution.

## User Capabilities

### Running Headless
- Users can run `hydra <plan> --headless` to execute without a terminal
- Users can combine with other flags: `hydra <plan> --headless --no-review --max 10`
- Users can pick the harness with `--harness pi` or `--harness claude`; headless mode works with either
- Users can use headless mode in CI/CD pipelines, scripts, and automated workflows
- Users can use headless mode standalone or let the parallel skill use it internally

### Output Behavior
- Hydra prints its own minimal status lines to stdout (iteration count, signal detected, completion)
- Full Claude output is written to the log file (same `.hydra/logs/` location as PTY mode)
- No TUI rendering, no terminal escape sequences, no raw mode
- Stream-json from Claude is parsed internally — never exposed to stdout

### Iteration Model
- Each iteration is a fresh harness invocation in print/pipe mode (clean context, no `--continue`)
- Prompt is piped via stdin (e.g. `echo "$prompt" | claude -p ...` or `echo "$prompt" | pi -p --mode json`)
- The harness runs to natural completion (no mid-stream killing needed)
- Hydra parses the harness-specific stream-json for text content and stop signals
- Next iteration starts fresh if TASK_COMPLETE detected
- Loop ends on ALL_TASKS_COMPLETE, max iterations, or stop signal

## Constraints

### CLI Flag
- `--headless` (long only, no short form)
- No effect on `init`, `tui`, or `--install` commands
- Compatible with all existing flags: `--max`, `--timeout`, `--verbose`, `--no-review`, `--prompt`, `--reset-plan`, `--dry-run`, `--harness`
- Default: false (PTY mode remains the default)

### Harness Selection
- The `--harness <name>` flag (or `.hydra/harness.json`) selects which coding agent to spawn in headless mode
- `--harness claude` uses `claude -p --dangerously-skip-permissions --output-format stream-json --verbose`
- `--harness pi` uses `pi -p --mode json`
- Pi manages its own tool permissions, so there is no skip-permissions flag for pi
- Each harness ships its own stream-json parser; hydra picks the right one at runtime
- See [Pi Harness](./pi-harness.md) for the full equivalence table and pi's JSON event format

### Harness Invocation
- Claude command: `claude -p --dangerously-skip-permissions --output-format stream-json --verbose`
- Pi command: `pi -p --mode json`
- Prompt delivery: piped via stdin (avoids shell argument length limits)
- Each iteration is a separate process (no `--continue`, clean context per task)
- Working directory: inherited from hydra's cwd
- Env var cleanup is harness-specific (claude removes `CLAUDECODE`, pi removes nothing)

### Stream-JSON Parsing
- Hydra reads newline-delimited JSON from the harness's stdout
- Claude parser: filters for `assistant` messages (`{"type":"assistant","message":{"content":[...]}}`), extracts text from `{"text":"..."}` content blocks, ignores tool_use blocks, system events, user messages, and result events
- Pi parser: filters for `message_update` events whose `assistantMessageEvent.type` is `text_delta`, extracts the `delta` string, ignores thinking, toolcall, session, and lifecycle events
- Both parsers append text to a per-iteration accumulator
- Both parsers scan the accumulator for `###TASK_COMPLETE###` and `###ALL_TASKS_COMPLETE###` (shared helper — `ALL_TASKS_COMPLETE` takes priority)
- Text content is simultaneously written to the session log file
- No ANSI stripping needed (stream-json text content is plain text)

### Timeout Handling
- Same `--timeout` flag applies (default: 3000s)
- If the harness process exceeds the timeout, hydra sends SIGTERM then SIGKILL
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
- Review uses the same harness that ran the iterations, in print mode, with the review prompt piped via stdin (`claude -p` or `pi -p`)
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
- [Pi Harness](./pi-harness.md) - multi-harness support and pi's JSON event format
- [Parallel Execution](./parallel-execution.md) - parallel skill that uses headless internally

## Source

- [src/cli.rs](../src/cli.rs) - `--headless` and `--harness` flag definitions
- [src/headless.rs](../src/headless.rs) - headless execution module with per-harness stream-json parsers
- [src/harness.rs](../src/harness.rs) - harness abstraction (claude / pi command + args + env)
- [src/runner.rs](../src/runner.rs) - shared iteration logic
- [src/main.rs](../src/main.rs) - routing to headless vs PTY mode and plan-review dispatch
