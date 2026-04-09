# Pi Harness

Multi-harness support for hydra: run iterations with `pi` (the pi coding agent) instead of Claude Code.

## Reference

- Pi coding agent repo: https://github.com/badlogic/pi-mono/tree/main/packages/coding-agent
- Use `/refs` to browse the pi-mono source code for implementation details, JSON format specs, and CLI behavior
- Pi CLI help: `pi --help`
- Pi JSON format docs: `packages/coding-agent/docs/json.md` in the pi-mono repo
- Pi RPC format docs: `packages/coding-agent/docs/rpc.md` in the pi-mono repo

## User Capabilities

- Users can run `hydra --harness pi` to use the pi coding agent instead of Claude Code
- Users can run `hydra --harness claude` explicitly (or omit — claude is the default)
- Users can set a default harness in `.hydra/harness.json` so `--harness` flag is not needed every time
- Users can override the config file default with the `--harness` CLI flag (CLI wins)
- `hydra init` automatically creates `.hydra/harness.json` with `{"harness": "claude"}` so users have a discoverable place to switch the default
- The pi harness supports all hydra features in scope: PTY mode, headless mode, iteration loop, plan injection, stop signals, timeouts, plan review, and parallel execution

## Out of Scope

- **TUI mode (`hydra tui`)** does not need pi support. The multi-tab TUI continues to spawn Claude Code only. If `--harness pi` is passed alongside `tui`, hydra may either ignore it or error — implementer's choice, but it must not crash.
- **Pi-specific configuration passthrough** (model, provider, thinking level, api keys). Pi manages its own config; hydra does not surface `--provider`, `--model`, etc. Users configure pi directly via env vars or pi's own config files.

## Constraints

### CLI Flag
- `--harness <name>` (long only, no short form)
- Valid values: `claude`, `pi`
- Default: `claude` (unless overridden by `.hydra/harness.json`)
- Compatible with all existing flags

### Config File (`.hydra/harness.json`)
- Location: `.hydra/harness.json` (project-level, in the local `.hydra/` directory)
- Format:
  ```json
  {
    "harness": "claude"
  }
  ```
- Created automatically by `hydra init` (and `hydra init --quick`) with `claude` as the default value
- If the file doesn't exist (e.g. project predates this feature), hydra still defaults to `claude` — no error
- CLI `--harness` flag overrides the config file

### Harness Equivalence Table

| Hydra need | Claude | Pi |
|---|---|---|
| PTY interactive | `claude --dangerously-skip-permissions <prompt-file>` | `pi @<prompt-file>` |
| Headless print | `claude -p --dangerously-skip-permissions --output-format stream-json --verbose` (stdin pipe) | `pi -p --mode json` (stdin pipe) |
| Text extraction (headless) | Filter `type:"assistant"` events, extract `message.content[].text` | Filter `type:"message_update"` events where `assistantMessageEvent.type` is `"text_delta"`, extract `assistantMessageEvent.delta` |
| Permission skip | `--dangerously-skip-permissions` | Not needed (pi manages its own tool permissions) |
| Env var cleanup | Remove `CLAUDECODE` env var | No equivalent needed |
| Stop signals | Prompt-based (`###TASK_COMPLETE###`, `###ALL_TASKS_COMPLETE###`) | Same — prompt-based, harness-agnostic |
| Plan review (interactive) | `claude <review-prompt-file>` | `pi @<review-prompt-file>` |
| Plan review (headless) | `claude -p --dangerously-skip-permissions` (stdin pipe) | `pi -p` (stdin pipe) |

### Pi JSON Streaming Format

Pi's `--mode json` outputs newline-delimited JSON. Key event types for hydra:

- **Session start**: `{"type":"session","version":3,...}`
- **Text content**: `{"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":N,"delta":"chunk"},...}`
- **Text complete**: `{"type":"message_update","assistantMessageEvent":{"type":"text_end","contentIndex":N,"content":"full text"},...}`
- **Thinking**: `{"type":"message_update","assistantMessageEvent":{"type":"thinking_delta",...}}` (ignored by hydra)
- **Tool use**: `{"type":"message_update","assistantMessageEvent":{"type":"toolcall_start|toolcall_delta|toolcall_end",...}}` (ignored by hydra)
- **Done**: `{"type":"message_update","assistantMessageEvent":{"type":"done","reason":"stop"|"length"|"toolUse",...}}`
- **Agent end**: `{"type":"agent_end","messages":[...]}`

