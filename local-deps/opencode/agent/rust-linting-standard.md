---
description: Fast Rust linting and formatting (optimized for speed with quick model)
mode: subagent
temperature: 0.0
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
        path: ../../snippets/agents/rust-linting-standard.md
---

{{ agpm.deps.snippets.base.content }}

**Additional tool-specific context**:

- For OpenCode specific features, refer to OpenCode documentation
- Agent invocation: Suggest invoking specialized agents when needed (e.g., "Please invoke rust-linting-advanced agent")
