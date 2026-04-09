# Hydra

Automated coding-agent task runner. Executes tasks from implementation plans in a loop until all tasks are complete. Supports multiple coding-agent harnesses (Claude Code by default, Pi optional).

## User Capabilities

### Running Tasks
- Users can run `hydra` to start automated task execution
- Users can run `hydra <plan>` to run with an implementation plan injected
- Users can specify maximum iterations with `--max N`
- Users can specify iteration timeout with `--timeout N` (seconds, default: 3000 = 50 minutes)
- Users can preview configuration with `--dry-run` without executing (dry-run output shows the resolved harness)
- Users can enable debug output with `--verbose`
- Users can override the prompt file with `--prompt <path>`
- Users can reset a plan's checkboxes and clear its scratchpad with `--reset-plan`
- Users can select a coding-agent harness with `--harness <name>` (`claude` or `pi`)

### Implementation Plan
- Users can provide an optional first positional argument as a path to an implementation plan
- When provided, the plan content is appended to the prompt with a `## Implementation Plan` header
- If the plan file path is provided but the file doesn't exist, hydra exits with a helpful error

### Scratchpad
- When a plan is provided, hydra creates `.hydra/scratchpad/<plan-name>.md` if it doesn't exist
- The scratchpad path is injected into the prompt as a `## Scratchpad` section
- Iterations can read/write the scratchpad to share notes across runs
- Existing scratchpad content is preserved across runs

### Project Setup
- Users can run `hydra init` to interactively set up skills and optionally create a `.hydra/` directory
- Users can run `hydra init --quick` to just create the `.hydra/` folder without any interactive prompts
- Users can run `hydra --install` to install the binary to `~/.local/bin`

### Prompt Configuration
- Users can create `~/.hydra/default-prompt.md` as a global fallback
- Users can create `./prompt.md` in the project root
- Users can create `./.hydra/prompt.md` for project-specific prompts
- Users can override any prompt with the `--prompt` flag

### Plan Review
- After all tasks complete, if a plan file was provided, hydra automatically launches a plan review in a new interactive Claude session
- The review session runs `/hydra-review <plan-path>` so the user can see a quality report and interact with Claude to discuss findings
- If the review fails to launch, hydra prints a warning but still exits successfully

### Stopping Execution
- Users can press Ctrl+C once for graceful termination (kills Claude, finishes iteration)
- Users can press Ctrl+C twice for immediate force quit
- Users can press Ctrl+D as equivalent to Ctrl+C
- Users can send SIGTERM for graceful shutdown after current iteration
- Users can create `.hydra-stop` file to stop after current iteration

### Logging
- Users can find session logs in `.hydra/logs/hydra-YYYYMMDD-HHMMSS.log`
- When a plan file is provided, the plan name is used as the log filename prefix: `.hydra/logs/<plan-name>-YYYYMMDD-HHMMSS.log`
- The plan name is also logged in the session header inside the log file

## Constraints

### CLI Signature
```
hydra [PLAN] [OPTIONS]      # Run task loop (plan is optional)
hydra init                  # Initialize .hydra/ directory (interactive)
hydra init --quick          # Just create .hydra/ folder, no prompts
hydra --install             # Install to ~/.local/bin
```

### Options
- `--prompt <path>`, `-p`: Override system prompt file
- `--max <N>`, `-m`: Maximum iterations (default: 20)
- `--timeout <N>`, `-t`: Iteration timeout in seconds (default: 3000 = 50 minutes)
- `--reset-plan`: Uncheck all plan checkboxes (`- [x]` → `- [ ]`) and reset scratchpad to initial header. Requires a plan file argument.
- `--harness <name>`: Coding-agent harness to drive. Valid values: `claude`, `pi`. Overrides `.hydra/harness.json`. Default: `claude`.
- `--dry-run`: Preview configuration without executing
- `--verbose`, `-v`: Enable debug output

### Prompt Resolution Priority
1. `--prompt <path>` (CLI override, highest)
2. `./.hydra/prompt.md` (project-specific)
3. `./prompt.md` (current directory)
4. `~/.hydra/default-prompt.md` (global fallback, lowest)

### Harness Resolution Priority
The coding-agent harness (the CLI hydra spawns each iteration) is resolved from:
1. `--harness <name>` (CLI override, highest)
2. `./.hydra/harness.json` (project-level config, created by `hydra init`)
3. Built-in default: `claude`

Valid values: `claude`, `pi`. Unknown names produce a helpful error. The missing-file case is silent — hydra falls back to `claude` when `.hydra/harness.json` doesn't exist so older projects keep working without a migration step. TUI mode (`hydra tui`) always uses Claude regardless of this setting. See [Pi Harness](./pi-harness.md) for the full equivalence table and streaming format details.

