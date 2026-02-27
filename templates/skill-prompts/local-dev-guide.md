# Create local-dev-guide Skill

Create a Claude Code skill file at `.claude/skills/local-dev-guide/SKILL.md`.

## Steps

1. Analyze the repo (build system, package manager, dev dependencies, etc.)
2. Test non-destructive commands to verify they work
3. Ask clarifying questions if needed
4. Create the skill file

## Example output

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

## Constraints

- Adapt to the actual project. Include only what applies
- Max 30 lines total (including frontmatter)
- Only include commands that actually work in this project
- Create `.claude/skills/local-dev-guide/` directory if it doesn't exist
- Do NOT include deployment or production commands
