---
name: rust-linting-standard
description: Fast Rust linting and formatting (Haiku - optimized for speed)
model: haiku
tools: Task, Read, Edit, MultiEdit, Glob, Grep, TodoWrite, Bash
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/agents/rust-linting-standard.md
---

{{ agpm.deps.snippets.base.content }}

**Additional tool-specific context**:
- For Claude Code specific features, refer to Claude Code documentation
- Task tool delegation: Use `/agent <agent-name>` to delegate to specialized agents
