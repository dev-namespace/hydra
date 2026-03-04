# Parallel Progress Tracking Implementation Plan

## Summary

Add resume capability to the `/hydra` parallel execution skill. Track completed plans in a `.hydra-parallel-progress` JSONL dotfile inside the plan folder so interrupted runs can pick up where they left off.

## Tasks

- [x] Update the Discovery section of `~/.claude/skills/hydra/SKILL.md` to check for `.hydra-parallel-progress`
  - After globbing `*.md` files, read the progress file if it exists using Bash `cat`
  - Parse each JSONL line to extract completed plan filenames
  - Filter completed plans out of the queue
  - Print resume status showing skipped plans and remaining plans
  - If all plans are already complete, show the summary from the progress file and stop
  + ([spec: Resuming Interrupted Runs](../specs/parallel-execution.md#resuming-interrupted-runs))
- [x] Update the Execution section of `SKILL.md` to write progress after each plan completes
  - After recording a plan's result and printing the progress line, append a JSONL line to the progress file using Bash `echo '{"plan":"name.md","status":"PASS","exit":0}' >> <folder>/.hydra-parallel-progress`
  + ([spec: Progress File](../specs/parallel-execution.md#progress-file))
- [x] Update the Final Summary section of `SKILL.md` to include resumed plans in the table
  - The summary table should show ALL plans (both previously completed from the progress file and newly completed)
  - Mark previously-completed plans with a note like `(resumed)` in the table
  + ([spec: Monitoring Progress](../specs/parallel-execution.md#monitoring-progress))

## Verification

- [ ] Run `/hydra plans/test-parallel/ --parallel 2`, let it complete, verify `.hydra-parallel-progress` exists in `plans/test-parallel/`
- [ ] Run again — verify all plans are skipped and summary shows immediately
- [ ] Delete the progress file, run again — verify full execution from scratch
