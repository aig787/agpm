---
allowed-tools: Task, Bash(cargo fmt:*), Bash(cargo clippy:*), Bash(cargo doc:*), Bash(cargo check:*), Bash(cargo build:*), Bash(cargo test:*), Bash(cargo fix:*), Bash(rustfmt:*), BashOutput, Read, Write, Edit, MultiEdit, Glob, Grep, TodoWrite, WebSearch, WebFetch, ExitPlanMode
description: Run code quality checks (formatting, linting, documentation)
argument-hint: |
 [ --fix | --check ] [ --all ] [ --doc ] - e.g., "--fix" or "--check --all"
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/lint.md
---

{{ agpm.deps.snippets.base.content }}

## Context

- Project name: AGPM (Claude Code Package Manager)

## Tool-Specific Notes

- This command is designed for Claude Code
- Use the Task tool and allowed-tools from frontmatter
