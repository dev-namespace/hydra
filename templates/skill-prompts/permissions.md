# Configure Claude Code Permissions

Your task is to create a `.claude/settings.local.json` file that grants appropriate non-destructive permissions for this project.

## Instructions

1. **Analyze the repository** to understand:
   - Project structure and file locations
   - Build system and development tools used
   - Test framework and commands
   - Any deployment or infrastructure configuration

2. **Create `.claude/settings.local.json`** with permissions that allow:
   - Read, Write, Edit access to all project files
   - Read, Write, Edit access to `.claude/` and `.hydra/` directories
   - Bash commands for:
     - Building the project (npm, cargo, make, etc.)
     - Running the dev server
     - Running tests
     - Git operations (status, diff, log, add, commit, branch, checkout)
     - SSH connectivity checks (non-destructive)
     - Docker/docker-compose commands for local development

3. **Deny destructive commands** including:
   - `rm -rf` or recursive deletion
   - `git push --force` or force pushes
   - `git reset --hard` (data loss risk)
   - Production deployments
   - Database drops or destructive migrations
   - Any command that could cause data loss

## Required File Format

Create `.claude/settings.local.json` with this structure:

```json
{
  "permissions": {
    "allow": [
      "Read(*)",
      "Write(*)",
      "Edit(*)",
      "Glob(*)",
      "Grep(*)",
      "Bash(git status*)",
      "Bash(git diff*)",
      "Bash(git log*)",
      "Bash(git add*)",
      "Bash(git commit*)",
      "Bash(git checkout*)",
      "Bash(git fetch*)",
      "Bash(git pull*)",
      "Bash(<build-command>*)",
      "Bash(<test-command>*)",
      "Bash(<dev-server-command>*)"
    ],
    "ask": [
      "Bash(git branch*)",
      "Bash(git stash*)",
      "Bash(git merge*)",
      "Bash(git rebase*)",
      "Bash(rm -rf*)",
      "Bash(git push --force*)",
      "Bash(git push -f*)",
      "Bash(git reset --hard*)",
      "Bash(*deploy*)",
      "Bash(*production*)",
      "Bash(DROP DATABASE*)",
      "Bash(DROP TABLE*)"
    ],
    "deny": []
  }
}
```

## Constraints

- Create the `.claude/` directory if it doesn't exist
- Replace `<build-command>`, `<test-command>`, `<dev-server-command>` with actual commands for this project
- Add project-specific build, test, and dev commands to the allow list
- Keep the deny list conservative - better to require explicit permission than accidentally allow destructive actions
- Do NOT run any destructive commands while analyzing the project
