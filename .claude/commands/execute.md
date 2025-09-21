---
allowed-tools: Task, Bash, BashOutput, Read, Write, Edit, MultiEdit, Glob, Grep, TodoWrite, WebSearch, WebFetch, ExitPlanMode, NotebookEdit
description: Execute a shared markdown prompt from .claude/ directory
argument-hint: <prompt-name> [additional-args...] - e.g., "fix-failing-tests" or "refactor-duplicated-code --module src/cache"
---

## Context

- Available prompts: !`ls -1 .claude/*.md | grep -v commands | xargs -I {} basename {} .md`
- Current directory: !`pwd`
- Git status: !`git status --short`

## Your task

Execute a shared markdown prompt from the `.claude/` directory based on the provided arguments.

1. **Parse the arguments**:
   - First argument is the prompt name (without .md extension)
   - Additional arguments can be passed to customize the prompt execution
   - Arguments: $ARGUMENTS

2. **Load and execute the prompt**:
   - Look for the prompt file at `.claude/{prompt-name}.md`
   - If the prompt doesn't exist, list available prompts and provide helpful guidance
   - Read the entire prompt file to understand the task
   - Execute the prompt, incorporating any additional arguments provided

3. **Prompt execution strategy**:
   - If the prompt requires specialized analysis or complex operations, use Task tool to delegate to appropriate agents
   - For prompts that involve code changes, use TodoWrite to track progress
   - Apply any additional arguments as context or constraints to the prompt execution

4. **Available shared prompts** (common use cases):
   - `fix-failing-tests`: Identify and fix failing tests in the codebase
   - `improve-test-coverage`: Analyze and improve test coverage
   - `refactor-duplicated-code`: Find and refactor duplicated code patterns
   - Custom prompts can be added to `.claude/` directory as needed

5. **Error handling**:
   - If the prompt file doesn't exist, show available prompts with descriptions
   - If the prompt has specific requirements (tools, context), validate before execution
   - Provide clear feedback about what the prompt is doing

## Example usage

```bash
# Execute the fix-failing-tests prompt
/execute fix-failing-tests

# Execute with additional context
/execute refactor-duplicated-code --module src/cache --preserve-api

# Execute improve-test-coverage for specific module
/execute improve-test-coverage tests/integration_stress_test.rs
```

## Implementation approach

When executing a prompt:
1. First, verify the prompt exists and read its content
2. Parse any special directives or requirements from the prompt
3. Create a todo list if the prompt involves multiple steps
4. Execute the prompt instructions, using appropriate tools
5. Provide clear feedback about progress and results

For complex prompts that require deep analysis:
- Delegate to specialized agents using Task tool
- Run operations in parallel when possible for efficiency
- Track progress with TodoWrite for visibility

Remember: The goal is to make commonly-used prompts easily reusable and shareable across the team.