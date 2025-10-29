---
agpm:
  templating: true
dependencies:
  snippets:
    - name: attribution
      install: false
      path: ../attribution.md
---
## Git Commit Implementation

Create git commits following these guidelines:

### Argument Semantics

- **Flags**:
  - `--multi`: Analyze changes and create optimal number of logical commits
  - `--multi=N`: Target exactly N commits (group logically)
  - `--no-attribution`: Skip all attribution notes
  - `--co-authored`: Force co-author attribution
  - `--contributed`: Force contribution note
  - `--include-untracked`: Include untracked files in commit

- **Paths**: Directory/file paths to stage (default: all tracked changes)
- **Message**: Commit message after `--` separator or as last argument

### Commit Mode Selection

- **Multi-commit mode**: Analyze changes and create logical groupings
- **Single-commit mode**: Commit all changes together

**Note**: For complex commits with extensive changes across multiple modules, delegate to specialized agents using Task:
- Use Task with subagent_type="rust-expert-standard" to review architectural implications:
  ```
  Task(description="Review commit changes",
       prompt="Review the changes for architectural implications before committing...",
       subagent_type="rust-expert-standard")
  ```
- Use Task with subagent_type="rust-linting-standard" to ensure code quality:
  ```
  Task(description="Lint before commit",
       prompt="Run linting checks to ensure code quality before committing...",
       subagent_type="rust-linting-standard")
  ```
- These agents can help ensure commits are well-structured and complete

## Commit Mode Selection

1. Check for `--multi` flag to determine commit mode:
   - **Multi-commit mode (`--multi` or `--multi=N`)**: Analyze changes and create multiple logically grouped commits
     - `--multi`: Automatically determine optimal number of commits based on change analysis
     - `--multi=N`: Target exactly N commits (where N is a positive integer)
   - **Single-commit mode (default)**: Create a single commit with all changes

### Multi-Commit Mode Process

If `--multi` flag is present:

1. **Parse the multi-commit target**:
   - If `--multi=N` format is used, extract the target number N
   - If just `--multi`, use automatic grouping based on change analysis
   - Validate that N is a positive integer if provided

2. **Analyze all changes** to identify logical groupings:
   - Group by module/component (e.g., cli, resolver, cache, tests)
   - Group by type of change (feat, fix, docs, test, refactor)
   - Consider dependencies between changes
   - Identify atomic units that make sense as separate commits

   **Grouping Guidelines**:
   - Keep related changes together (e.g., a feature and its tests)
   - Separate unrelated changes even if in the same file
   - Each commit should be buildable and pass tests independently
   - Documentation updates can be separate or combined with related code changes
   - Refactoring should typically be separate from feature changes
   - Test additions/updates can be grouped with the feature they test or separate

3. **Create logical groupings based on target**:
   - **For `--multi` (automatic)**: Create optimal number of groups based on natural logical boundaries
   - **For `--multi=N` (targeted)**:
     - If N is larger than natural groups: Split larger groups into smaller logical units
     - If N is smaller than natural groups: Combine related groups while maintaining logical coherence
     - Always prioritize logical coherence over exact count matching
     - Warn user if target count would create illogical groupings

4. **Present grouping analysis** to user:
   ```
   Suggested commit groups:
   1. feat(resolver): add centralized version resolver
      - src/resolver/version_resolver.rs
      - src/resolver/mod.rs

   2. refactor(cache): optimize worktree management
      - src/cache/mod.rs
      - src/cache/worktree.rs

   3. test: add version resolver integration tests
      - tests/integration_version_resolver.rs
   ```

5. **Get user confirmation** for the grouping:
   - Ask if they want to proceed with suggested groups
   - Allow them to adjust grouping if needed
   - Option to combine or split groups

6. **Create commits sequentially**:
   - For each group, stage only the relevant files
   - Generate appropriate commit message for each group
   - Apply attribution rules to each commit
   - Show progress after each commit

### Single-Commit Mode Process (Default)

If no `--multi` flag, proceed with standard single commit process:

2. Parse the arguments provided using this convention:
   ```
   /commit [flags] [paths...] [-- commit message]
   ```

   **Parsing Logic**:
   - Parse all `--` flags first (e.g., `--multi`, `--dry-run`, `--no-attribution`, `--include-untracked`)
   - Treat remaining arguments as paths until you hit `--` or run out of arguments
   - Everything after `--` is the commit message
   - If no `--` separator and no explicit paths, the last argument may be a commit message

   **Examples**:
   ```
   /commit .opencode                    # path only
   /commit --multi src/ tests/          # flags + paths
   /commit src/ -- feat: update cli     # paths + message
   /commit --no-attribution             # flags only
   /commit feat: quick fix              # message only
   ```

   **Arguments received**: $ARGUMENTS

