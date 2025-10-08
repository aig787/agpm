---
allowed-tools: Task, Bash(git add:*), Bash(git status:*), Bash(git diff:*), Bash(git commit:*), Bash(git log:*), Bash(git show:*), Read, Glob, Grep, TodoWrite
description: Create well-formatted git commits following project conventions - supports single or multiple logically grouped commits
argument-hint: [ --multi[=N] | --co-authored | --contributed | --no-attribution | --include-untracked ] [ paths... ] [ message ] - e.g., "--multi" or "--multi=3" for multiple commits or "tests/" for specific paths
---

**IMPORTANT**: This command extends the shared base prompt. Read the complete command logic from:
- `.agpm/snippets/commands/commit.md`

## Context

- Current git status: !`git status --short`
- Current git diff: !`git diff HEAD`
- Recent commits for style reference: !`git log --oneline -5`

## Tool-Specific Notes

- This command is designed for Claude Code
- Use the Task tool and allowed-tools from frontmatter