# Skill Setup Implementation Plan

## Summary

Extend `hydra init` to optionally create Claude Code skills (`local-dev-guide` and `deploy-and-check`) by prompting the user and spawning Claude via PTY to interactively generate each skill.

## Tasks

- [ ] Add embedded skill-creation prompts in `templates/skill-prompts/{local-dev-guide,deploy-and-check}.md` with override support via `~/.hydra/skill-templates/`
- [ ] Add `prompt_yes_no()` helper for interactive y/N terminal prompts
- [ ] Add `create_skill_with_claude()` that creates `.claude/skills/<name>/` and spawns Claude via PTY with the prompt
- [ ] Extend `init_command()` to prompt for each skill and call `create_skill_with_claude()` if accepted

## Specs

- [Skill Setup](../specs/skill-setup.md) - requirements and constraints
- [Hydra](../specs/hydra.md#architecture) - PTY infrastructure

## Verification

- [ ] `hydra init` prompts for both skills, "y" spawns Claude, Enter/n skips
- [ ] Claude creates valid `.claude/skills/<name>/SKILL.md` files
