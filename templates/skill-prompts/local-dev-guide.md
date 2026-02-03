# Create local-dev-guide Skill

Your task is to create a Claude Code skill file at `.claude/skills/local-dev-guide/SKILL.md` that serves as a quick reference guide for local development in this project.

## Instructions

1. **Analyze the repository** to understand:
   - Build system (npm, cargo, make, gradle, etc.)
   - Package manager and dependencies
   - Development server setup
   - Docker/docker-compose configuration (if present)
   - Test framework and commands
   - Database setup (if applicable)

2. **Test non-destructive commands** to verify they work:
   - Build commands (e.g., `npm run build`, `cargo build`)
   - Dev server commands (e.g., `npm run dev`, `cargo run`)
   - Test commands (e.g., `npm test`, `cargo test`)
   - Do NOT run any destructive commands (deploy, delete, drop database, etc.)

3. **Ask clarifying questions** if needed:
   - How to access test credentials or seed data
   - Any manual verification steps after starting the dev server
   - Environment variables needed for development

4. **Create the skill file** with this exact format:

```yaml
---
name: local-dev-guide
description: Quick reference for local development commands
disable-model-invocation: true
---
```

Followed by a VERY succinct guide containing ONLY:
- Build command(s)
- Dev server command(s)
- Docker/docker-compose instructions (if applicable)
- Commands for manual checks
- Test credentials or seed data info (if applicable)

## Constraints

- Keep the guide extremely brief - bullet points only
- Only include commands that actually work in this project
- Do NOT include deployment or production commands
- Create the `.claude/skills/local-dev-guide/` directory if it doesn't exist
