---
allowed-tools: Task, Bash(git log:*), Bash(git show:*), Bash(git diff:*), Bash(git reset:*), Bash(git rebase:*), Bash(git cherry-pick:*), Bash(git commit:*), Bash(git add:*), Bash(git status:*), Bash(git reflog:*), Read, Glob, Grep, TodoWrite
description: Squash commits between two hashes into one, optionally regrouping into logical commits, or restore from a previous squash
argument-hint: <from> <to> [ --regroup ] | --restore [ <reflog-entry> ] - e.g., "HEAD~5 HEAD --regroup" or "--restore" or "--restore HEAD@{3}"
---

## Context

- Current branch: !`git branch --show-current`
- Recent commits: !`git log --oneline -10`
- Current status: !`git status --short`

## Your task

Squash commits between the specified range into a single commit, with optional intelligent regrouping, or restore from a previous squash operation.

**IMPORTANT**: This command modifies git history. Ensure the branch is not shared or coordinate with team members before proceeding.

**Note**: For complex changes, delegate analysis to specialized agents using Task tool for better understanding of the codebase implications.

1. **Parse and validate arguments**:
   - Check for `--restore` flag first (restore mode)
   - If in restore mode:
     * Check for optional reflog entry (e.g., `HEAD@{3}` or `ORIG_HEAD`)
     * If no entry specified, find the most recent squash-related operation
   - Otherwise (squash mode):
     * Extract `from` commit hash (required first argument)
     * Extract `to` commit hash (required second argument)
     * Check for `--regroup` flag to enable intelligent regrouping
   - Arguments: $ARGUMENTS
   - Validate inputs based on mode

2. **Restore mode (--restore)**:

   **With specific reflog entry**:
   - If reflog entry provided (e.g., `HEAD@{3}` or `ORIG_HEAD`):
     * Validate the entry exists: `git rev-parse <entry>`
     * Show what will be restored: `git log --oneline -5 <entry>`
     * Confirm with user before proceeding
     * Execute: `git reset --hard <entry>`
     * Report success and show new HEAD

   **Without specific entry (auto-detect)**:
   - Search reflog for recent squash-related operations:
     * `git reflog --grep="rebase" --grep="reset" --grep="squash" -10`
     * Look for patterns indicating squash operations:
       - "rebase (start)" or "rebase -i (start)"
       - "reset: moving to" followed by commit refs
       - Previous HEAD positions before these operations
   - Present findings to user:
     * Show last 3-5 potential restore points
     * Include commit message and timestamp
     * Let user select which one to restore
   - Execute restoration: `git reset --hard <selected-entry>`
   - Verify: Show resulting commits and confirm changes are restored

3. **Analyze the commit range** (skip if in restore mode):
   - Get list of commits: `git log --oneline <from>..<to>`
   - Get detailed changes: `git diff <from> <to>`
   - Calculate total files changed and lines modified
   - If changes are extensive (>10 files or >500 lines), warn the user

4. **Squashing strategy** (skip if in restore mode):

**Without --regroup flag (default)**:
   - Create a single squashed commit with all changes
   - Generate commit message following project conventions from `.claude/commands/commit.md`:
     * Analyze all changes to determine commit type (feat/fix/docs/test/refactor/chore)
     * Create concise message (≤72 chars) that summarizes the overall change
     * Include a body section listing the original commits being squashed
   - Use interactive rebase or reset + commit approach:
     ```bash
     # Option 1: Interactive rebase (safer)
     git rebase -i <from>^
     # Mark all commits except first as 'squash'

     # Option 2: Reset approach (simpler)
     git reset --soft <from>
     git commit -m "type: concise summary

     Squashed commits:
     - original commit 1
     - original commit 2
     ..."
     ```

**With --regroup flag (intelligent regrouping)**:

   a. **Analyze changes for logical groupings**:
      - Use Task with subagent_type="rust-expert-standard" to analyze the changes:
        ```
        Task(description="Analyze for regrouping",
             prompt="Analyze these changes and suggest logical groupings for separate commits...",
             subagent_type="rust-expert-standard")
        ```
      - Group by these categories:
        * Feature additions (new functionality)
        * Bug fixes (corrections to existing code)
        * Documentation updates
        * Test additions/modifications
        * Refactoring (no functional changes)
        * Dependencies/build configuration
        * CI/CD workflow changes

   b. **Identify logical boundaries**:
      - Related files that should be committed together
      - Dependencies between changes
      - Atomic units of work
      - Cross-cutting concerns that span multiple files

   c. **Create staged commits**:
      - Reset to `from` commit: `git reset --soft <from>`
      - For each logical group identified:
        * Stage relevant files: `git add <files>`
        * Create commit with appropriate message (using commit.md guidelines)
        * Determine attribution per commit.md rules based on diff analysis
      - Ensure no changes are left unstaged

   d. **Example regrouping**:
      ```
      Original: 5 commits with mixed changes
      Regrouped into:
      1. feat: add new resource validation
      2. test: add validation test coverage
      3. docs: update API documentation
      ```

5. **Commit message generation** (reference `.claude/commands/commit.md`):
   - Analyze changes to determine type prefix
   - Use present tense, be concise (≤72 chars)
   - For squashed commits, include summary in body
   - Apply attribution rules from commit.md:
     * Analyze diff for AI-generated percentage
     * >50% AI: Add co-author
     * 25-50% AI: Add contribution note
     * <25% AI or tool-generated: No attribution

6. **Safety checks**:
   - Verify working directory is clean before starting
   - Git automatically saves current HEAD to `ORIG_HEAD` before rebase/reset operations
   - If operation fails or needs reverting:
     * Use `git reset --hard ORIG_HEAD` to return to pre-squash state
     * Or check `git reflog` to find the commit before squashing
     * Reset with `git reset --hard HEAD@{n}` where n is the reflog entry
   - Never force push without explicit user confirmation
   - Inform user about recovery options before starting:
     ```
     Note: Git will save your current HEAD to ORIG_HEAD.
     To undo this squash operation, run: git reset --hard ORIG_HEAD
     Or use git reflog to find and restore any previous state.
     ```

7. **Final verification**:
   - Show the resulting commit(s): `git log --stat <from>..HEAD`
   - Display total changes: `git diff <from> HEAD`
   - Confirm all original changes are preserved

Examples of usage:
- `/squash HEAD~3 HEAD` - squash last 3 commits into one
- `/squash abc123 def456` - squash commits between abc123 and def456
- `/squash HEAD~5 HEAD --regroup` - intelligently regroup last 5 commits
- `/squash feature-start HEAD --regroup` - regroup all feature branch commits
- `/squash --restore` - find and restore from most recent squash operation
- `/squash --restore ORIG_HEAD` - restore to ORIG_HEAD (last HEAD change)
- `/squash --restore HEAD@{3}` - restore to specific reflog entry