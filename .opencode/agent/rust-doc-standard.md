---
description: Comprehensive documentation expert for Rust projects. Adds docstrings, examples, and architectural documentation.
mode: subagent
model: zai-coding-plan/glm-4.6
temperature: 0.2
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
- `.agpm/snippets/agents/rust-doc-standard.md`

**Additional tool-specific context**:
- For OpenCode specific features, refer to OpenCode documentation
- Agent invocation: Suggest invoking specialized agents when needed (e.g., "Please invoke rust-doc-advanced agent")
