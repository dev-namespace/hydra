#!/usr/bin/env bash
#
# hydra.sh - Automated Claude Code Task Runner
#
# Runs Claude Code in a loop, executing tasks from prompt.md until
# all tasks in the prompt are complete.
#

set -euo pipefail

# Configuration
TASK_COMPLETE_SIGNAL="###TASK_COMPLETE###"
ALL_COMPLETE_SIGNAL="###ALL_TASKS_COMPLETE###"
MAX_ITERATIONS=10
DEFAULT_PROMPT_FILE="./prompt.md"
STOP_FILE=".hydra-stop"

# Global variable to track script process PID for signal handling
SCRIPT_PID=""

# Global flag to track graceful shutdown request via SIGTERM
SIGTERM_RECEIVED=0

# Dry-run mode flag (0 = normal, 1 = dry-run)
DRY_RUN=0

# Verbose mode flag (0 = normal, 1 = verbose)
VERBOSE=0

# Session log file path (initialized in init_log_file)
LOG_FILE=""

# Show usage information and exit
show_help() {
    cat << 'EOF'
hydra.sh - Automated Claude Code Task Runner

USAGE:
    hydra.sh [OPTIONS] [prompt_file]

DESCRIPTION:
    Runs Claude Code in a loop, executing tasks from an implementation plan
    until all tasks are complete or maximum iterations are reached.

    Claude controls task selection - it picks the highest-leverage task
    from whatever implementation plan is referenced in the prompt file.

OPTIONS:
    -h, --help       Show this help message and exit
    -m, --max N      Set maximum iterations (default: 10)
    -n, --dry-run    Show configuration without running Claude
    -v, --verbose    Enable verbose debug output

ARGUMENTS:
    prompt_file      Path to the prompt file (default: ./prompt.md)

EXAMPLES:
    # Run with default settings (prompt.md, max 10 iterations)
    hydra.sh

    # Run with a custom prompt file
    hydra.sh tasks.md

    # Run with increased iteration limit
    hydra.sh --max 20 prompt.md

    # Preview configuration without executing
    hydra.sh --dry-run

    # Run with verbose output for debugging
    hydra.sh --verbose

    # Combine options
    hydra.sh -v -m 5 my-tasks.md

STOP SIGNALS:
    hydra monitors Claude's output for two stop signals:

        ###TASK_COMPLETE###
        Claude finished one task, more tasks remain.
        hydra terminates the current iteration and starts a new one.

        ###ALL_TASKS_COMPLETE###
        Claude finished all tasks in the implementation plan.
        hydra terminates the session with exit code 0.

    Claude decides which signal to use based on the implementation plan.

STOPPING hydra:
    There are three ways to stop hydra:

    1. Ctrl+C (SIGINT)
       Immediately terminates the current iteration and exits.

    2. SIGTERM signal
       Allows the current iteration to complete, then exits gracefully.
       Example: kill -TERM <hydra_pid>

    3. Stop file (.hydra-stop)
       Create a file named .hydra-stop in the working directory.
       hydra checks for this file before each iteration and exits
       gracefully if found. The file is automatically deleted.
       Example: touch .hydra-stop

EXIT CODES:
    0    Success (all tasks complete, max iterations reached, or dry-run)
    1    Stopped (user interrupt, SIGTERM, or stop file)
    2    Error (prompt file not found)

EOF
    exit 0
}

# Print verbose debug message
# Only outputs when VERBOSE=1
# Usage: verbose_log "message"
verbose_log() {
    if [[ "$VERBOSE" -eq 1 ]]; then
        echo "[hydra:debug] $1"
    fi
}

# Initialize log file with timestamped name
# Creates log file: hydra-YYYYMMDD-HHMMSS.log
init_log_file() {
    local timestamp
    timestamp=$(date '+%Y%m%d-%H%M%S')
    LOG_FILE="hydra-${timestamp}.log"
    touch "$LOG_FILE"
}

# Write a message to the log file with timestamp
# Usage: log_message "message"
log_message() {
    local message="$1"
    local timestamp
    timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    echo "[${timestamp}] ${message}" >> "$LOG_FILE"
}

# Log session start information
# Logs start time, prompt file, and settings
log_session_start() {
    local start_time
    start_time=$(date '+%Y-%m-%d %H:%M:%S')
    {
        echo "================================================================================"
        echo "hydra SESSION LOG"
        echo "================================================================================"
        echo ""
        echo "Start time: ${start_time}"
        echo "Prompt file: ${PROMPT_FILE}"
        echo "Max iterations: ${MAX_ITERATIONS}"
        echo ""
        echo "================================================================================"
        echo ""
    } >> "$LOG_FILE"
}

