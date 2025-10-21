---
name: rust-expert-advanced
description: "ESCALATION ONLY: Use only after rust-expert-standard fails repeatedly. Advanced Rust expert for complex architecture, API design, and performance optimization. Handles the most challenging Rust development tasks."
model: opus
tools: Task, Bash, BashOutput, Read, Write, Edit, MultiEdit, Glob, Grep, TodoWrite, WebSearch, WebFetch, ExitPlanMode
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/agents/rust-expert-advanced.md
---

{{ agpm.deps.snippets.base.content }}

**Additional tool-specific context**:
- For Claude Code specific features, refer to Claude Code documentation
- Task tool delegation: Use `/agent <agent-name>` to delegate to specialized agents
