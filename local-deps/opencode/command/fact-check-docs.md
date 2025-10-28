---
description: Fact-check all documentation files against the current codebase implementation
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/fact-check-docs.md
---

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- Parse for flags: `--report-only`, `--fix`

{{ agpm.deps.snippets.base.content }}

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- **VERIFICATION TOOLS**: Use Read, Grep, and Glob tools extensively to cross-reference documentation claims with actual code
- **PRECISION APPROACH**: For each claim in documentation, find the corresponding code and verify exact matches
- **SYSTEMATIC PROCESS**: Go through each documentation file systematically, line by line
- Generate a detailed report of any inconsistencies found with specific evidence

**VERIFICATION STRATEGY**:
1. Read a section of documentation
2. Identify specific claims (versions, commands, paths, dependencies)
3. Use tools to locate the corresponding implementation
4. Compare claim vs reality exactly
5. Document every discrepancy found
6. Repeat for all documentation files
