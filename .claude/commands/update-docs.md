---
allowed-tools: Task, Bash(git diff:*), Bash(git status:*), Bash(git log:*), Bash(git show:*), Bash(cargo:*), Read, Write, Edit, MultiEdit, Glob, Grep, TodoWrite, WebSearch, WebFetch
description: Review changes and update README.md to stay current with implementation
argument-hint: [ --check-only | --auto-update ] - e.g., "--check-only" to only report needed updates
---

**IMPORTANT**: This command extends the shared base prompt. Read the complete command logic from:
- `.agpm/snippets/commands/update-docs.md`

## Context

- Current changes: !`git diff HEAD`
- Files changed: !`git status --short`
- Recent commits: !`git log --oneline -5`

## Tool-Specific Notes

- This command is designed for Claude Code
- Use the Task tool and allowed-tools from frontmatter
