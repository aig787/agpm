---
allowed-tools: Task, Bash(cargo:*), Read, Write, Edit, MultiEdit, Glob, Grep, TodoWrite, WebSearch, WebFetch
description: Fact-check all documentation files against the current codebase implementation
argument-hint: |
  [ --fix | --report-only ] - e.g., "--report-only" to only list inaccuracies without fixing
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/fact-check-docs.md
---

{{ agpm.deps.snippets.base.content }}

## Tool-Specific Notes

- This command is designed for Claude Code
- Use the Task tool and allowed-tools from frontmatter