# Log final session status
# Usage: log_final_status "status_message"
log_final_status() {
    local status="$1"
    local end_time
    end_time=$(date '+%Y-%m-%d %H:%M:%S')
    {
        echo ""
        echo "================================================================================"
        echo "SESSION END"
        echo "================================================================================"
        echo ""
        echo "End time: ${end_time}"
        echo "Final status: ${status}"
        echo ""
        echo "================================================================================"
    } >> "$LOG_FILE"
}

# Handle Ctrl+C (SIGINT) signal
# Terminates Claude process if running and exits with code 1
# shellcheck disable=SC2329  # Function is invoked via trap
handle_sigint() {
    echo ""
    echo "Interrupted by user"
    if [[ -n "$LOG_FILE" ]] && [[ -f "$LOG_FILE" ]]; then
        log_final_status "stopped (interrupted by user)"
    fi
    if [[ -n "$SCRIPT_PID" ]] && kill -0 "$SCRIPT_PID" 2>/dev/null; then
        kill -TERM "$SCRIPT_PID" 2>/dev/null || true
    fi
    exit 1
}

# Handle SIGTERM signal for graceful shutdown
# Allows current iteration to complete, then exits with code 1
# shellcheck disable=SC2329  # Function is invoked via trap
handle_sigterm() {
    echo ""
    echo "SIGTERM received, finishing current iteration"
    SIGTERM_RECEIVED=1
}

# Set up signal traps
trap handle_sigint SIGINT
trap handle_sigterm SIGTERM

# Parse command-line arguments
# Usage: hydra.sh [-h|--help] [-m|--max N] [-n|--dry-run] [-v|--verbose] [prompt_file]
# Options:
#   -h, --help     Show usage information
#   -m, --max N    Set maximum iterations (default: 10)
#   -n, --dry-run  Test loop logic without running Claude
#   -v, --verbose  Enable verbose debug output
# If prompt_file is not provided, defaults to ./prompt.md
parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            -h|--help)
                show_help
                ;;
            -m|--max)
                if [[ $# -lt 2 ]]; then
                    echo "[hydra] Error: --max requires a numeric argument" >&2
                    exit 1
                fi
                MAX_ITERATIONS="$2"
                shift 2
                ;;
            -n|--dry-run)
                DRY_RUN=1
                shift
                ;;
            -v|--verbose)
                VERBOSE=1
                shift
                ;;
            -*)
                echo "[hydra] Error: Unknown option: $1" >&2
                exit 1
                ;;
            *)
                PROMPT_FILE="$1"
                shift
                ;;
        esac
    done

    # Set default prompt file if not specified
    if [[ -z "${PROMPT_FILE:-}" ]]; then
        PROMPT_FILE="$DEFAULT_PROMPT_FILE"
    fi

    # Output parsed arguments in verbose mode
    verbose_log "Parsed arguments:"
    verbose_log "  PROMPT_FILE=$PROMPT_FILE"
    verbose_log "  MAX_ITERATIONS=$MAX_ITERATIONS"
    verbose_log "  DRY_RUN=$DRY_RUN"
    verbose_log "  VERBOSE=$VERBOSE"

    # Confirm signal trap setup in verbose mode
    verbose_log "Signal traps configured:"
    verbose_log "  SIGINT (Ctrl+C) -> handle_sigint (immediate termination)"
    verbose_log "  SIGTERM -> handle_sigterm (graceful shutdown)"
}

# Iteration instructions prepended to prompt
read -r -d '' ITERATION_INSTRUCTIONS << 'EOF' || true
╔══════════════════════════════════════════════════════════════════════════════╗
║                           hydra ITERATION INSTRUCTIONS                       ║
╚══════════════════════════════════════════════════════════════════════════════╝

You are running inside hydra, an automated task runner.

YOUR TASK:
1. Review the implementation plan referenced in the prompt below
2. Pick the highest-leverage task that is not yet complete
3. Complete that ONE task thoroughly
4. Mark the task as completed in the plan
4. Signal completion with the appropriate stop sequence

