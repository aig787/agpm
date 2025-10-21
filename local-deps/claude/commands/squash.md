---
allowed-tools: Task, Bash(git log:*), Bash(git show:*), Bash(git diff:*), Bash(git reset:*), Bash(git rebase:*), Bash(git cherry-pick:*), Bash(git commit:*), Bash(git add:*), Bash(git status:*), Bash(git reflog:*), Read, Glob, Grep, TodoWrite
description: Squash commits between two hashes into one, optionally regrouping into logical commits, or restore from a previous squash
argument-hint: <from> <to> [ --regroup ] | --restore [ <reflog-entry> ] - e.g., "HEAD~5 HEAD --regroup" or "--restore" or "--restore HEAD@{3}"
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/squash.md
---

{{ agpm.deps.snippets.base.content }}

## Context

- Current branch: !`git branch --show-current`
- Recent commits: !`git log --oneline -10`
- Current status: !`git status --short`

## Tool-Specific Notes

- This command is designed for Claude Code
- Use the Task tool and allowed-tools from frontmatter