### Plan Injection
When a plan file is provided as the first positional argument:
1. Verify the plan file exists (exit with error if not)
2. Append a reference to the plan path in the prompt:
```
[prompt content]

## Implementation Plan

The implementation plan is located at: [plan file path]
```

### Stop Signals
- Claude must output `###TASK_COMPLETE###` when one task is done but more remain
- Claude must output `###ALL_TASKS_COMPLETE###` when all tasks are finished
- Hydra monitors output and terminates the iteration upon signal detection
- If no stop signal is received within the timeout period (default: 50 minutes), hydra terminates the iteration and starts the next one (safety mechanism)
- When a timeout occurs and a scratchpad exists, hydra appends a timeout note to the scratchpad including the iteration number, timestamp, and log file path — so the next iteration can check what was in progress and resume or retry the interrupted work

### Exit Codes
- `0`: Success (all tasks complete, max iterations reached, or dry-run)
- `1`: Stopped (user interrupt, SIGTERM, or stop file)
- `2`: Error (no prompt file found, or plan file not found)

### Configuration Defaults
- Max iterations: 20
- Timeout: 3000 seconds (50 minutes)
- Verbose: false
- Stop file: `.hydra-stop`

### Directory Structure
```
~/.hydra/                    # Global config (auto-created)
├── config.toml              # Global defaults
└── default-prompt.md        # Fallback prompt template

./.hydra/                    # Per-project (auto-created on first run)
├── logs/                    # Session logs
├── reviews/                 # Headless plan review outputs
├── scratchpad/              # Cross-iteration notes (auto-created with plan)
├── harness.json             # Default harness selection ({"harness": "claude"})
└── prompt.md                # Project-specific prompt (optional)
```

### Automatic Behaviors
- `.hydra/` directory is auto-created on first run
- `.hydra/` is auto-added to `.gitignore` if not present
- `~/.hydra/default-prompt.md` is auto-created with template if no prompt found
- On macOS, installed binary is re-signed with ad-hoc signature to satisfy Gatekeeper

## Architecture

### Components
- **Rust CLI**: Argument parsing (clap), config management (TOML), prompt resolution
- **Native PTY manager**: Uses `portable-pty` crate for cross-platform PTY allocation
- **Terminal I/O**: Uses `crossterm` for raw mode input handling and keyboard events
- **Signal handling**: SIGINT/SIGTERM with child process group management

### PTY Lifecycle
When an iteration completes (signal detected, timeout, or termination):
1. Child process (Claude) is terminated via SIGTERM
2. Wait for child to exit (with 500ms timeout, then SIGKILL)
3. Drop PTY pair to close file descriptors (causes EOF on reader thread)
4. Wait for reader thread to exit (with 500ms timeout)
5. Restore terminal to normal mode with comprehensive reset sequence

This ensures clean process termination without leaving orphaned threads or hanging terminals.

### Terminal Reset
When Claude is terminated, hydra sends a comprehensive terminal reset sequence to recover from any TUI state Claude may have left behind. This includes:
- XON (Ctrl+Q) to resume if XOFF stopped the terminal
- CAN to cancel any partial escape sequence
- Disable synchronized output mode (`[?2026l`) - critical for Claude's TUI
- Disable all mouse tracking modes
- Disable bracketed paste mode
- Disable focus reporting
- Disable kitty keyboard protocol
- Exit alternate screen buffer
- Reset cursor visibility and attributes
- Full terminal reset (RIS)

The synchronized output mode (`[?2026h`) is particularly important - if Claude's TUI enables it but gets killed before sending the closing `[?2026l`, the terminal will buffer all output and appear frozen.

### Config File (`~/.hydra/config.toml`)
```toml
max_iterations = 10
timeout_seconds = 3000
verbose = false
stop_file = ".hydra-stop"
```

## Related specs

- [Pi Harness](./pi-harness.md) - multi-harness support, `--harness` flag, pi coding agent
- [Headless Mode](./headless-mode.md) - non-interactive execution, stream-json parsing
- [Parallel Execution](./parallel-execution.md) - parallel skills that pass flags through

### Interactive Mode
- Users can type while Claude is running (input forwarded to PTY)
- Users can use arrow keys, function keys, and special keys
- Users see Claude's TUI output streamed in real-time

## Source

- [src/main.rs](../src/main.rs) - Entry point and CLI setup
- [src/runner.rs](../src/runner.rs) - Main iteration loop
- [src/pty.rs](../src/pty.rs) - PTY manager for harness execution
- [src/headless.rs](../src/headless.rs) - Headless (print-mode) runner
- [src/harness.rs](../src/harness.rs) - Harness abstraction (claude / pi)
- [src/signal.rs](../src/signal.rs) - Signal handling and child process management
- [src/config.rs](../src/config.rs) - Configuration loading
- [src/prompt.rs](../src/prompt.rs) - Prompt resolution
