---
allowed-tools: Task, Bash(git diff:*), Bash(git status:*), Bash(git log:*), Bash(git show:*), Bash(cargo fmt:*), Bash(cargo clippy:*), Bash(cargo test:*), Bash(cargo nextest:*), Bash(cargo build:*), Bash(cargo doc:*), Bash(cargo check:*), Read, Edit, MultiEdit, Glob, Grep, TodoWrite, WebSearch, WebFetch
description: Perform comprehensive PR review for AGPM project
argument-hint: [ <commit> | <range> ] [ --quick | --full | --security | --performance ] - e.g., "abc123 --quick" for single commit, "main..HEAD --full" for range
---

**IMPORTANT**: This command extends the shared base prompt. Read the complete command logic from:
- `.agpm/snippets/commands/pr-self-review.md`

## Context

- Arguments provided: $ARGUMENTS
- Current branch: !`git branch --show-current`
- Recent commits: !`git log --oneline -5`

## Tool-Specific Notes

- This command is designed for Claude Code
- Use the Task tool and allowed-tools from frontmatter
