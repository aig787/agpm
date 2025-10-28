## Docstring Update Implementation

Review the current code changes and ensure all related documentation (doc comments, module docs, CLAUDE.md) accurately reflects the implementation.

**CRITICAL**: Use the Task tool to delegate to specialized documentation agents for comprehensive updates. Do NOT attempt to update extensive documentation manually.

### Argument Semantics

- **Flags**:
  - `--check-only`: Only report documentation issues without making changes
  - `--auto-update`: Update documentation to match code changes (default)

### Analysis Focus Areas

**IMPORTANT**: Validate docstrings for ANY changed code, regardless of the type of change. All code modifications should trigger documentation review.

Analyze git diff for documentation-relevant code changes:
- New functions, structs, or modules added
- Modified function signatures or behavior
- Removed or deprecated functionality
- Changed error types or handling
- Modified data structures or APIs
- New or changed configuration options
- Algorithm or logic changes
- **Any other code modifications** - ALL changes require documentation validation

### Documentation Agent Delegation

For comprehensive documentation updates:
- Use `rust-doc-standard` for standard docstring updates
- Use `rust-doc-advanced` for complex architectural documentation
- Agents handle adding missing docs, updating examples, and ensuring accuracy

2. Analyze the git diff to identify code changes and validate documentation:

   **Step 1: Identify Changed Functions**
   - Parse git diff to find all functions that have been modified
   - For each changed function, locate and review its docstring for:
     * **Accuracy**: Ensure the docstring correctly describes what the code currently does
     * **Up-to-date**: Verify the docstring reflects the specific changes made in this commit
     * **Parameter/return documentation**: Check that all parameters and return types are documented correctly
     * **Examples**: Ensure any code examples compile and demonstrate current behavior

   **Step 2: Identify Changed Modules**
   - Parse git diff to find all modules containing changed code
   - For each changed module, review the module-level documentation (top-level `///` or '//!' comments) for:
     * **Accuracy**: Ensure the module description correctly reflects the module's current purpose and responsibilities
     * **Up-to-date**: Verify the module docs incorporate the specific changes made in this commit
     * **Completeness**: Check that new exported items are mentioned if relevant to the module's purpose

   **Step 3: Documentation Validation Checklist**
   - **Function-level validation**:
     * Docstring exists and is accurate for changed functions
     * Function signature matches docstring parameters
     * Return type documentation is correct
     * Error conditions are documented if function returns Result
     * Examples use current API and compile
     * Deprecated functions have proper deprecation notices

   - **Module-level validation**:
     * Module docs reflect current responsibilities
     * New major functionality is mentioned in module overview
     * Removed functionality is no longer referenced
     * Cross-references to other modules are still accurate

   **For comprehensive documentation updates, delegate to specialized agents using Task:**
   - Use Task with appropriate subagent_type:
     ```
     Task(description="Update code documentation",
          prompt="Review the git diff and validate documentation for ALL changed code:

          **For each changed function:**
          1. Locate the function's docstring and verify it accurately describes what the code currently does
          2. Ensure the docstring reflects the specific changes made in this commit
          3. Check that all parameters and return types are documented correctly
          4. Verify examples (if any) use the current API and compile
          5. Update any inaccurate or outdated information

          **For each module containing changed code:**
          1. Review module-level documentation for accuracy with current implementation
          2. Ensure module docs reflect the specific changes made in this commit
          3. Update cross-references and remove mentions of removed functionality
          4. Add mentions of new significant functionality where appropriate

          **Documentation standards - BE CONCISE:**
          - Use minimal words to convey meaning clearly
          - Avoid redundant information (e.g., don't repeat the function name in the description)
          - Prefer one-sentence descriptions when possible
          - Keep examples focused and minimal
          - Remove verbose explanations that don't add value
          - Avoid stating the obvious (e.g., "This function returns..." for return types)

          **General documentation standards:**
          - Follow Rust documentation conventions
          - Use ```no_run for code examples by default unless they should be executed as tests
          - Document all public APIs thoroughly but concisely
          - Focus on what the user needs to know, not implementation details

          Focus specifically on the changed functions and modules identified in the git diff.",
          subagent_type="rust-doc-standard")
     ```
   - For complex architectural documentation, use subagent_type="rust-doc-advanced"
   - The agent will systematically validate docstrings for every changed function and module docs for every changed module

3. Review documentation accuracy for changed code:

   **Documentation types to check**:
   - **Doc comments (///)**: Function, struct, and module documentation
   - **Inline comments (//)**: Implementation detail explanations
   - **Module documentation**: Top-level module descriptions
   - **CLAUDE.md**: Architecture decisions and module descriptions
   - **Error messages**: User-facing error documentation
   - **Examples in docs**: Code examples in documentation

4. Identify documentation issues based on changes:

   **Common documentation problems**:
   - Doc comments describing old behavior
   - Missing documentation for new public APIs
   - Outdated parameter descriptions
   - Incorrect return value documentation
   - Stale examples that won't compile
   - Module docs not reflecting new responsibilities
   - CLAUDE.md architecture section outdated
   - Error variants without documentation

5. Apply updates based on mode:

   **Check-only mode (--check-only)**:
   - List all documentation discrepancies found
   - Show specific lines needing updates
   - Highlight missing documentation
   - Report outdated or incorrect information
   - Suggest documentation improvements

   **Auto-update mode (--auto-update or default)**:
   - Use Task to delegate to `rust-doc-standard` for regular docs or `rust-doc-advanced` for architectural documentation
   - Update doc comments to match implementation
   - Add missing documentation for public items
   - **Ensure docstrings are concise: brief but informative, avoid verbosity**
   - Fix parameter and return value descriptions
   - Update examples to compile with current code
   - Correct module-level documentation
   - Update CLAUDE.md if architecture changed
   - Ensure error messages are documented

6. Focus areas for AGPM project:

   **Critical documentation to maintain**:
   - **Public API documentation**: All pub functions and structs
   - **Error handling**: ErrorContext and user-facing errors
   - **Cross-platform behavior**: Platform-specific code paths
   - **Security considerations**: Input validation and safety checks
   - **Resource types**: Documentation for each resource type
   - **CLI commands**: Argument and behavior documentation
   - **Test requirements**: Test isolation and environment setup

7. Documentation standards to enforce:

   **Rust documentation conventions**:
   - Start doc comments with brief description (one sentence preferred)
   - **Be concise: Use minimal words to convey meaning clearly**
   - **Avoid redundant information (e.g., don't repeat the function name)**
   - Use `# Examples` section for code examples
   - Document all public items
   - Use `# Panics` section if function can panic
   - Use `# Errors` section for Result-returning functions
   - Use `# Safety` section for unsafe code
   - Include parameter descriptions with backticks
   - Ensure examples compile (use ```no_run if needed)

8. Documentation applies to ALL modules in the codebase:
   - No module is excluded from documentation requirements
   - Both core modules (cli, resolver, lockfile, manifest) and supporting modules (utils, git, cache) require proper documentation
   - Test modules should document test helpers and utilities
   - **ANY change to ANY code in ANY module triggers documentation validation**

9. Quality checks:

    **Documentation quality criteria**:
    - Accuracy: Matches current implementation
    - Completeness: All public APIs documented
    - Clarity: Easy to understand
    - **Conciseness: Brief but informative - avoid verbosity and redundancy**
    - Consistency: Uniform style and terminology
    - Examples: Working code examples where helpful
    - Cross-references: Links between related items

Examples of changes requiring doc updates:
- **ANY code change → Validate related documentation**
- New public function → Add /// doc comment
- Changed function behavior → Update doc comment
- New error variant → Document in Error enum
- Modified algorithm → Update implementation comments
- New module → Add module-level documentation
- Architecture change → Update CLAUDE.md
- New test requirements → Document in test module
- Internal refactoring → Validate affected docstrings
- Variable rename → Update references in documentation
- Comment-only changes → Validate consistency with code

Examples of usage:
- `/update-docstrings` - automatically update docstrings based on code changes (all modules)
- `/update-docstrings --check-only` - report documentation issues without changes (all modules)
