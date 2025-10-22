---
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/update-all.md
---

{{ agpm.deps.snippets.base.content }}

## Tool-Specific Notes

- This command is designed for Claude Code
- This is a meta-command that runs multiple documentation update commands in parallel
