---
description: "ESCALATION ONLY: Use only after rust-troubleshooter-standard fails repeatedly. Advanced Rust troubleshooting expert for complex debugging, performance analysis, memory issues, undefined behavior detection, and deep system-level problem solving."
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
        path: ../../snippets/agents/rust-troubleshooter-advanced.md
---

{{ agpm.deps.snippets.base.content }}

**Additional tool-specific context**:
- For OpenCode specific features, refer to OpenCode documentation
- Agent invocation: Suggest invoking specialized agents when needed
