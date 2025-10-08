---
description: "⚠️ ESCALATION ONLY: Use only after rust-test-standard fails repeatedly. Advanced test expert for Rust projects. Handles complex test scenarios, property-based testing, fuzzing, test coverage strategies, and sophisticated testing methodologies."
mode: subagent
model: anthropic/claude-opus-4-20250514
temperature: 0.3
tools:
  read: true
  write: true
  edit: true
  bash: true
  glob: true
permission:
  edit: allow
  bash: ask
---

**IMPORTANT**: This agent extends the shared base prompt. Read the complete prompt from:
- `.agpm/snippets/agents/rust-test-advanced.md`

**Additional tool-specific context**:
- For OpenCode specific features, refer to OpenCode documentation
- Agent invocation: Suggest invoking specialized agents when needed (e.g., "Please invoke rust-troubleshooter-advanced agent")
