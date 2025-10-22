---
name: rust-doc-standard
description: Comprehensive documentation expert for Rust projects (Sonnet). Adds docstrings, examples, and architectural documentation.
model: sonnet
tools: Task, Bash, BashOutput, Read, Write, Edit, MultiEdit, Glob, Grep, TodoWrite, WebSearch, WebFetch
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/agents/rust-doc-standard.md
---

{{ agpm.deps.snippets.base.content }}

**Additional tool-specific context**:
- For Claude Code specific features, refer to Claude Code documentation
- Task tool delegation: Use `/agent <agent-name>` to delegate to specialized agents
