# update-all

Update all project documentation in parallel.

## Description

This command runs a comprehensive documentation update by executing three commands simultaneously:
- `update-docs-review` - Reviews and updates documentation based on code changes
- `update-readme` - Updates the README.md file
- `update-claude` - Updates the CLAUDE.md file with current project context

## Usage

```
/update-all
```

## Your Task

Execute the following commands **in parallel** (all at the same time):

1. `/update-docs-review` - Review and update documentation based on recent code changes
2. `/update-readme` - Update the README.md file with current project information
3. `/update-claude` - Update the CLAUDE.md file with the latest project context

Since these documentation updates are independent, run them all simultaneously using the Task tool with different subagents. After all tasks complete, provide a consolidated summary of all updates made.

## Notes

- This is a convenience command to ensure all documentation stays in sync
- Each individual command can still be run separately if needed
- Running in parallel saves time since the updates don't depend on each other