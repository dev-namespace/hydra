#!/bin/bash
# Test script for terminal freeze fix
# Run this in a NEW terminal window

set -e

cd "$(dirname "$0")"

# Create a temporary test directory
TEST_DIR=$(mktemp -d)
echo "Test directory: $TEST_DIR"

# Create a minimal prompt
cat > "$TEST_DIR/prompt.md" << 'EOF'
You are a test assistant. Simply output the stop signal immediately.
EOF

# Create a simple test plan with instant tasks
cat > "$TEST_DIR/plan.md" << 'EOF'
# Test Plan

## Task 1: Instant complete
- Just output ###TASK_COMPLETE### immediately
EOF

echo ""
echo "Running hydra with instant task..."
echo "If terminal freezes after hydra exits, the fix didn't work."
echo "If you can type normally after hydra exits, the fix works!"
echo ""
echo "Press Enter to start test..."
read

# Run hydra with the test plan
./target/release/hydra "$TEST_DIR/plan.md" --prompt "$TEST_DIR/prompt.md" --max 1

echo ""
echo "========================================="
echo "Test complete! If you can see this and type,"
echo "the terminal fix is working."
echo "========================================="
echo ""
echo "Try typing something to confirm terminal is responsive:"
read -p "> " user_input
echo "You typed: $user_input"

# Cleanup
rm -rf "$TEST_DIR"
echo "Test files cleaned up."
