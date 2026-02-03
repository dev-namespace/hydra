# Skill Setup

Extension to `hydra init` that optionally creates Claude Code skills for local development and deployment workflows.

## User Capabilities

### During `hydra init`

- Users are prompted "Set up local-dev-guide skill? [y/N]" after standard init completes
- Users are prompted "Set up deploy-and-check skill? [y/N]" after the first skill prompt
- Users can decline either skill by pressing Enter or typing "n"
- Users can accept by typing "y" or "Y"

### When Creating a Skill

- Users interact with Claude Code in headful (interactive) mode
- Users can answer Claude's questions about their project setup
- Users see Claude analyze the repository and test non-destructive commands
- Users can interrupt Claude with Ctrl+C if needed

### Skill Templates

- Users can customize skill creation prompts via `~/.hydra/skill-templates/local-dev-guide.md`
- Users can customize skill creation prompts via `~/.hydra/skill-templates/deploy-and-check.md`
- If template files don't exist, embedded defaults are used

## Constraints

### Prompt Behavior

- Prompts default to "No" (pressing Enter skips the skill)
- Skills are created one at a time, sequentially
- If user declines both, init completes normally with no additional output

### Claude Code Execution

- Claude is spawned via hydra's existing PTY infrastructure
- Claude runs in headful/interactive mode (not with `--print`)
- Claude receives a skill-creation prompt that instructs it to:
  1. Analyze the repository structure and configuration files
  2. Test non-destructive commands (build, dev server, SSH connectivity)
  3. Ask user questions as needed to fill in gaps
  4. Create the skill file at `.claude/skills/<skill-name>/SKILL.md`
- Claude must NOT perform destructive actions (deploy, delete, etc.)

### Skill Output Location

- Skills are created in `.claude/skills/<skill-name>/SKILL.md` (project-specific)
- The `.claude/skills/` directory is created if it doesn't exist

### local-dev-guide Skill Content

The created skill should be a VERY succinct but clear guide containing:
- Build command(s)
- Dev server command(s)
- Docker/docker-compose instructions (if applicable)
- Commands for manual checks
- Test credentials or seed data info (if applicable)

### deploy-and-check Skill Content

The created skill should be a VERY succinct but clear guide containing:
- How to trigger a deployment
- How to read build and deploy logs
- How to SSH into production machines
- How to verify the deployment succeeded

### Skill Format

Skills follow Claude Code skill format with frontmatter:
```yaml
---
name: local-dev-guide
description: Quick reference for local development commands
disable-model-invocation: true
---
```

The `disable-model-invocation: true` ensures users invoke these manually via `/local-dev-guide` or `/deploy-and-check`.

## Related Specs

- [Hydra](./hydra.md) - Core hydra functionality and PTY infrastructure

## Source

- [src/main.rs](../src/main.rs) - `init_command` function (to be extended)
- [src/pty.rs](../src/pty.rs) - PTY infrastructure for spawning Claude
- [templates/skill-prompts/](../templates/skill-prompts/) - Embedded default prompts (to be created)