STOP SEQUENCES (output on its own line when done):

  ###TASK_COMPLETE###
  Use this when you have completed the current task but MORE tasks remain.
  hydra will start a new iteration for the next task.

  ###ALL_TASKS_COMPLETE###
  Use this when ALL tasks in the implementation plan are complete.
  hydra will end the session.

IMPORTANT:
- Complete only ONE task per iteration
- Always output exactly one of the two stop sequences when finished
- Mark the task as completed in the plan when finished
- Work AUTONOMOUSLY - do NOT ask the user for input or confirmation
- Make decisions yourself and proceed with the implementation
- Do NOT use AskUserQuestion or similar tools that require user input

────────────────────────────────────────────────────────────────────────────────

EOF

# Check if prompt file exists
# Exit code 2 if file doesn't exist (per acceptance criteria)
check_prompt_file() {
    if [[ ! -f "$PROMPT_FILE" ]]; then
        echo "[hydra] Error: $PROMPT_FILE not found" >&2
        exit 2
    fi
}

# Check for stop file and handle graceful exit
# Returns 0 if stop file exists (should exit), 1 otherwise
check_stop_file() {
    verbose_log "Checking for stop file: $STOP_FILE"
    if [[ -f "$STOP_FILE" ]]; then
        verbose_log "Stop file found, initiating graceful exit"
        echo "[hydra] Stop file detected, exiting gracefully"
        rm -f "$STOP_FILE"
        log_final_status "stopped (stop file detected)"
        exit 1
    fi
}


# Global variable to track which signal was detected
# Values: "task_complete", "all_complete", "none"
DETECTED_SIGNAL="none"

# Run a single iteration of Claude Code
# Sets DETECTED_SIGNAL to indicate which stop sequence was found:
#   "task_complete" - ###TASK_COMPLETE### detected
#   "all_complete"  - ###ALL_TASKS_COMPLETE### detected
#   "none"          - No signal detected (process ended without signal)
run_iteration() {
    local iteration=$1
    local combined_prompt
    local temp_prompt
    local output_file

    # Reset signal detection
    DETECTED_SIGNAL="none"

    echo "[hydra] Run #${iteration} starting..."
    log_message "Iteration ${iteration} started"

    # Combine iteration instructions with prompt content
    combined_prompt="${ITERATION_INSTRUCTIONS}
$(cat "$PROMPT_FILE")"

    # Create a temporary file for the combined prompt
    temp_prompt=$(mktemp)
    printf '%s' "$combined_prompt" > "$temp_prompt"

    # Create a temporary file to capture output for stop sequence detection
    output_file=$(mktemp)

    # Use script(1) to allocate a real PTY so Claude renders its TUI
    # This displays output directly to terminal in real-time
    # and captures everything to a file for signal detection and logging
    # --dangerously-skip-permissions makes Claude run autonomously without user prompts
    if [[ "$(uname)" == "Darwin" ]]; then
        # macOS (BSD script): script -e -q outfile command args...
        script -e -q "$output_file" claude --dangerously-skip-permissions "$temp_prompt" &
    else
        # Linux (GNU script): script -e -q -c "command" outfile
        script -e -q -c "claude --dangerously-skip-permissions $temp_prompt" "$output_file" &
    fi
    SCRIPT_PID=$!

    verbose_log "Monitoring for stop signals: $TASK_COMPLETE_SIGNAL and $ALL_COMPLETE_SIGNAL"
    verbose_log "Script process started with PID: $SCRIPT_PID"

    # Monitor the output file for stop sequences in background
    # This runs in parallel and doesn't block the streaming
    while kill -0 "$SCRIPT_PID" 2>/dev/null; do
        # Check for ALL_COMPLETE first (more specific)
        if grep -q "$ALL_COMPLETE_SIGNAL" "$output_file" 2>/dev/null; then
            DETECTED_SIGNAL="all_complete"
            verbose_log "All-complete signal found in output"
            echo ""
            echo "[hydra] All tasks complete signal detected, terminating Claude process..."
            verbose_log "Sending SIGTERM to script process (PID: $SCRIPT_PID)"
            kill -TERM "$SCRIPT_PID" 2>/dev/null || true
            break
        fi
        # Check for TASK_COMPLETE
        if grep -q "$TASK_COMPLETE_SIGNAL" "$output_file" 2>/dev/null; then
            DETECTED_SIGNAL="task_complete"
            verbose_log "Task-complete signal found in output"
            echo ""
            echo "[hydra] Task complete signal detected, terminating Claude process..."
            verbose_log "Sending SIGTERM to script process (PID: $SCRIPT_PID)"
            kill -TERM "$SCRIPT_PID" 2>/dev/null || true
            break
        fi
        # Small sleep to avoid busy-waiting, but fast enough for responsive detection
        sleep 0.1
    done

    # Wait for script to finish (either naturally or from our kill)
    wait "$SCRIPT_PID" 2>/dev/null || true

    # Clear the PID after process completes
    SCRIPT_PID=""

    # Append captured output to the session log file
    cat "$output_file" >> "$LOG_FILE" 2>/dev/null || true

    # Clean up
    rm -f "$temp_prompt" "$output_file"

    echo "[hydra] Run #${iteration} complete"
    log_message "Iteration ${iteration} ended"
}


