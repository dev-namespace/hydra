# Create Dev Skills

Create TWO Claude Code skill files in parallel using subagents (Task tool). Launch both subagents simultaneously in a single message.

Each subagent should:
1. Analyze the repo (build system, CI/CD, infra, etc.)
2. Test non-destructive commands to verify they work
3. Ask clarifying questions if needed
4. Create the skill file

## Skill 1: local-dev-guide

Create `.claude/skills/local-dev-guide/SKILL.md`.

Example output:

```markdown
---
name: local-dev-guide
description: Quick reference for local development commands
---

## Build
- `cargo build` — debug build
- `cargo build --release` — release build

## Dev Server
- `cargo run` — run locally
- `docker-compose up -d` — start dependencies (postgres, redis)

## Test
- `cargo test` — run all tests
- `cargo test <name>` — run specific test

## Seed Data
- `./scripts/seed.sh` — populate dev database
- Test user: admin@example.com / password123
```

Adapt to the actual project. Include only what applies. Max 30 lines total.
Do NOT include deployment or production commands.

## Skill 2: deploy-and-check

Create `.claude/skills/deploy-and-check/SKILL.md`.

Example output:

```markdown
---
name: deploy-and-check
description: Quick reference for deployment and verification
disable-model-invocation: true
---

## Deploy
- Merge to `main` triggers deploy via GitHub Actions
- Manual: `gh workflow run deploy.yml`

## Logs
- `gh run list --workflow=deploy.yml` — recent deploys
- `gh run view <id> --log` — deploy logs
- Datadog: https://app.datadoghq.com/logs?query=service:myapp

## SSH
- `ssh deploy@prod.example.com` — production server
- `ssh deploy@staging.example.com` — staging server

## Verify
- `curl -s https://api.example.com/health` — health check
- Check `/var/log/myapp/app.log` on prod for errors
```

Adapt to the actual project. Include only what applies. Max 30 lines total.
Do NOT trigger deployments or modify production.

## Constraints

- Create both skills IN PARALLEL using two subagents
- Each skill file must be under 30 lines total (including frontmatter)
- Only include commands that actually work in this project
- Create `.claude/skills/<name>/` directories if they don't exist
