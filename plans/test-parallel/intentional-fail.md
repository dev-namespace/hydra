# Intentional Fail Plan

## Summary

Trivial debug plan that intentionally fails to test failure reporting in the parallel orchestrator.
Creates the `.hydra-stop` file to trigger hydra's graceful stop (exit code 1).

NOTE: After running this plan, delete `.hydra-stop` from the project root.

## Tasks

- [x] Create the file `.hydra-stop` in the current working directory with the content "intentional-fail test"
- [x] Read `.hydra-stop` back to confirm it exists (this task should never be reached — hydra will stop first)
