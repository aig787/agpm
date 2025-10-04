---
allowed-tools: Bash(git *), Bash(gh *), Read, Glob, Grep, TodoWrite
description: Automatically create a GitHub pull request with a well-formatted title and description
argument-hint: [ --draft ] [ --base <branch> ] [ title ] - e.g., "--draft" or "--base develop" or custom title
---

## Context

- Current branch: !`git branch --show-current`
- Git status: !`git status --short`

## Your task

Create a GitHub pull request following these guidelines:

1. Verify prerequisites:
   - Check for uncommitted changes to tracked files: `git diff --quiet && git diff --cached --quiet`
   - If there are uncommitted changes, STOP and inform the user to commit or stash them first
   - Check if branch exists on remote: `git ls-remote --heads origin $(git branch --show-current)`
   - If branch doesn't exist on remote, push with: `git push -u origin $(git branch --show-current)`
   - If branch exists, check if local is ahead: `git rev-list --count origin/$(git branch --show-current)..HEAD`
   - If local is ahead (count > 0), push with: `git push origin $(git branch --show-current)`
   - Check that `gh` CLI is installed and authenticated

2. Determine the base branch:
   - If `--base <branch>` is provided in arguments, use that
   - Otherwise, detect from remote: `git rev-parse --abbrev-ref origin/HEAD`
   - If that fails, use "main" as default

3. Parse the remaining arguments:
   - Check for `--draft` flag to create a draft PR
   - If a PR title is provided (after flags), use it (otherwise generate one)
   - Arguments: $ARGUMENTS

4. Gather PR information:
   - Get commits in this PR: `git log origin/<base-branch>..HEAD --oneline`
   - Get full diff from base: `git diff origin/<base-branch>...HEAD`
   - Replace `<base-branch>` with the base branch determined in step 2

5. Analyze all changes in the PR (NOT just the latest commit!) to determine:
   - The primary type of change: feat, fix, docs, test, refactor, chore, build, ci, perf
   - The scope (component/module affected)
   - The overall purpose and impact

6. Generate a PR title that:
   - Follows conventional commit format: `type(scope): description`
   - Uses present tense ("add" not "added")
   - Is clear and concise (ideally < 72 characters)
   - Accurately summarizes the entire PR, not just one commit

7. Generate a PR description with this structure:
   ```markdown
   ## Summary
   [2-4 bullet points summarizing what changed and why]

   ## Changes
   [Bulleted list of key changes organized by component/area]

   ## Test plan
   [Bulleted markdown checklist of steps to test the PR]
   ```

8. Create the PR:
   - Use `gh pr create` with appropriate flags
   - Include `--draft` if requested
   - Include `--base <branch>` if specified
   - Use HEREDOC for the body to ensure proper formatting:
     ```bash
     gh pr create --title "feat(scope): description" --body "$(cat <<'EOF'
     ## Summary
     ...
     EOF
     )"
     ```

9. Return the PR URL when successful

## Examples of Usage

- `/gh-pr-create` - create PR with automatic title and description
- `/gh-pr-create --draft` - create draft PR
- `/gh-pr-create --base develop` - create PR against develop branch
- `/gh-pr-create --draft --base develop` - create draft PR against develop branch
- `/gh-pr-create feat: add new feature` - create PR with custom title

## Important Notes

- Always check for uncommitted changes first - abort if any exist
- Check if branch exists on remote using `git ls-remote`, not tracking configuration (important for worktrees)
- Push branch if it doesn't exist on remote or if local is ahead
- Always analyze the FULL diff from base branch, not just the latest commit
- The PR title should summarize the entire PR, considering all commits
- Ensure the description provides clear context for reviewers
- Test plan should be actionable and specific to the changes
