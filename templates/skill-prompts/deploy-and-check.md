# Create deploy-and-check Skill

Create a Claude Code skill file at `.claude/skills/deploy-and-check/SKILL.md`.

## Steps

1. Analyze the repo (CI/CD, infra, deployment config, etc.)
2. Test non-destructive commands to verify they work
3. Ask clarifying questions if needed
4. Create the skill file

## Example output

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

## Constraints

- Adapt to the actual project. Include only what applies
- Max 30 lines total (including frontmatter)
- Only include commands that actually work in this project
- Create `.claude/skills/deploy-and-check/` directory if it doesn't exist
- Do NOT trigger deployments or modify production
