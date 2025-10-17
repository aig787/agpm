---
description: Advanced linting and code quality fixes. Handles complex clippy warnings and refactoring suggestions. Delegates architectural changes to rust-expert-advanced.
mode: subagent
temperature: 0.2
tools:
  read: true
  write: true
  edit: true
  bash: true
  glob: true
permission:
  edit: allow
  bash: allow
---

**IMPORTANT**: This agent extends the shared base prompt. Read the complete prompt from:

- `.agpm/snippets/agents/rust-linting-advanced.md`

**Additional tool-specific context**:

- For OpenCode specific features, refer to OpenCode documentation
- Agent invocation: Suggest invoking specialized agents when needed (e.g., "Please invoke rust-expert-advanced agent")
