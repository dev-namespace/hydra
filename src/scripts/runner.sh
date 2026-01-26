#!/usr/bin/env bash
#
# runner.sh - PTY wrapper for Claude Code execution
#
# This script is embedded in the hydra binary and extracted at runtime.
# It uses script(1) to allocate a real PTY so Claude renders its TUI properly.
#
# Arguments:
#   $1 - Path to the combined prompt file
#   $2 - Path to output file for signal detection
#
# Exit codes:
#   0 - Claude process completed normally
#   1 - Error
#

set -euo pipefail

PROMPT_FILE="$1"
OUTPUT_FILE="$2"

# Use script(1) to allocate a real PTY so Claude renders its TUI
# --dangerously-skip-permissions makes Claude run autonomously without user prompts
if [[ "$(uname)" == "Darwin" ]]; then
    # macOS (BSD script): script -e -q outfile command args...
    script -e -q "$OUTPUT_FILE" claude --dangerously-skip-permissions "$PROMPT_FILE"
else
    # Linux (GNU script): script -e -q -c "command" outfile
    script -e -q -c "claude --dangerously-skip-permissions $PROMPT_FILE" "$OUTPUT_FILE"
fi
