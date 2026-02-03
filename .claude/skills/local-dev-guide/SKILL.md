---
name: local-dev-guide
description: Quick reference for local development commands
disable-model-invocation: true
---

## Build

- `cargo build` - Debug build
- `cargo build --release` - Release build

## Run

- `./target/debug/hydra --help` - Run debug binary
- `./target/release/hydra --help` - Run release binary
- `./target/release/hydra --dry-run` - Preview config without executing

## Test

- `cargo test` - Run all tests (47 tests)

## Install

- `./target/release/hydra --install` - Install to ~/.local/bin
