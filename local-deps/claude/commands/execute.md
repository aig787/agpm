---
allowed-tools: Task, Bash, BashOutput, Read, Write, Edit, MultiEdit, Glob, Grep, TodoWrite, WebSearch, WebFetch, ExitPlanMode, NotebookEdit
description: Execute a shared markdown prompt from .claude/ directory
argument-hint: <prompt-name> [additional-args...] - e.g., "fix-failing-tests" or "refactor-duplicated-code --module src/cache"
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/execute.md
---

{{ agpm.deps.snippets.base.content }}

## Context

- Available prompts: !`{ ls -1 .claude/*.md 2>/dev/null | grep -v commands; ls -1 .agpm/snippets/prompts/*.md 2>/dev/null; } | xargs -I {} basename {} .md | sort -u`
- Current directory: !`pwd`
- Git status: !`git status --short`

## Tool-Specific Notes

- This command is designed for Claude Code
- Use the Task tool and allowed-tools from frontmatter
