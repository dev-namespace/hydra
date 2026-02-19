# Create Dev Skills

Create TWO Claude Code skill files in parallel using subagents (Task tool). Launch both subagents simultaneously in a single message.

## Skill 1: local-dev-guide

Create `.claude/skills/local-dev-guide/SKILL.md`. Analyze the repo, test non-destructive commands (build, dev server, test), ask clarifying questions if needed.

```yaml
---
name: local-dev-guide
description: Quick reference for local development commands
disable-model-invocation: true
---
```

Content (max 30 lines, bullet points only):
- Build command(s)
- Dev server command(s)
- Docker/docker-compose instructions (if applicable)
- Test commands
- Test credentials or seed data (if applicable)

Do NOT include deployment or production commands.

## Skill 2: deploy-and-check

Create `.claude/skills/deploy-and-check/SKILL.md`. Analyze CI/CD config, test non-destructive connectivity (SSH, logs), ask clarifying questions if needed.

```yaml
---
name: deploy-and-check
description: Quick reference for deployment and verification
disable-model-invocation: true
---
```

Content (max 30 lines, bullet points only):
- How to trigger a deployment
- How to read build and deploy logs
- How to SSH into production machines
- How to verify the deployment succeeded

Do NOT trigger deployments or modify production.

## Constraints

- Create both skills IN PARALLEL using two subagents
- Each skill file must be under 30 lines total (including frontmatter)
- Only include commands that actually work in this project
- Create `.claude/skills/<name>/` directories if they don't exist
