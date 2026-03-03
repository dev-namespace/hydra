#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."
HYDRA="./target/debug/hydra"
PLANS="plans/test-parallel"
PASS=0
FAIL=0

green() { printf "\033[32m%s\033[0m\n" "$1"; }
red()   { printf "\033[31m%s\033[0m\n" "$1"; }
bold()  { printf "\033[1m%s\033[0m\n" "$1"; }

assert_exit() {
    local label="$1" expected="$2" actual="$3"
    if [ "$actual" -eq "$expected" ]; then
        green "  PASS: $label (exit $actual)"
        PASS=$((PASS + 1))
    else
        red "  FAIL: $label (expected exit $expected, got $actual)"
        FAIL=$((FAIL + 1))
    fi
}

# --- Build ---
bold "Building hydra..."
cargo build --quiet 2>&1
green "Build OK"
echo

# --- Verify plan files exist ---
bold "Checking test plan files..."
for plan in echo-hello.md read-cargo.md list-specs.md intentional-fail.md; do
    if [ -f "$PLANS/$plan" ]; then
        green "  EXISTS: $plan"
    else
        red "  MISSING: $plan"
        FAIL=$((FAIL + 1))
    fi
done
echo

# --- Dry-run tests (no Claude needed) ---
bold "Dry-run tests (--no-review flag acceptance)..."
for plan in echo-hello.md read-cargo.md list-specs.md intentional-fail.md; do
    set +e
    $HYDRA "$PLANS/$plan" --no-review --dry-run > /dev/null 2>&1
    code=$?
    set -e
    assert_exit "dry-run $plan" 0 "$code"
done
echo

# --- Verify intentional-fail plan content ---
bold "Checking intentional-fail.md creates .hydra-stop..."
if grep -q "\.hydra-stop" "$PLANS/intentional-fail.md"; then
    green "  PASS: plan references .hydra-stop"
    PASS=$((PASS + 1))
else
    red "  FAIL: plan does not reference .hydra-stop"
    FAIL=$((FAIL + 1))
fi
echo

# --- Live test: intentional-fail produces exit 1 ---
# This spawns a real Claude session — requires claude CLI available.
# Uses --max 2 so hydra has a chance to check the stop file on iteration 2.
# Uses --timeout 60 as a safety net.
if [ "${LIVE:-0}" = "1" ]; then
    bold "Live test: intentional-fail exits non-zero..."
    rm -f .hydra-stop
    set +e
    $HYDRA "$PLANS/intentional-fail.md" --no-review --max 2 --timeout 60
    code=$?
    set -e
    assert_exit "intentional-fail exits 1" 1 "$code"
    rm -f .hydra-stop
    echo
else
    bold "Skipping live test (set LIVE=1 to enable)"
    echo
fi

# --- Summary ---
bold "Results: $PASS passed, $FAIL failed"
if [ "$FAIL" -gt 0 ]; then
    red "SOME TESTS FAILED"
    exit 1
else
    green "ALL TESTS PASSED"
    exit 0
fi
