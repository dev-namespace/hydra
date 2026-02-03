#!/bin/bash
# Test script for hydra iteration loop and terminal handling
# Tests: 1) Multiple iterations work (TASK_COMPLETE continues to next)
#        2) Terminal doesn't freeze after exit
# Run this in a NEW terminal window

set -e

cd "$(dirname "$0")"

# Create a temporary test directory
TEST_DIR=$(mktemp -d)
echo "Test directory: $TEST_DIR"

# Create a minimal prompt
cat > "$TEST_DIR/prompt.md" << 'EOF'
You are a test assistant. Look at the plan and output the appropriate stop signal.
- If there are incomplete tasks, output ###TASK_COMPLETE###
- If all tasks are marked [x], output ###ALL_TASKS_COMPLETE###
EOF

# Create a test plan with 2 tasks to test iteration continuation
cat > "$TEST_DIR/plan.md" << 'EOF'
# Test Plan

## Task 1: First task
- [ ] Mark this task complete and output ###TASK_COMPLETE###

## Task 2: Second task
- [ ] Mark this task complete and output ###ALL_TASKS_COMPLETE###
EOF

echo ""
echo "Running hydra with 2-task plan..."
echo "Expected: Iteration 1 completes Task 1, Iteration 2 completes Task 2"
echo "If hydra stops after iteration 1, the bug is NOT fixed."
echo "If hydra runs both iterations, the fix works!"
echo ""
echo "Press Enter to start test..."
read

# Run hydra with the test plan (max 3 to allow both iterations)
./target/release/hydra "$TEST_DIR/plan.md" --prompt "$TEST_DIR/prompt.md" --max 3

echo ""
echo "========================================="
echo "Test complete!"
echo "- If hydra ran 2 iterations: iteration bug is FIXED"
echo "- If you can type normally: terminal fix is WORKING"
echo "========================================="
echo ""
echo "Try typing something to confirm terminal is responsive:"
read -p "> " user_input
echo "You typed: $user_input"

# Cleanup
rm -rf "$TEST_DIR"
echo "Test files cleaned up."
