# Headless Mode Implementation Plan

## Summary

Add `--headless` flag to hydra that uses `claude -p` (pipe mode) instead of PTY-based interactive execution. Each iteration spawns a fresh `claude -p --dangerously-skip-permissions --output-format stream-json` process, pipes the prompt via stdin, and parses stream-json text deltas for stop signals. Output goes to the log file, not stdout. The parallel skill will use `--headless` internally to avoid PTY-in-non-terminal issues.

## Tasks

- [ ] Add `--headless` CLI flag and create `headless.rs` module skeleton
  - Add `pub headless: bool` to `Cli` struct in `src/cli.rs` with clap attribute (`--headless`, long only, no short form, default false)
  - Create `src/headless.rs` with `HeadlessRunner` struct that holds `Config`, `ResolvedPrompt`, `Arc<AtomicBool>` stop flag, and optional `SessionLogger`
  - Add `pub mod headless;` to `src/main.rs`
  - Add routing in `main.rs`: when `cli.headless` is set, construct and run `HeadlessRunner` instead of the PTY-based `Runner`
  - Both paths share the same prompt resolution, plan injection, scratchpad injection, and config merging — only the execution differs
  + ([spec: CLI Flag](../specs/headless-mode.md#cli-flag))
  + ([spec: What Headless Mode Skips](../specs/headless-mode.md#what-headless-mode-skips))

- [ ] Implement stream-json text parsing and stop signal detection
  - Add a `StreamJsonParser` that reads newline-delimited JSON from a `BufReader<ChildStdout>`
  - Each line is a JSON object — filter for `{"type": "stream_event"}` where `event.delta.type == "text_delta"`
  - Extract `event.delta.text` field and append to a `String` text accumulator
  - After each append, scan the accumulator for `###ALL_TASKS_COMPLETE###` and `###TASK_COMPLETE###` (plain string search, no ANSI stripping needed)
  - Return the appropriate `IterationResult` variant when a signal is found
  - Also write extracted text to the log file writer as it arrives (for session logging)
  - Use `serde_json::Value` for flexible JSON parsing (avoid rigid struct definitions since stream-json schema may evolve)
  + ([spec: Stream-JSON Parsing](../specs/headless-mode.md#stream-json-parsing))
  + ([spec: Stop Signals](../specs/hydra.md#stop-signals))

- [ ] Implement headless iteration loop in `HeadlessRunner::run()`
  - Single iteration (`run_iteration`): create combined prompt (iteration instructions + user prompt), spawn `claude -p --dangerously-skip-permissions --output-format stream-json --verbose` as a `Command` with `stdin(Stdio::piped())` and `stdout(Stdio::piped())`, write prompt to stdin then drop it, read stdout through `StreamJsonParser`, return `IterationResult`
  - Timeout: spawn a timer thread or use the existing timeout pattern — if exceeded, SIGTERM the child process group (reuse `signal::kill_child_process_group()`), wait briefly, SIGKILL if needed
  - Multi-iteration loop (`run`): mirror `runner.rs` loop — check stop file and graceful shutdown flag between iterations, handle `AllComplete` (exit success), `TaskComplete`/`NoSignal`/`Timeout` (continue), `Terminated` (exit). Return `RunResult` with iteration count
  - Signal handling: call `signal::set_child_pid()` after spawn, `signal::clear_child_pid()` after exit — same pattern as PTY mode
  - Stdout status lines: print minimal progress (`[hydra] Iteration N/M...`, `[hydra] TASK_COMPLETE detected`, `[hydra] ALL_TASKS_COMPLETE — done`)
  + ([spec: Iteration Model](../specs/headless-mode.md#iteration-model))
  + ([spec: Timeout Handling](../specs/headless-mode.md#timeout-handling))
  + ([spec: Signal Handling](../specs/headless-mode.md#signal-handling))

- [ ] Update parallel skill to use `--headless` flag
  - In `~/.claude/skills/hydra/SKILL.md`, change the subagent prompt from `hydra <plan> --no-review` to `hydra <plan> --headless --no-review`
  + ([spec: Parallel Skill Integration](../specs/headless-mode.md#parallel-skill-integration))

## Verification

- [ ] `hydra plan.md --headless --dry-run` shows headless mode in config preview
- [ ] `hydra plan.md --headless` executes tasks using `claude -p`, not PTY
- [ ] Stop signals are detected correctly from stream-json output
- [ ] Session log contains extracted text content (not raw JSON)
- [ ] Ctrl+C gracefully terminates the headless claude process
- [ ] Timeout kills the claude process and continues to next iteration
- [ ] `cargo clippy` and `cargo test` pass
- [ ] Parallel skill `/hydra plans/` uses `--headless` internally