# Display iteration status header
# Shows current iteration and max iterations
display_iteration_header() {
    local iteration=$1
    local max_iterations=$2

    echo ""
    echo "=== Iteration ${iteration}/${max_iterations} ==="
    echo ""
}

# Run dry-run simulation
# Shows configuration and explains Claude-controlled task flow
run_dry_run() {
    echo ""
    echo "╔══════════════════════════════════════════════════════════════════════════════╗"
    echo "║                              DRY-RUN MODE                                    ║"
    echo "╚══════════════════════════════════════════════════════════════════════════════╝"
    echo ""
    echo "Configuration:"
    echo "  Prompt file:     $PROMPT_FILE"
    echo "  Max iterations:  $MAX_ITERATIONS"
    echo "  Stop file:       $STOP_FILE"
    echo ""
    echo "Claude-Controlled Task Flow:"
    echo "  Claude picks the highest-leverage task from the implementation plan."
    echo "  hydra monitors for two stop signals:"
    echo ""
    echo "    ###TASK_COMPLETE###"
    echo "      → Claude finished one task, more tasks remain"
    echo "      → hydra starts next iteration"
    echo ""
    echo "    ###ALL_TASKS_COMPLETE###"
    echo "      → Claude finished all tasks"
    echo "      → hydra ends session with exit code 0"
    echo ""
    echo "Session Termination:"
    echo "  • ALL_TASKS_COMPLETE signal detected"
    echo "  • Max iterations ($MAX_ITERATIONS) reached"
    echo "  • Stop file (.hydra-stop) created"
    echo "  • SIGTERM signal received"
    echo "  • Ctrl+C pressed"
    echo ""
    echo "[Dry-run complete]"
}

# Main loop
main() {
    local iteration=1

    # Parse command-line arguments first
    parse_args "$@"

    # Check prompt file early (needed for dry-run too)
    # Set default prompt file if not already set by parse_args
    if [[ -z "${PROMPT_FILE:-}" ]]; then
        PROMPT_FILE="$DEFAULT_PROMPT_FILE"
    fi
    check_prompt_file

    # Handle dry-run mode
    if [[ "$DRY_RUN" -eq 1 ]]; then
        run_dry_run
        exit 0
    fi

    # Initialize session log file
    init_log_file

    echo "[hydra] Starting automated task runner"
    echo "[hydra] Using prompt file: $PROMPT_FILE"
    echo "[hydra] Log file: $LOG_FILE"
    echo "[hydra] Claude controls task selection from implementation plan"

    # Log session start after prompt file is validated
    log_session_start

    # Main iteration loop
    # Continues until: ALL_COMPLETE signal, max iterations, stop file, or SIGTERM
    while [[ "$iteration" -le "$MAX_ITERATIONS" ]]; do
        # Check for stop file before each iteration
        check_stop_file

        display_iteration_header "$iteration" "$MAX_ITERATIONS"
        run_iteration "$iteration"

        # Check for graceful shutdown via SIGTERM after iteration completes
        if [[ "$SIGTERM_RECEIVED" -eq 1 ]]; then
            echo "[hydra] Graceful shutdown complete"
            log_final_status "stopped (SIGTERM received)"
            exit 1
        fi

        # Check which signal was detected
        if [[ "$DETECTED_SIGNAL" == "all_complete" ]]; then
            echo "[hydra] All tasks complete! Total runs: ${iteration}"
            log_final_status "completed (all tasks done)"
            exit 0
        fi

        # task_complete or none: continue to next iteration
        ((iteration++))
    done

    # Stopped due to max iterations
    echo "[hydra] Max iterations reached"
    log_final_status "stopped (max iterations reached)"
    exit 0
}

main "$@"
