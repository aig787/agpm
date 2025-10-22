---
description: Fast test failure fixer. Handles assertion failures, missing imports, test setup issues. Delegates complex refactoring to rust-expert-advanced.
mode: all
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
        path: ../../snippets/agents/rust-test-standard.md
---

{{ agpm.deps.snippets.base.content }}

**Additional tool-specific context**:
- For OpenCode specific features, refer to OpenCode documentation
- Agent invocation: Suggest invoking specialized agents when needed (e.g., "Please invoke rust-expert-advanced agent")
