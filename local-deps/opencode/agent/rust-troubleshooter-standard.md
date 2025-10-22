---
description: Standard Rust troubleshooting expert. Handles common debugging, build issues, dependency problems. Delegates complex issues to rust-troubleshooter-advanced.
mode: subagent
temperature: 0.2
tools:
  read: true
  write: false
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
        path: ../../snippets/agents/rust-troubleshooter-standard.md
---

{{ agpm.deps.snippets.base.content }}

**Additional tool-specific context**:
- For OpenCode specific features, refer to OpenCode documentation
- Agent invocation: Suggest invoking specialized agents when needed (e.g., "Please invoke rust-troubleshooter-advanced agent")
