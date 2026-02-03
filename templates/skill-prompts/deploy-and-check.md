# Create deploy-and-check Skill

Your task is to create a Claude Code skill file at `.claude/skills/deploy-and-check/SKILL.md` that serves as a quick reference guide for deployment and production verification in this project.

## Instructions

1. **Analyze the repository** to understand:
   - CI/CD configuration (GitHub Actions, GitLab CI, Jenkins, etc.)
   - Deployment method (Vercel, AWS, Docker, SSH, etc.)
   - Production infrastructure setup
   - Logging and monitoring setup

2. **Test non-destructive commands** to verify connectivity:
   - Check if SSH access to production servers works (do NOT make changes)
   - Verify you can access build/deploy logs
   - Do NOT trigger any deployments or make any production changes

3. **Ask clarifying questions** if needed:
   - How deployments are triggered (merge to main, manual, etc.)
   - Where to find deployment and build logs
   - SSH hostnames or connection details for production servers
   - How to verify a deployment succeeded

4. **Create the skill file** with this exact format:

```yaml
---
name: deploy-and-check
description: Quick reference for deployment and verification
disable-model-invocation: true
---
```

Followed by a VERY succinct guide containing ONLY:
- How to trigger a deployment
- How to read build and deploy logs
- How to SSH into production machines
- How to verify the deployment succeeded

## Constraints

- Keep the guide extremely brief - bullet points only
- Only include methods that actually work for this project
- Do NOT include commands that would modify production
- Create the `.claude/skills/deploy-and-check/` directory if it doesn't exist
