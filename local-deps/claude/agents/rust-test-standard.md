---
name: rust-test-standard
description: Fast test failure fixer (Sonnet). Handles assertion failures, missing imports, test setup issues. Delegates complex refactoring to rust-expert-advanced.
model: sonnet
tools: Task, Bash, BashOutput, Read, Edit, MultiEdit, Glob, Grep, TodoWrite
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/agents/rust-test-standard.md
---

{{ agpm.deps.snippets.base.content }}

**Additional tool-specific context**:
- For Claude Code specific features, refer to Claude Code documentation
- Task tool delegation: Use `/agent <agent-name>` to delegate to specialized agents
