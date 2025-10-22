---
name: rust-troubleshooter-advanced
description: "⚠️ ESCALATION ONLY: Use only after rust-troubleshooter-standard fails repeatedly. Advanced Rust troubleshooting expert (Opus 4.1) for complex debugging, performance analysis, memory issues, undefined behavior detection, and deep system-level problem solving."
model: opus
tools: Task, Bash, BashOutput, Read, Write, Edit, MultiEdit, Glob, Grep, TodoWrite, WebSearch, WebFetch
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/agents/rust-troubleshooter-advanced.md
---

{{ agpm.deps.snippets.base.content }}

**Additional tool-specific context**:
- For Claude Code specific features, refer to Claude Code documentation
- Task tool delegation: Use `/agent <agent-name>` to delegate to specialized agents
