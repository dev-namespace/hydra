# Hydra

Automated Claude Code task runner. Executes tasks from implementation plans in a loop until all tasks are complete.

## How It Works

Hydra spawns Claude Code in a PTY (pseudo-terminal) and monitors its output for completion signals:

1. **Load prompt** - Resolves a system prompt from multiple possible locations
2. **Inject plan** (optional) - Appends an implementation plan reference to the prompt
3. **Run Claude** - Spawns `claude -p <prompt>` in a PTY with full terminal emulation
4. **Monitor output** - Watches for stop signals (`###TASK_COMPLETE###` or `###ALL_TASKS_COMPLETE###`)
5. **Loop** - Repeats until all tasks complete, max iterations reached, or user interrupts

### Stop Signals

Claude must output these signals to control the loop:

- `###TASK_COMPLETE###` - One task done, more remain. Hydra starts a new iteration.
- `###ALL_TASKS_COMPLETE###` - All tasks finished. Hydra exits successfully.

If no signal is received within the timeout (default: 20 minutes), Hydra terminates the iteration and starts the next one.

## Installation

### From Source

```bash
# Build release binary
cargo build --release

# Install to ~/.local/bin
./target/release/hydra --install
```

### Manual Install

Copy the release binary to a directory in your PATH:

```bash
cp target/release/hydra ~/.local/bin/
```

On macOS, the `--install` command automatically re-signs the binary with an ad-hoc signature for Gatekeeper.

## Building

### Debug Build

```bash
cargo build
./target/debug/hydra --help
```

### Release Build

```bash
cargo build --release
./target/release/hydra --help
```

The release binary is at `target/release/hydra`.

## Usage

```
hydra [PLAN] [OPTIONS]      # Run task loop
hydra init                  # Initialize .hydra/ directory
hydra --install             # Install to ~/.local/bin
```

### Examples

```bash
# Run with default prompt
hydra

# Run with an implementation plan
hydra ./plan.md

# Run with custom prompt and max 5 iterations
hydra --prompt ./my-prompt.md --max 5

# Preview configuration without executing
hydra --dry-run

# Run with 30-minute timeout per iteration
hydra --timeout 1800
```

### Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--prompt <path>` | `-p` | Override system prompt file | (resolved) |
| `--max <N>` | `-m` | Maximum iterations | 10 |
| `--timeout <N>` | `-t` | Iteration timeout (seconds) | 1200 |
| `--dry-run` | | Preview config without executing | false |
| `--verbose` | `-v` | Enable debug output | false |

## Prompt Configuration

Hydra resolves prompts in this priority order (highest first):

1. `--prompt <path>` - CLI override
2. `./.hydra/prompt.md` - Project-specific
3. `./prompt.md` - Current directory
4. `~/.hydra/default-prompt.md` - Global fallback

On first run, if no prompt is found, Hydra creates a template at `~/.hydra/default-prompt.md`.

### Writing Prompts

Your prompt should instruct Claude on:
- What tasks to work on
- How to signal completion (include the stop signals)
- Any project-specific context

Example prompt template:

```markdown
You are working on implementing features for this project.

Work through tasks one at a time. After completing each task:
- If more tasks remain, output: ###TASK_COMPLETE###
- If all tasks are done, output: ###ALL_TASKS_COMPLETE###

Focus on quality. Run tests after changes. Commit your work.
```

## Directory Structure

```
~/.hydra/                    # Global config (auto-created)
├── config.toml              # Global defaults
└── default-prompt.md        # Fallback prompt template

./.hydra/                    # Per-project (auto-created on first run)
├── logs/                    # Session logs
│   └── hydra-YYYYMMDD-HHMMSS.log
└── prompt.md                # Project-specific prompt (optional)
```

## Stopping Execution

| Method | Behavior |
|--------|----------|
| `Ctrl+C` (once) | Graceful: kills Claude, finishes iteration |
| `Ctrl+C` (twice) | Force quit immediately |
| `Ctrl+D` | Same as `Ctrl+C` |
| `SIGTERM` | Graceful shutdown after current iteration |
| Create `.hydra-stop` | Stop after current iteration completes |

## Configuration File

Global defaults in `~/.hydra/config.toml`:

```toml
max_iterations = 10
timeout_seconds = 1200
verbose = false
stop_file = ".hydra-stop"
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success (all tasks complete, max iterations, or dry-run) |
| 1 | Stopped (user interrupt, SIGTERM, or stop file) |
| 2 | Error (no prompt found, plan file missing) |

## Interactive Mode

While Claude is running, you can:
- Type to send input to Claude (forwarded to PTY)
- Use arrow keys, function keys, and special keys
- See Claude's TUI output streamed in real-time

## License

MIT
