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

   **Consider using checkpoint restore first**:
   - Check if checkpoints are available by loading `.claude/commands/checkpoint.md` list command
   - If recent checkpoint exists from squash operation, suggest using `/checkpoint restore latest`
   - Checkpoints provide cleaner restoration with better context preservation

   **With specific reflog entry**:
   - If reflog entry provided (e.g., `HEAD@{3}` or `ORIG_HEAD`):
     * Validate the entry exists: `git rev-parse <entry>`
     * Show what will be restored: `git log --oneline -5 <entry>`
     * Confirm with user before proceeding
     * Execute: `git reset --hard <entry>`
     * Report success and show new HEAD

   **Without specific entry (auto-detect)**:
   - First check for available checkpoints (preferred method)
   - Search reflog for recent squash-related operations:
     * `git reflog --grep="rebase" --grep="reset" --grep="squash" -10`
     * Look for patterns indicating squash operations:
       - "rebase (start)" or "rebase -i (start)"
       - "reset: moving to" followed by commit refs
       - Previous HEAD positions before these operations
   - Present findings to user:
     * Show available checkpoints first (if any)
     * Show last 3-5 potential restore points from reflog
     * Include commit message and timestamp
     * Let user select which one to restore
   - Execute restoration: `git reset --hard <selected-entry>` or checkpoint restore
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
   - Analyze the code diff to determine appropriate attribution:
     * Review the actual changes being squashed: `git diff <from> <to>`
     * Apply attribution analysis from `.agpm/snippets/attribution.md` to determine AI contribution percentage
   - Use reset + commit approach:
     ```bash
     git reset --soft <from>
     # Analyze changes and determine attribution need
     git diff --cached
     git commit -m "$(cat <<'EOF'
     type: concise summary

     Squashed commits:
     - original commit 1
     - original commit 2
     ...

     [Include attribution only if analysis shows >25% AI contribution]
     EOF
     )"
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

   c. **Create staged commits with analyzed attribution**:
      - Reset to `from` commit: `git reset --mixed <from>` (use mixed to allow proper staging)
      - For each logical group identified:
        * Stage relevant files: `git add <files>`
        * Analyze the diff to determine AI contribution: `git diff --cached`
        * Apply attribution analysis and thresholds from `.agpm/snippets/attribution.md`
        * Create commit with appropriate message and attribution:
          ```
          git commit -m "$(cat <<'EOF'
          type: concise summary

          [Additional body if needed]

          [Attribution based on analysis - only if warranted]
          EOF
          )"
          ```
      - Ensure no changes are left unstaged

   d. **Example regrouping**:
      ```
      Original: 5 commits with mixed changes
      Regrouped into:
      1. feat: add new resource validation
      2. test: add validation test coverage
      3. docs: update API documentation
      ```

5. **Commit message generation and attribution**:
   - Analyze changes to determine type prefix (feat/fix/docs/test/refactor/chore)
   - Use present tense, be concise (≤72 chars)
   - For squashed commits, include summary in body
   - **Apply attribution based on code analysis**:
     * Analyze the actual diff using logic and thresholds from `.agpm/snippets/attribution.md`
     * The squashing operation itself doesn't require attribution - only the actual code changes matter
     * Example attribution when warranted:
       ```
       🤖 Generated with [Claude Code](https://claude.ai/code)

       Co-Authored-By: Claude <noreply@anthropic.com>
       ```

6. **Safety checks**:
   - Verify working directory is clean before starting
   - **Create a checkpoint before squashing**:
     * Load and execute `.claude/commands/checkpoint.md` create command
     * Use message: "Before squash operation from <from> to <to>"
     * This provides an additional safety net beyond Git's ORIG_HEAD
   - Git automatically saves current HEAD to `ORIG_HEAD` before rebase/reset operations
   - If operation fails or needs reverting:
     * Use `/checkpoint restore latest` to return to pre-squash state
     * Or use `git reset --hard ORIG_HEAD` to return to pre-squash state
     * Or check `git reflog` to find the commit before squashing
     * Reset with `git reset --hard HEAD@{n}` where n is the reflog entry
   - Never force push without explicit user confirmation
   - Inform user about recovery options before starting:
     ```
     Note: A checkpoint will be created before squashing.
     To undo this squash operation, run: /checkpoint restore latest
     Alternatively, use: git reset --hard ORIG_HEAD
     Or use git reflog to find and restore any previous state.
     ```

7. **Final verification**:
   - Show the resulting commit(s): `git log --stat <from>..HEAD`
   - Display total changes: `git diff <from> HEAD`
   - Confirm all original changes are preserved

Examples of usage:
- `/squash HEAD~3 HEAD` - squash last 3 commits into one (creates checkpoint first)
- `/squash abc123 def456` - squash commits between abc123 and def456 (creates checkpoint first)
- `/squash HEAD~5 HEAD --regroup` - intelligently regroup last 5 commits (creates checkpoint first)
- `/squash feature-start HEAD --regroup` - regroup all feature branch commits (creates checkpoint first)
- `/squash --restore` - find and restore from most recent squash operation (checks checkpoints first)
- `/squash --restore ORIG_HEAD` - restore to ORIG_HEAD (last HEAD change)
- `/squash --restore HEAD@{3}` - restore to specific reflog entry
- `/checkpoint restore latest` - alternative way to undo last squash if checkpoint was created