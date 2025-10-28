---
description: "ESCALATION ONLY: Use only after rust-expert-standard fails repeatedly. Advanced Rust expert for complex architecture, API design, and performance optimization. Handles the most challenging Rust development tasks."
mode: all
temperature: 0.3
tools:
  read: true
  write: true
  edit: true
  bash: true
  glob: true
permission:
  edit: allow
  bash: allow
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/agents/rust-expert-advanced.md
---

{{ agpm.deps.snippets.base.content }}

**Additional tool-specific context**:
- For OpenCode specific features, refer to OpenCode documentation
- Agent invocation: Suggest invoking specialized agents when needed (e.g., "Please invoke rust-troubleshooter-advanced agent")
