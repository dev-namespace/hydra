# Fix UTF-8 String Slicing Panic Implementation Plan

## Summary

Fix panic in `src/pty.rs:249` where `signal_accumulator.drain()` fails because byte offset may land in the middle of a multi-byte UTF-8 character. The fix ensures we drain at a valid character boundary.

## Tasks

- [ ] Fix the UTF-8 boundary issue in `src/pty.rs` at lines 247-250 by finding a valid character boundary before draining
- [ ] Add a unit test for the UTF-8 boundary handling with multi-byte characters
- [ ] Build and verify the fix works with `cargo build && cargo test`
