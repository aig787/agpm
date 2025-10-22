---
name: rust-expert-standard
description: Expert Rust developer for implementation, refactoring, API design (Sonnet). Delegates memory issues, UB, and deep debugging to rust-troubleshooter-advanced.
model: sonnet
tools: Task, Bash, BashOutput, Read, Write, Edit, MultiEdit, Glob, Grep, TodoWrite, WebSearch, WebFetch
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/agents/rust-expert-standard.md
---

{{ agpm.deps.snippets.base.content }}

**Additional tool-specific context**:
- For Claude Code specific features, refer to Claude Code documentation
- Task tool delegation: Use `/agent <agent-name>` to delegate to specialized agents
