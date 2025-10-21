---
allowed-tools: Task, Bash(git diff:*), Bash(git status:*), Bash(git log:*), Bash(git show:*), Bash(cargo fmt:*), Bash(cargo clippy:*), Bash(cargo test:*), Bash(cargo nextest:*), Bash(cargo build:*), Bash(cargo doc:*), Bash(cargo check:*), Read, Edit, MultiEdit, Glob, Grep, TodoWrite, WebSearch, WebFetch
description: Perform comprehensive PR review for AGPM project
argument-hint: |
  [ <commit> | <range> ] [ --quick | --full | --security | --performance ] - e.g., "abc123 --quick" for single commit, "main..HEAD --full" for range
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/pr-self-review.md
---

{{ agpm.deps.snippets.base.content }}

## Tool-Specific Notes

- This command is designed for Claude Code
- Use the Task tool and allowed-tools from frontmatter
