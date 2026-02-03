# Skill Setup Implementation Plan

## Summary

Extend `hydra init` to configure Claude Code permissions, optionally create skills (`local-dev-guide` and `deploy-and-check`), and set up fast precommit hooks via PTY-spawned Claude.

## Tasks

- [x] Add `templates/skill-prompts/permissions.md` prompt for Claude to create `.claude/settings.local.json` with non-destructive permissions
- [x] Extend `init_command()` to prompt "Configure Claude Code permissions? [y/N]" as first step, spawn Claude if accepted
- [x] Skill creation infrastructure (`prompt_yes_no()`, `create_skill_with_claude()`, skill prompts)
- [x] Extend `init_command()` to prompt for each skill and spawn Claude if accepted
- [x] Add `templates/skill-prompts/precommit.md` prompt for Claude to set up prek hooks
- [ ] Extend `init_command()` to prompt "Set up precommit hooks? [y/N]" after skills
- [ ] Claude creates `.prek.toml` with fast, parallel hooks (lint, typecheck, sanity checks)
- [ ] If hooks added, Claude appends succinct precommit guidance to `CLAUDE.md`

## Specs

- [Skill Setup](../specs/skill-setup.md) - requirements and constraints
- [Hydra](../specs/hydra.md#architecture) - PTY infrastructure

## Verification

- [x] `hydra init` prompts for permissions first, then skills
- [x] Claude creates valid `.claude/skills/<name>/SKILL.md` files
- [ ] `hydra init` prompts for precommit hooks after skills
- [ ] Claude creates valid `.prek.toml` with parallel hooks
- [ ] Claude appends precommit guidance to `CLAUDE.md` (only if hooks created)

**Note**: Verification that "Claude creates valid `.claude/settings.local.json` with non-destructive permissions" requires manual testing with actual Claude execution.
