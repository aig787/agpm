# update-all

Update all project documentation sequentially.

## Description

This command runs a comprehensive documentation update by executing three commands in sequence:
- `update-docstrings` - Reviews and updates Rust docstrings based on code changes (uses `rust-doc-standard` or `rust-doc-advanced`)
- `update-docs` - Updates the project documentation files (README.md and docs/) with potential use of `rust-doc-standard`/`advanced`
- `update-claude` - Updates the CLAUDE.md file with current project context (may use `rust-doc-*` and `rust-expert-*` agents)

## Usage

```
/update-all
```

## Your Task

Execute the documentation update tasks by loading and running the instructions from each command file:

1. **Update Rust docstrings** - Load `.claude/commands/update-docstrings.md` and execute the task described there to review and update Rust docstrings based on recent code changes

2. **Update project documentation** - Load `.claude/commands/update-docs.md` and execute the task described there to update README.md and docs/ files with current project information  

3. **Update CLAUDE.md** - Load `.claude/commands/update-claude.md` and execute the task described there to update the CLAUDE.md file with latest project context and architecture

For each command, you should:
- Read the markdown file from `.claude/commands/`
- Follow the instructions in the "Your task" section
- Use the allowed tools specified in the frontmatter
- Execute any Task invocations or other operations as instructed in that file

**IMPORTANT**: These must be run sequentially (one after another) rather than in parallel, as the Task tool may encounter issues when multiple Task invocations are run simultaneously.

## Notes

- This is a convenience command to ensure all documentation stays in sync
- Each individual command can still be run separately if needed
- Commands are run sequentially to ensure reliable execution of Task tool invocations