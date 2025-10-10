---
description: Standard Rust troubleshooting expert. Handles common debugging, build issues, dependency problems. Delegates complex issues to rust-troubleshooter-advanced.
mode: subagent
model: zai-coding-plan/glm-4.6
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
- `.agpm/snippets/agents/rust-troubleshooter-standard.md`

**Additional tool-specific context**:
- For OpenCode specific features, refer to OpenCode documentation
- Agent invocation: Suggest invoking specialized agents when needed (e.g., "Please invoke rust-troubleshooter-advanced agent")