Hydra only needs to extract text from `text_delta` events and scan for stop signals. All other event types are ignored.

### Terminal Reset

The PTY terminal reset sequence in `pty.rs` is designed for Claude Code's TUI. Pi may use different terminal modes. The harness should be able to provide its own reset sequence if needed, or the existing comprehensive reset should be sufficient for both (it covers all standard terminal modes).

## Roadmap

High-level milestones for `/auto-sprint` execution. Each milestone is a self-contained unit of work.

**Before starting any milestone**: run `/spec study` to load all relevant specs into context. The CLAUDE.md for this project requires it.

- [x] **Milestone 1: Harness abstraction layer** — Add `--harness` CLI flag to `cli.rs`. Create `.hydra/harness.json` config loading in `config.rs` and update `hydra init` (both interactive and `--quick`) to write the file with `{"harness": "claude"}` as the default. Define a `Harness` trait (or enum with match arms) that encapsulates: command name, PTY args, headless args, stream JSON parsing, env var overrides, and review command building. Extract the existing Claude-specific code into a `ClaudeHarness` implementation. All existing tests must still pass — this is a pure refactor with no behavior change.

- [x] **Milestone 2: Pi harness for PTY mode** — Implement `PiHarness` for PTY mode. The PTY spawn in `pty.rs` should use the harness to build the command (`pi @<prompt-file>` instead of `claude --dangerously-skip-permissions <prompt-file>`). Use `/refs` to browse https://github.com/badlogic/pi-mono/tree/main/packages/coding-agent for any pi-specific PTY behavior, and verify empirically that pi actually consumes `@<prompt-file>` as the initial message and starts running rather than waiting at an idle prompt. Note: TUI mode (`hydra tui`) is out of scope and should continue to spawn Claude only. Test with `hydra --harness pi --dry-run` and a real PTY run.

- [x] **Milestone 3: Pi harness for headless mode** — Implement `PiHarness` for headless mode. Build a `PiStreamJsonParser` that extracts text from `text_delta` events (field path: `assistantMessageEvent.delta` where `assistantMessageEvent.type == "text_delta"`). The headless runner in `headless.rs` should use the harness to build the command (`pi -p --mode json`) and select the correct parser. Use `/refs` to check `packages/coding-agent/docs/json.md` for the exact format. Test with `hydra --harness pi --headless`.

- [x] **Milestone 4: Plan review + parallel passthrough** — Wire the harness through plan review in `main.rs` (both interactive and headless review paths). Update the `/hydra` parallel skills (`~/.claude/skills/hydra-parallel-plans/SKILL.md` and `~/.claude/skills/hydra-parallel-tasks/SKILL.md`) to pass `--harness` through to hydra subcommands. Update dry-run output to show the active harness.

- [x] **Milestone 5: Spec + test + docs update** — Update `specs/hydra.md` and `specs/headless-mode.md` to document the `--harness` flag. Add unit tests for `PiStreamJsonParser` (same pattern as existing `StreamJsonParser` tests in `headless.rs`). Add integration test or manual verification for both harnesses in both modes. Update `specs/index.md`.

## Related specs

- [Hydra](./hydra.md) - core task runner, PTY management, iteration loop
- [Headless Mode](./headless-mode.md) - non-interactive execution, stream-json parsing
- [Parallel Execution](./parallel-execution.md) - parallel skills that pass flags through

## Source

- [src/cli.rs](../src/cli.rs) - `--harness` flag (to be added)
- [src/config.rs](../src/config.rs) - harness config loading (to be added)
- [src/pty.rs](../src/pty.rs) - PTY command building (to be modified)
- [src/headless.rs](../src/headless.rs) - headless command + parser (to be modified)
- [src/runner.rs](../src/runner.rs) - runner uses harness (to be modified)
- [src/main.rs](../src/main.rs) - plan review uses harness (to be modified)
