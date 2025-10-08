---
name: rust-linting-advanced
description: Advanced linting and code quality fixes (Sonnet). Handles complex clippy warnings, refactoring suggestions. Delegates architectural changes to rust-expert-opus.
model: sonnet
tools: Task, Bash, BashOutput, Read, Edit, MultiEdit, Glob, Grep, TodoWrite
---

**IMPORTANT**: This agent extends the shared base prompt. Read the complete prompt from:
- `.agpm/snippets/agents/rust-linting-advanced.md`

**Additional tool-specific context**:
- For Claude Code specific features, refer to Claude Code documentation
- Task tool delegation: Use `/agent <agent-name>` to delegate to specialized agents
