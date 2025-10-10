---
description: Perform comprehensive PR review for AGPM project
---

## Your task

Perform a comprehensive pull request review for the AGPM project based on the current changes.

**IMPORTANT**: You are being asked to directly perform the review actions yourself - do NOT attempt to create or submit a pull request. Analyze the changes and generate a review report.

**IMPORTANT**: This command extends the shared base prompt. Read the complete command logic from:
- `.agpm/snippets/commands/pr-self-review.md`

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- Parse for review target: DIFF keyword for staged changes, commit hashes, branch names
- Parse for review scope: specific files, modules, or full review
- Pass parsed arguments to the sub-logic in `.agpm/snippets/commands/pr-self-review.md`

## Execution

Based on the parsed arguments:
- Use Task tool to delegate to specialized agents for code analysis
- Review against coding standards in `.agpm/snippets/rust-best-practices.md`
- Generate comprehensive review report with findings and recommendations

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- Focus on reviewing the actual changes in the repository
- Do NOT use gh CLI commands to create PRs
