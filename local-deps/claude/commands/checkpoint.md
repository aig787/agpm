---
allowed-tools: Bash(git:*), Read, Glob, Grep, TodoWrite
description: Create or restore Git-based checkpoints for safe AI-assisted editing
argument-hint: |
 [ create | restore | list | clean ] [--message "description"] - e.g., "create --message 'Before refactoring cache module'" or "restore HEAD~1"
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/checkpoint.md
---

{{ agpm.deps.snippets.base.content }}

## Context

- Current branch: !`git branch --show-current`
- Current status: !`git status --short`
- Uncommitted changes: !`git diff --stat HEAD`
- Recent commits: !`git log --oneline -5`

## Tool-Specific Notes

- This command is designed for Claude Code
- Use the Task tool and allowed-tools from frontmatter
