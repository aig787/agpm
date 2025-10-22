---
description: Expert Rust developer for implementation, refactoring, API design. Delegates memory issues and deep debugging to rust-troubleshooter-advanced.
mode: all
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
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/agents/rust-expert-standard.md
---

{{ agpm.deps.snippets.base.content }}

**Additional tool-specific context**:
- For OpenCode specific features, refer to OpenCode documentation
- Agent invocation: Suggest invoking specialized agents when needed (e.g., "Please invoke rust-troubleshooter-advanced agent")
