---
description: Create well-formatted git commits following project conventions - supports single or multiple logically grouped commits
---

## Your task

Create well-formatted git commits following project conventions.

**IMPORTANT**: You are being asked to directly create git commits - analyze the changes, craft appropriate commit messages, and use the git commands to commit them. Do NOT ask for permission or confirmation.

**IMPORTANT**: This command extends the shared base prompt. Read the complete command logic from:
- `.agpm/snippets/commands/commit.md`

## Argument Parsing

Parse the arguments using this convention:
```
/commit [flags] [paths...] [-- commit message]
```

**Argument Order**:
1. **Flags**: Arguments starting with `--` (e.g., `--multi`, `--dry-run`, `--no-attribution`)
2. **Paths**: Directory/file paths (e.g., `.opencode`, `src/`, `tests/`)
3. **Separator**: `--` (optional, separates paths from commit message)
4. **Message**: Commit message (everything after `--` or last non-path argument)

**Examples**:
```
/commit                              # all tracked changes
/commit .opencode                    # specific directory
/commit --multi src/ tests/          # flag + paths
/commit src/ tests/ -- feat: update cli and tests    # paths + message
/commit --multi=3 --no-attribution   # flags only
/commit -- feat: quick fix           # flag + message (no paths)
/commit .opencode -- docs: update opencode agents    # path + message
```

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- Parse flags first, then paths, then message (after `--` separator)
- Pass parsed arguments to the sub-logic in `.agpm/snippets/commands/commit.md`

## Execution

Based on the parsed arguments, execute the appropriate commit strategy:
- `--multi` or `--multi=N`: Create multiple logically grouped commits
- No flags: Create a single commit with all changes
- `--dry-run`: Show what would be committed without actually committing

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- Use the Bash tool to run git commands directly
- Follow the commit message style from the repository
