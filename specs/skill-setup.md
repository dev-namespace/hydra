# Skill Setup

Extension to `hydra init` that optionally creates Claude Code skills for local development and deployment workflows.

## User Capabilities

### During `hydra init`

- Users are prompted "Configure Claude Code permissions? [y/N]"
- Users are prompted "Set up dev skills (local-dev-guide + deploy-and-check)? [y/N]" after permissions
- Users are prompted "Set up precommit hooks? [y/N]" after dev skills
- Users are prompted "Add CLAUDE.md instructions (browser automation, specs)? [y/N]" after precommit
- Users are prompted "Create .hydra/ directory with prompt template? [y/N]" at the end
- Users can decline any prompt by pressing Enter or typing "n"
- Users can accept by typing "y" or "Y"

### When Creating a Skill

- Users interact with Claude Code in headful (interactive) mode
- Users can answer Claude's questions about their project setup
- Users see Claude analyze the repository and test non-destructive commands
- Users can interrupt Claude with Ctrl+C if needed

### Permissions Setup

- Users interact with Claude to configure `.claude/settings.local.json`
- Claude analyzes the project and suggests appropriate non-destructive permissions
- Permissions include read/write access to project files, `.claude/`, and `.hydra/`

### Skill Templates

- Users can customize permissions prompt via `~/.hydra/skill-templates/permissions.md`
- Users can customize dev skills prompt via `~/.hydra/skill-templates/dev-skills.md`
- Users can customize precommit prompt via `~/.hydra/skill-templates/precommit.md`
- If template files don't exist, embedded defaults are used

## Constraints

### Prompt Behavior

- Prompts default to "No" (pressing Enter skips)
- Steps are executed sequentially: permissions → dev skills → precommit → CLAUDE.md instructions → .hydra/ directory
- If user declines all, init completes normally with no additional output

### Permissions Setup

- Claude creates/updates `.claude/settings.local.json`
- Grants non-destructive permissions: Read, Write, Edit, Glob, Grep for project files
- Allows Bash commands for build, dev server, and SSH (non-destructive)
- Denies destructive commands (rm -rf, git push --force, deploy, etc.)
- Ensures access to `.claude/` and `.hydra/` directories

### Claude Code Execution

- Claude is spawned via hydra's existing PTY infrastructure
- Claude runs in headful/interactive mode (not with `--print`)
- Claude receives a skill-creation prompt that instructs it to:
  1. Analyze the repository structure and configuration files
  2. Test non-destructive commands (build, dev server, SSH connectivity)
  3. Ask user questions as needed to fill in gaps
  4. Create the skill file(s) at `.claude/skills/<skill-name>/SKILL.md`
- Claude must NOT perform destructive actions (deploy, delete, etc.)

### Dev Skills (Combined)

- Single prompt instructs Claude to create both skills **in parallel using subagents**
- Both `.claude/skills/local-dev-guide/` and `.claude/skills/deploy-and-check/` directories are pre-created
- Each skill file must be **max 30 lines** (including frontmatter), bullet points only

### local-dev-guide Content

- Build command(s), dev server, docker/compose (if applicable), test commands, test credentials/seed data

### deploy-and-check Content

- How to trigger deployment, read logs, SSH into production, verify success

### Skill Format

Skills follow Claude Code skill format with frontmatter and `disable-model-invocation: true` so users invoke manually via `/local-dev-guide` or `/deploy-and-check`.

### Precommit Setup

- Users are prompted "Set up precommit hooks? [y/N]" after skill prompts
- Claude analyzes the codebase and generates a `.pre-commit-config.yaml`
- Claude shows the generated config and asks for confirmation before running `prek install`
- Users can review the config and request changes before installation proceeds
- Users can customize the precommit prompt via `~/.hydra/skill-templates/precommit.md`

### Precommit Hook Requirements

Hooks must be **fast and parallel**:

- All hooks run in parallel via prek's parallel execution
- Individual hooks should complete in under 5 seconds
- Total precommit time should stay under 10 seconds

Hooks to include (if applicable to the project):

- **Lint**: ESLint, Clippy, Ruff, golangci-lint, etc.
- **Type check**: TypeScript, mypy, pyright, etc.
- **Sanity checks**: Format check (no auto-fix), import sorting check
- **Fast tests**: Only if they run in under 2-3 seconds (unit tests, not integration)

Hooks to **exclude**:

- Full test suites (even if project has them)
- Integration tests
- E2E tests
- Build/compile steps that take more than a few seconds
- Any check that requires network access
- Any check that takes more than 5 seconds

### Precommit Output Location

- Hooks are configured in `.pre-commit-config.yaml` at project root
- If `.pre-commit-config.yaml` already exists, Claude updates it (preserving existing hooks)

### CLAUDE.md Update

If and only if precommit hooks are successfully created:

- Claude appends a brief section to `CLAUDE.md` (creates file if needed)
- The section explains which hooks are active
- Suggests Claude commit checkpoints frequently and rely on parallel hooks
- Must be very succinct (3-5 lines max)

Example CLAUDE.md addition:
```markdown
## Precommit Hooks

Fast parallel hooks via prek: lint, typecheck, format-check. Commit checkpoints frequently—hooks catch issues faster than manual checks.
```

### CLAUDE.md Instructions

- Does NOT spawn Claude — directly appends sections to `CLAUDE.md`
- If `CLAUDE.md` doesn't exist, creates it with a `# Project` heading
- Each section is idempotent — skips if the heading already exists
- Sections added:
  - `## Browser Automation` — documents `agent-browser` CLI usage
  - `## Specs` — explains `/spec study` for reviewing existing systems

## Related Specs

- [Hydra](./hydra.md) - Core hydra functionality and PTY infrastructure

## Source

- [src/main.rs](../src/main.rs) - `init_command` function and `setup_skills` integration
- [src/skill.rs](../src/skill.rs) - Skill creation infrastructure (`prompt_yes_no`, `create_skill_with_claude`)
- [src/pty.rs](../src/pty.rs) - PTY infrastructure for spawning Claude
- [templates/skill-prompts/](../templates/skill-prompts/) - Embedded default prompts
  - `permissions.md` - Claude Code permissions setup
  - `dev-skills.md` - Combined local-dev-guide + deploy-and-check (parallel subagents)
  - `precommit.md` - Precommit hooks setup with prek
