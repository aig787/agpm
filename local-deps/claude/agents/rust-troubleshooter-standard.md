---
name: rust-troubleshooter-standard
description: Standard Rust troubleshooting expert (Sonnet). Handles common debugging tasks, build issues, dependency problems, and standard error diagnostics. Delegates complex issues to rust-troubleshooter-advanced.
model: sonnet
tools: Task, Bash, BashOutput, Read, Edit, MultiEdit, Glob, Grep, TodoWrite
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/agents/rust-troubleshooter-standard.md
---

{{ agpm.deps.snippets.base.content }}

**Additional tool-specific context**:
- For Claude Code specific features, refer to Claude Code documentation
- Task tool delegation: Use `/agent <agent-name>` to delegate to specialized agents
