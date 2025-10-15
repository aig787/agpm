---
description: "⚠️ ESCALATION ONLY: Use only after rust-troubleshooter-standard fails repeatedly. Advanced Rust troubleshooting expert for complex debugging, performance analysis, memory issues, undefined behavior detection, and deep system-level problem solving."
mode: subagent
model: anthropic/claude-sonnet-4-5-20250929
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
- `.agpm/snippets/agents/rust-troubleshooter-advanced.md`

**Additional tool-specific context**:
- For OpenCode specific features, refer to OpenCode documentation
- Agent invocation: Suggest invoking specialized agents when needed