3. Analyze the relevant changes and determine the commit type:
    - `feat`: New feature or functionality
    - `fix`: Bug fix
    - `docs`: Documentation changes
    - `test`: Test additions or modifications
    - `refactor`: Code refactoring without functional changes
    - `chore`: Maintenance tasks, dependency updates

4. Write a concise commit message that:
    - Starts with the type prefix (e.g., "feat:", "fix:")
    - Uses present tense ("add" not "added")
    - Is no longer than 72 characters
    - Clearly describes what changed and why

5. Handle attribution based on flags:
    - If `--no-attribution` flag is provided: Skip all attribution
    - If `--co-authored` flag is provided: Force co-author attribution
    - If `--contributed` flag is provided: Force contribution note
    - If NO attribution flags are provided: Automatically determine attribution by analyzing the diff using the logic in `Commit Attribution`
    - Briefly explain your attribution decision

6. Stage the appropriate files:
    - If `--include-untracked` flag is provided: Use `git add -A` or `git add .` to include untracked files
    - If specific paths were provided: Use `git add <path>` to stage ONLY those specific paths - NEVER stage files outside the specified paths
    - Default behavior (no `--include-untracked`): Use `git add -u` to stage only tracked files with changes
    - **CRITICAL**: When paths are specified, ONLY stage files matching those exact paths. Do not stage "related" files outside specified paths.
    - Never include untracked files unless `--include-untracked` is explicitly provided

6.1. **Validation Step** (when paths are specified):
    - After staging, run `git status --porcelain` to verify staged files
    - Check if any staged files are outside the specified paths
    - If found, run `git reset` and restage only the correct paths
    - This ensures you only commit exactly what was requested

7. Create the commit with the formatted message and appropriate attribution

## Examples of Usage

### Single Commit Examples

- `/commit` - commits tracked changes only with automatic attribution detection
- `/commit --include-untracked` - commits all changes including untracked files
- `/commit --co-authored` - commits tracked changes with explicit co-author attribution
- `/commit --contributed tests/` - commits tests directory with explicit contribution note
- `/commit --no-attribution` - commits tracked changes with no attribution
- `/commit --include-untracked --co-authored` - commits all files including untracked with co-author
- `/commit --co-authored -- fix: resolve test failures` - commits with specified message and co-author
- `/commit --no-attribution -- fix: manual bugfix` - commits with specified message and no attribution
- `/commit tests/` - commits specific directory with automatic attribution detection
- `/commit --include-untracked tests/` - commits specific directory including untracked files
- `/commit -- fix: update dependencies` - commits with specified message and automatic attribution
- `/commit .opencode` - commits only changes in .opencode directory
- `/commit src/ tests/ -- feat: update cli and add tests` - commits specific paths with custom message

### Multi-Commit Examples

- `/commit --multi` - analyzes all changes and creates optimal number of logical commits
- `/commit --multi=3` - creates exactly 3 commits based on logical groupings
- `/commit --multi --co-authored` - creates multiple commits with co-author attribution
- `/commit --multi=2 --no-attribution` - creates 2 commits without attribution
- `/commit --multi --include-untracked` - creates multiple commits including untracked files
- `/commit --multi=4 --co-authored` - creates 4 commits with co-author attribution
- `/commit --multi src/ tests/` - creates multiple commits for specific directories only

### Multi-Commit Workflow Example

```
User: /commit --multi=3

Claude: Analyzing changes to create 3 logical commit groups...

Suggested commit groups:

1. docs: update command documentation for multi-commit support
   Files:
   - .claude/commands/commit.md

2. feat(commands): add update-all command for parallel documentation updates
   Files:
   - .claude/commands/update-all.md
   - Makefile

3. refactor(resolver): centralize version resolution logic
   Files:
   - src/resolver/version_resolver.rs (new)
   - src/resolver/mod.rs
   - src/resolver/version_resolution.rs

These 3 commits match your target count. Would you like to proceed? (yes/no/adjust)

User: yes

Claude: Creating commit 1/3: docs: update command documentation...
[Creates first commit]
✓ Commit 1 created

Creating commit 2/3: feat(commands): add update-all command...
[Creates second commit]
✓ Commit 2 created

Creating commit 3/3: refactor(resolver): centralize version resolution...
[Creates third commit]
✓ Commit 3 created

All 3 commits created successfully!
```

### Path-Specific Examples

```
User: /commit .opencode

Claude: Staging changes in .opencode directory...
[Stages only .opencode/** files]
✓ Committed: docs: update opencode agents and commands

User: /commit src/ tests/ -- feat: improve cli error handling

Claude: Staging changes in src/ and tests/ directories...
[Stages src/** and tests/** files]
✓ Committed: feat: improve cli error handling
```

# Commit Attribution
{{ agpm.deps.snippets.attribution.content }}
# TEST MODIFICATION
