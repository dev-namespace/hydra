# Set Up Precommit Hooks

Your task is to configure fast, parallel precommit hooks using [prek](https://github.com/j178/prek) for this project.

## Reference

If `.claude/skills/local-dev-guide/SKILL.md` exists, read it first—it contains verified build/test commands for this project.

## Instructions

1. **Analyze the repository** to understand:
   - Programming language(s) used
   - Linting tools available (ESLint, Clippy, Ruff, golangci-lint, etc.)
   - Type checking tools (TypeScript, mypy, pyright, etc.)
   - Formatting tools (Prettier, rustfmt, Black, gofmt, etc.)
   - Test framework and typical test runtime

2. **Determine which hooks apply** to this project:
   - **Lint**: Only if a linter is configured (eslint, clippy, ruff, etc.)
   - **Type check**: Only if type checking is set up (tsc, mypy, pyright)
   - **Format check**: Only check mode (--check), no auto-fix
   - **Fast tests**: Only if unit tests run in under 2-3 seconds

3. **Create `.pre-commit-config.yaml`** at project root with parallel hooks:

```yaml
# Precommit hooks - all run in parallel via prek
# Each hook should complete in under 5 seconds
repos:
  - repo: local
    hooks:
      - id: lint
        name: lint
        entry: <lint-command>  # e.g., "cargo clippy -- -D warnings" or "npm run lint"
        language: system
        pass_filenames: false
      - id: typecheck
        name: typecheck
        entry: <typecheck-command>  # e.g., "npx tsc --noEmit" or "mypy ."
        language: system
        pass_filenames: false
      - id: format-check
        name: format-check
        entry: <format-check-command>  # e.g., "cargo fmt --check" or "npx prettier --check ."
        language: system
        pass_filenames: false
```

4. **Install the git hook** by running:
```bash
prek install
```
This creates `.git/hooks/pre-commit` which executes the hooks defined in `.pre-commit-config.yaml`.

5. **Update CLAUDE.md** (create if needed) with a brief note:

```markdown
## Precommit Hooks

Fast parallel hooks via prek: <list active hooks>. Commit checkpoints frequently—hooks catch issues faster than manual checks.
```

## Constraints

### Speed Requirements
- Total precommit time must stay under 10 seconds
- Individual hooks must complete in under 5 seconds
- All hooks run in parallel (prek default behavior)

### What to Include
- Lint checks (if configured)
- Type checks (if configured)
- Format checks in check-only mode (no auto-fix)
- Very fast unit tests (only if they run in 2-3 seconds)

### What to EXCLUDE
- Full test suites (even if project has them)
- Integration tests
- E2E tests
- Slow build/compile steps
- Checks requiring network access
- Any check taking more than 5 seconds

### File Handling
- Create `.pre-commit-config.yaml` at project root
- If `.pre-commit-config.yaml` exists, update it (preserve existing hooks, add missing ones)
- Only add CLAUDE.md section if hooks were successfully created
- Keep CLAUDE.md addition to 3-5 lines max

## Do NOT
- Run destructive commands
- Install the prek binary (user handles that—just run `prek install` to set up hooks)
- Add hooks for tools not configured in the project
- Include slow test suites even if they exist
