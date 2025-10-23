## Prompt Execution Implementation

Execute a shared markdown prompt from the `.claude/` directory based on the provided arguments.

### Argument Semantics

- **Prompt name**: First argument (without .md extension) - the prompt file to execute
- **Additional arguments**: Passed to customize prompt execution behavior

### Available Prompts

- `fix-failing-tests`: Identify and fix failing tests in the codebase
- `improve-test-coverage`: Analyze and improve test coverage
- `refactor-duplicated-code`: Find and refactor duplicated code patterns

### Execution Strategy

1. Look for prompt file at `.agpm/snippets/prompts/{name}.md` first, then `.claude/{name}.md`
2. Read entire prompt file and understand requirements
3. Execute prompt instructions with any additional arguments as context
4. Use Task tool for complex operations requiring specialized agents

2. **Load and execute the prompt**:
   - Look for the prompt file at `.agpm/snippets/prompts/{prompt-name}.md` or `.claude/{prompt-name}.md` (for backward compatibility)
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
   - Custom prompts can be added to `.agpm/snippets/prompts/` or `.claude/` directory as needed

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
1. First, verify the prompt exists (check `.agpm/snippets/prompts/{name}.md` first, then `.claude/{name}.md`) and read its content
2. Parse any special directives or requirements from the prompt
3. Create a todo list if the prompt involves multiple steps
4. Execute the prompt instructions, using appropriate tools
5. Provide clear feedback about progress and results

For complex prompts that require deep analysis:
- Delegate to specialized agents using Task tool
- Run operations in parallel when possible for efficiency
- Track progress with TodoWrite for visibility

Remember: The goal is to make commonly-used prompts easily reusable and shareable across the team.
