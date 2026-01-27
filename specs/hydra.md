# Hydra

Automated Claude Code task runner. Executes tasks from implementation plans in a loop until all tasks are complete.

## User Capabilities

### Running Tasks
- Users can run `hydra` to start automated task execution
- Users can run `hydra <plan>` to run with an implementation plan injected
- Users can specify maximum iterations with `--max N`
- Users can preview configuration with `--dry-run` without executing
- Users can enable debug output with `--verbose`
- Users can override the prompt file with `--prompt <path>`

### Implementation Plan
- Users can provide an optional first positional argument as a path to an implementation plan
- When provided, the plan content is appended to the prompt with a `## Implementation Plan` header
- If the plan file path is provided but the file doesn't exist, hydra exits with a helpful error

### Project Setup
- Users can run `hydra init` to create a `.hydra/` directory in their project
- Users can run `hydra --install` to install the binary to `~/.local/bin`

### Prompt Configuration
- Users can create `~/.hydra/default-prompt.md` as a global fallback
- Users can create `./prompt.md` in the project root
- Users can create `./.hydra/prompt.md` for project-specific prompts
- Users can override any prompt with the `--prompt` flag

### Stopping Execution
- Users can press Ctrl+C once for graceful termination (kills Claude, finishes iteration)
- Users can press Ctrl+C twice for immediate force quit
- Users can press Ctrl+D as equivalent to Ctrl+C
- Users can send SIGTERM for graceful shutdown after current iteration
- Users can create `.hydra-stop` file to stop after current iteration

### Logging
- Users can find session logs in `.hydra/logs/hydra-YYYYMMDD-HHMMSS.log`

## Constraints

### CLI Signature
```
hydra [PLAN] [OPTIONS]      # Run task loop (plan is optional)
hydra init                  # Initialize .hydra/ directory
hydra --install             # Install to ~/.local/bin
```

### Options
- `--prompt <path>`, `-p`: Override system prompt file
- `--max <N>`, `-m`: Maximum iterations (default: 10)
- `--dry-run`: Preview configuration without executing
- `--verbose`, `-v`: Enable debug output

### Prompt Resolution Priority
1. `--prompt <path>` (CLI override, highest)
2. `./.hydra/prompt.md` (project-specific)
3. `./prompt.md` (current directory)
4. `~/.hydra/default-prompt.md` (global fallback, lowest)

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

### Exit Codes
- `0`: Success (all tasks complete, max iterations reached, or dry-run)
- `1`: Stopped (user interrupt, SIGTERM, or stop file)
- `2`: Error (no prompt file found, or plan file not found)

### Configuration Defaults
- Max iterations: 10
- Verbose: false
- Stop file: `.hydra-stop`

### Directory Structure
```
~/.hydra/                    # Global config (auto-created)
├── config.toml              # Global defaults
└── default-prompt.md        # Fallback prompt template

./.hydra/                    # Per-project (auto-created on first run)
├── logs/                    # Session logs
└── prompt.md                # Project-specific prompt (optional)
```

### Automatic Behaviors
- `.hydra/` directory is auto-created on first run
- `.hydra/` is auto-added to `.gitignore` if not present
- `~/.hydra/default-prompt.md` is auto-created with template if no prompt found

## Architecture

### Components
- **Rust CLI**: Argument parsing (clap), config management (TOML), prompt resolution
- **Native PTY manager**: Uses `portable-pty` crate for cross-platform PTY allocation
- **Terminal I/O**: Uses `crossterm` for raw mode input handling and keyboard events
- **Signal handling**: SIGINT/SIGTERM with child process group management

### Config File (`~/.hydra/config.toml`)
```toml
max_iterations = 10
verbose = false
stop_file = ".hydra-stop"
```

## Related specs

None yet.

### Interactive Mode
- Users can type while Claude is running (input forwarded to PTY)
- Users can use arrow keys, function keys, and special keys
- Users see Claude's TUI output streamed in real-time

## Source

- [src/main.rs](../src/main.rs) - Entry point and CLI setup
- [src/runner.rs](../src/runner.rs) - Main iteration loop
- [src/pty.rs](../src/pty.rs) - PTY manager for Claude execution
- [src/signal.rs](../src/signal.rs) - Signal handling and child process management
- [src/config.rs](../src/config.rs) - Configuration loading
- [src/prompt.rs](../src/prompt.rs) - Prompt resolution
