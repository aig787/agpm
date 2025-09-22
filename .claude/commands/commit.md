---
allowed-tools: Task, Bash(git add:*), Bash(git status:*), Bash(git diff:*), Bash(git commit:*), Bash(git log:*), Bash(git show:*), Read, Glob, Grep, TodoWrite
description: Create a well-formatted git commit following project conventions
argument-hint: [ --co-authored | --contributed | --no-attribution | --include-untracked ] [ paths... ] [ message ] - e.g., "tests/" or "--co-authored fix: update dependencies"
---

## Context

- Current git status: !`git status --short`
- Current git diff: !`git diff HEAD`
- Recent commits for style reference: !`git log --oneline -5`

## Your task

Based on the changes shown above, create a single git commit following these guidelines:

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

1. Parse the arguments provided:
    - Check for attribution flags: `--co-authored`, `--contributed`, or `--no-attribution`
    - Check for `--include-untracked` flag to include untracked files (default: exclude untracked)
    - If paths are specified (e.g., "tests/", ".github/"), only stage and commit changes in those paths
    - If a commit message is provided, use it (otherwise generate one)
    - Arguments: $ARGUMENTS

2. Analyze the relevant changes and determine the commit type:
    - `feat`: New feature or functionality
    - `fix`: Bug fix
    - `docs`: Documentation changes
    - `test`: Test additions or modifications
    - `refactor`: Code refactoring without functional changes
    - `chore`: Maintenance tasks, dependency updates

3. Write a concise commit message that:
    - Starts with the type prefix (e.g., "feat:", "fix:")
    - Uses present tense ("add" not "added")
    - Is no longer than 72 characters
    - Clearly describes what changed and why

4. Handle attribution based on flags:
    - If `--no-attribution` flag is provided: Skip all attribution
    - If `--co-authored` flag is provided: Force co-author attribution
    - If `--contributed` flag is provided: Force contribution note
    - If NO attribution flags are provided: Automatically determine attribution by analyzing the diff using the logic in `.claude/snippets/attribution.md`
    - Briefly explain your attribution decision

5. Stage the appropriate files:
    - If `--include-untracked` flag is provided: Use `git add -A` or `git add .` to include untracked files
    - If specific paths were provided: Use `git add <path>` to stage only those paths
    - Default behavior (no `--include-untracked`): Use `git add -u` to stage only tracked files with changes
    - Never include untracked files unless `--include-untracked` is explicitly provided

6. Create the commit with the formatted message and appropriate attribution

Examples of usage:

- `/commit` - commits tracked changes only with automatic attribution detection
- `/commit --include-untracked` - commits all changes including untracked files
- `/commit --co-authored` - commits tracked changes with explicit co-author attribution
- `/commit --contributed tests/` - commits tests directory with explicit contribution note
- `/commit --no-attribution` - commits tracked changes with no attribution
- `/commit --include-untracked --co-authored` - commits all files including untracked with co-author
- `/commit --co-authored fix: resolve test failures` - commits with specified message and co-author
- `/commit --no-attribution fix: manual bugfix` - commits with specified message and no attribution
- `/commit tests/` - commits specific directory with automatic attribution detection
- `/commit --include-untracked tests/` - commits specific directory including untracked files
- `/commit fix: update dependencies` - commits with specified message and automatic attribution