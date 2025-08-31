---
allowed-tools: Bash(git add:*), Bash(git status:*), Bash(git diff:*), Bash(git commit:*), Bash(git log:*)
description: Create a well-formatted git commit following project conventions
argument-hint: [ --co-authored | --contributed | --no-attribution ] [ paths... ] [ message ] - e.g., "tests/" or "--co-authored fix: update dependencies"
---

## Context

- Current git status: !`git status --short`
- Current git diff: !`git diff HEAD`
- Recent commits for style reference: !`git log --oneline -5`

## Your task

Based on the changes shown above, create a single git commit following these guidelines:

1. Parse the arguments provided:
    - Check for attribution flags: `--co-authored`, `--contributed`, or `--no-attribution`
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

4. Handle attribution:
    - If `--no-attribution` flag is provided, skip all attribution (no co-author or contribution note)
    - If `--co-authored` or `--contributed` flag is explicitly provided, use that
    - If NO attribution flags are provided, automatically determine based on AI contribution:
        * Analyze the diff to estimate AI-generated percentage using these indicators:

          **Strong AI indicators (high weight):**
            - New files with 100+ lines of boilerplate/template code
            - Comprehensive documentation blocks with consistent formatting
            - Systematic error handling across multiple functions
            - Complete test suites with edge cases
            - Multi-language configurations (CI/CD workflows, Docker, etc.)
            - Repetitive patterns with consistent naming conventions

          **Mixed indicators (medium weight):**
            - Refactoring with consistent style changes
            - Adding type definitions or interfaces
            - Implementing standard patterns (singleton, factory, etc.)
            - Configuration updates with detailed comments

          **Human indicators (negative weight):**
            - Single-line fixes or small tweaks (<5 lines)
            - Business-specific logic or domain knowledge
            - Hotfixes addressing specific bugs
            - Custom regex patterns or complex conditionals
            - TODO comments or debugging code
            - Inconsistent formatting or style
            - Trial-and-error patterns (multiple similar attempts)

          **Contextual analysis:**
            - Check file history: new files vs modifications
            - Line count ratio: added vs modified vs deleted
            - Complexity: simple changes vs architectural additions
            - Consistency: uniform style suggests AI generation
            - Completeness: AI tends to handle edge cases comprehensively

        * Apply attribution based on percentage:
            - > 50% AI-generated: Add co-author attribution
              ```
              Co-authored-by: Claude <noreply@anthropic.com>
              ```
            - 25-50% AI-generated: Add contribution note
              ```
              🤖 Generated with Claude assistance
              ```
            - <25% AI-generated: No attribution
    - Briefly explain your attribution decision (e.g., "~70% AI-generated content, adding co-author")

5. Stage the appropriate files:
    - If specific paths were provided, only stage those paths
    - Otherwise, stage all tracked files with changes (avoid untracked files)
    - Use `git add <path>` for specific paths or `git add -u` for all tracked files
    - Never use `git add -A` to avoid accidentally committing untracked files

6. Create the commit with the formatted message and appropriate attribution

Examples of usage:

- `/commit` - commits all changes with automatic attribution detection
- `/commit --co-authored` - commits all changes with explicit co-author attribution
- `/commit --contributed tests/` - commits tests directory with explicit contribution note
- `/commit --no-attribution` - commits all changes with no attribution
- `/commit --co-authored fix: resolve test failures` - commits with specified message and co-author
- `/commit --no-attribution fix: manual bugfix` - commits with specified message and no attribution
- `/commit tests/` - commits specific directory with automatic attribution detection
- `/commit fix: update dependencies` - commits with specified message and automatic attribution