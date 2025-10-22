---
name: rust-linting-advanced
description: Advanced linting and code quality fixes (Sonnet). Handles complex clippy warnings, refactoring suggestions. Delegates architectural changes to rust-expert-opus.
model: sonnet
tools: Task, Bash, BashOutput, Read, Edit, MultiEdit, Glob, Grep, TodoWrite
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/agents/rust-linting-advanced.md
---

{{ agpm.deps.snippets.base.content }}

**Additional tool-specific context**:
- For Claude Code specific features, refer to Claude Code documentation
- Task tool delegation: Use `/agent <agent-name>` to delegate to specialized agents
