# CLI Plan Injection Implementation Plan

## Summary

Restructure hydra CLI to support optional implementation plan injection. Changes the default command from `hydra run` to `hydra [PLAN] [OPTIONS]`, where PLAN is an optional first positional argument pointing to an implementation plan file. When provided, the plan content is appended to the resolved prompt with a `## Implementation Plan` header.

([spec: CLI Signature](../specs/hydra.md#cli-signature)) | ([spec: Plan Injection](../specs/hydra.md#plan-injection))

## Tasks

- [ ] Update `src/cli.rs`: Remove `Run` subcommand, add optional `plan` positional arg to top-level `Cli` struct
- [ ] Update `src/cli.rs`: Remove `effective_*` methods that handled Run subcommand merging
- [ ] Add `src/plan.rs`: Create `read_plan_file()` function that reads plan content or returns error if file not found
- [ ] Update `src/prompt.rs`: Add `inject_plan()` function that appends plan content with `## Implementation Plan` header
- [ ] Update `src/error.rs`: Add `PlanNotFound(PathBuf)` error variant with helpful message
- [ ] Update `src/main.rs`: Route based on `cli.plan` presence, call plan injection before running
- [ ] Update `src/runner.rs`: Accept combined prompt+plan content instead of just prompt (already works via ResolvedPrompt)
- [ ] Add tests for plan file reading and error handling (in plan.rs: test_read_plan_file_success, test_read_plan_file_not_found, test_read_plan_file_error_message)
- [ ] Add tests for plan injection formatting (in prompt.rs: test_inject_plan_basic, test_inject_plan_format, test_inject_plan_trims_trailing_whitespace, test_inject_plan_preserves_plan_content)
- [ ] Update `hydra --help` output to reflect new CLI signature (shows [PLAN] argument)

## Verification

- [ ] `cargo test` passes
- [ ] `hydra` runs without plan (uses prompt only)
- [ ] `hydra plan.md` injects plan content after prompt
- [ ] `hydra nonexistent.md` shows helpful error message
- [ ] `hydra plan.md --prompt custom.md` uses custom prompt + plan
- [ ] `hydra init` still works as subcommand
- [ ] `hydra --install` still works as flag
