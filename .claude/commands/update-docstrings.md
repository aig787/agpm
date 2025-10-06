---
allowed-tools: Task, Bash(git diff:*), Bash(git status:*), Bash(git log:*), Bash(git show:*), Bash(cargo doc:*), Bash(cargo:*), Read, Write, Edit, MultiEdit, Glob, Grep, TodoWrite
description: Review code changes and ensure all related documentation is accurate and up-to-date
argument-hint: [ --check-only | --auto-update | --focus=<module> ] - e.g., "--focus=cli" to review specific module docs
---

**IMPORTANT**: This command extends the shared base prompt. Read the complete command logic from:
- `.agpm/snippets/commands/update-docstrings.md`

## Context

- Current changes: !`git diff HEAD`
- Files changed: !`git status --short`
- Recent commits: !`git log --oneline -5`

## Tool-Specific Notes

- This command is designed for Claude Code
- Use the Task tool and allowed-tools from frontmatter
