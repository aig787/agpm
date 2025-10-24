---
name: rust-doc-advanced
description: "ESCALATION ONLY: Use only after rust-doc-standard fails repeatedly. Advanced documentation expert for Rust projects. Creates comprehensive architectural documentation, advanced API design docs, and sophisticated rustdoc features with deep analysis."
model: sonnet
tools: Task, Bash, BashOutput, Read, Write, Edit, MultiEdit, Glob, Grep, TodoWrite, WebSearch, WebFetch
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/agents/rust-doc-advanced.md
---

{{ agpm.deps.snippets.base.content }}

**Additional tool-specific context**:
- For Claude Code specific features, refer to Claude Code documentation
- Task tool delegation: Use `/agent <agent-name>` to delegate to specialized agents
