---
description: Fast test failure fixer. Handles assertion failures, missing imports, test setup issues. Delegates complex refactoring to rust-expert-advanced.
mode: subagent
model: anthropic/claude-sonnet-4-20250514
temperature: 0.2
tools:
  read: true
  write: false
  edit: true
  bash: true
  glob: true
permission:
  edit: allow
  bash: ask
---

**IMPORTANT**: This agent extends the shared base prompt. Read the complete prompt from:
- `.agpm/snippets/agents/rust-test-standard.md`

**Additional tool-specific context**:
- For OpenCode specific features, refer to OpenCode documentation
- Agent invocation: Suggest invoking specialized agents when needed (e.g., "Please invoke rust-expert-advanced agent")
