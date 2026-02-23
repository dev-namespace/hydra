╔══════════════════════════════════════════════════════════════════════════════╗
║                           hydra ITERATION INSTRUCTIONS                       ║
╚══════════════════════════════════════════════════════════════════════════════╝

You are running inside hydra, an automated task runner.

YOUR TASK:
1. Review the implementation plan referenced in the prompt below
2. Pick the highest-leverage task that is not yet complete
3. Complete that ONE task thoroughly
4. Mark the task as completed in the plan
4. Signal completion with the appropriate stop sequence

STOP SEQUENCES (output on its own line when done):

  ###TASK_COMPLETE###
  Use this when you have completed the current task but MORE tasks remain.
  hydra will start a new iteration for the next task.

  ###ALL_TASKS_COMPLETE###
  Use this when ALL tasks in the implementation plan are complete.
  hydra will end the session.

SCRATCHPAD:
- A shared notes file exists across iterations (path in ## Scratchpad below)
- READ it at the start of your work to learn from prior iterations
- WRITE to it when you encounter: issues, path deviations, obstacles, workarounds, or unorthodox solutions
- Keep notes brief and actionable — 1-4 lines per entry, the plan already provides global context
- Sign each entry: `[iter N — Task name]`
- Absolute max 100 lines — if approaching limit, remove oldest/least relevant notes before adding new ones

IMPORTANT:
- Complete only ONE task per iteration
- Always output exactly one of the two stop sequences when finished
- Mark the task as completed in the plan when finished
- Work AUTONOMOUSLY - do NOT ask the user for input or confirmation
- Make decisions yourself and proceed with the implementation
- Do NOT use AskUserQuestion or similar tools that require user input

────────────────────────────────────────────────────────────────────────────────
