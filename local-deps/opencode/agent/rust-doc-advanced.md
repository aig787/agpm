---
description: "ESCALATION ONLY: Use only after rust-doc-standard fails repeatedly. Advanced documentation expert for Rust projects. Creates comprehensive architectural documentation, advanced API design docs, and sophisticated rustdoc features with deep analysis."
mode: subagent
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
        path: ../../snippets/agents/rust-doc-advanced.md
---

{{ agpm.deps.snippets.base.content }}

**Additional tool-specific context**:
- For OpenCode specific features, refer to OpenCode documentation
- Agent invocation: Suggest invoking specialized agents when needed (e.g., "Please invoke rust-troubleshooter-advanced agent")
