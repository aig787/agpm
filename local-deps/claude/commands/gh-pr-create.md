---
allowed-tools: Bash(git *), Bash(gh *), Read, Glob, Grep, TodoWrite
description: Automatically create a GitHub pull request with a well-formatted title and description
argument-hint: |
  [ --draft ] [ --base <branch> ] [ title ] - e.g., "--draft" or "--base develop" or custom title
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/gh-pr-create.md
---

{{ agpm.deps.snippets.base.content }}

## Context

- Current branch: !`git branch --show-current`
- Git status: !`git status --short`

## Tool-Specific Notes

- This command is designed for Claude Code
- Use the Task tool and allowed-tools from frontmatter
