---
title: Git Auto Commit
description: Automated git commit with semantic commit messages
author: CCPM Team
version: 1.0.0
tags:
  - git
  - automation
  - productivity
---

# Git Auto Commit

Automatically stage changes and create semantic commit messages based on file changes.

## Usage

This command analyzes your changes and creates appropriate commit messages following conventional commit standards.

```bash
# Auto-commit all changes
git-auto-commit

# Auto-commit with type prefix
git-auto-commit --type feat

# Dry run to preview commit message
git-auto-commit --dry-run
```

## Features

- Analyzes changed files to determine commit type (feat, fix, docs, etc.)
- Groups related changes intelligently
- Follows conventional commit format
- Supports interactive mode for message editing
- Validates commit message length and format

## Configuration

You can configure default behavior in your git config:

```bash
git config autocommit.type feat
git config autocommit.scope frontend
```

## Examples

```bash
# After modifying README.md
$ git-auto-commit
[main abc1234] docs: update README with usage instructions

# After fixing a bug in src/api.js
$ git-auto-commit
[main def5678] fix(api): resolve null pointer in response handler

# After adding new feature
$ git-auto-commit --type feat
[main ghi9012] feat: add user authentication module
```