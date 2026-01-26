# Hydra

Automated Claude Code task runner. Executes tasks from implementation plans in a loop until all tasks are complete.

## User Capabilities

### Running Tasks
- Users can run `hydra` to start automated task execution
- Users can specify maximum iterations with `--max N`
- Users can preview configuration with `--dry-run` without executing
- Users can enable debug output with `--verbose`
- Users can override the prompt file with `--prompt <path>`

### Project Setup
- Users can run `hydra init` to create a `.hydra/` directory in their project
- Users can run `hydra --install` to install the binary to `~/.local/bin`

### Prompt Configuration
- Users can create `~/.hydra/default-prompt.md` as a global fallback
- Users can create `./prompt.md` in the project root
- Users can create `./.hydra/prompt.md` for project-specific prompts
- Users can override any prompt with the `--prompt` flag

### Stopping Execution
- Users can press Ctrl+C (SIGINT) for immediate termination
- Users can send SIGTERM for graceful shutdown after current iteration
- Users can create `.hydra-stop` file to stop after current iteration

### Logging
- Users can find session logs in `.hydra/logs/hydra-YYYYMMDD-HHMMSS.log`

## Constraints

### Prompt Resolution Priority
1. `--prompt <path>` (CLI override, highest)
2. `./.hydra/prompt.md` (project-specific)
3. `./prompt.md` (current directory)
4. `~/.hydra/default-prompt.md` (global fallback, lowest)

### Stop Signals
- Claude must output `###TASK_COMPLETE###` when one task is done but more remain
- Claude must output `###ALL_TASKS_COMPLETE###` when all tasks are finished
- Hydra monitors output and terminates the iteration upon signal detection

### Exit Codes
- `0`: Success (all tasks complete, max iterations reached, or dry-run)
- `1`: Stopped (user interrupt, SIGTERM, or stop file)
- `2`: Error (no prompt file found at any priority level)

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
- **Rust CLI wrapper**: Argument parsing (clap), config management (TOML), prompt resolution
- **Embedded bash script**: PTY allocation via `script(1)` for Claude TUI streaming
- **Signal handling**: SIGINT for immediate stop, SIGTERM for graceful shutdown

### Config File (`~/.hydra/config.toml`)
```toml
max_iterations = 10
verbose = false
stop_file = ".hydra-stop"
```

## Related specs

None yet.

## Source

- [hydra.sh](../hydra.sh) - Original bash implementation (reference)
