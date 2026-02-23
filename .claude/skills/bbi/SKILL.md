---
name: bbi
description: Bump version, build release, and install hydra
disable-model-invocation: true
allowed-tools: Bash, Read, Edit
---

# Bump, Build, and Install

Perform a version bump, release build, and installation of the hydra binary.

## Steps

1. **Bump the version** in `Cargo.toml`:
   - Read the current version from `Cargo.toml`
   - Increment the patch version (e.g., 0.5.3 -> 0.5.4)
   - Update the version in `Cargo.toml`

2. **Build the release version**:
   ```bash
   cargo build --release
   ```

3. **Install using the built binary**:
   ```bash
   ./target/release/hydra --install
   ```

4. **Report the new version** to the user after successful installation.
