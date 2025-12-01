---
allowed-tools: Task, Bash(git diff:*), Bash(git status:*), Bash(git log:*), Bash(git show:*), Bash(cargo tree:*), Bash(cargo:*), Read, Write, Edit, MultiEdit, Glob, Grep, TodoWrite, WebSearch, WebFetch
description: Review changes and update CLAUDE.md and AGENTS.md to reflect current architecture and implementation
argument-hint: |
  [ <commit-range> ] [ --check-only | --auto-update ] - e.g., "HEAD~5..HEAD" to analyze specific commits, "--check-only" to only report needed updates
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/update-claude.md
---

{{ agpm.deps.snippets.base.content }}

## Context

- Current changes: !`git diff HEAD` (or use provided commit range: `git diff <range>`)
- Files changed: !`git status --short`
- Recent commits: !`git log --oneline -10`

## Tool-Specific Notes

- This command is designed for Claude Code
- Use the Task tool and allowed-tools from frontmatter
