# Rust Agent Shared Snippets

This directory contains shared content used by both Claude Code and OpenCode Rust agents to ensure consistency and maintainability.

## Purpose

Rather than duplicating content across multiple agent files, we maintain a single source of truth for common guidelines, best practices, and commands. Both Claude Code agents (`.claude/agents/`) and OpenCode agents (`.opencode/agent/`) reference these snippets.

## How It Works

Each agent file includes references to relevant snippets at the top:

```markdown
**IMPORTANT**: Read and follow the guidelines in these shared snippets:
- `.agpm/snippets/agents/rust-core-principles.md`
- `.agpm/snippets/agents/rust-mandatory-checks.md`
- `.agpm/snippets/agents/rust-cargo-commands.md`
```

When an AI agent reads the agent file, it will automatically read and incorporate the referenced snippet files into its understanding.

## Available Snippets

### Core Guidelines

- **`rust-core-principles.md`** - Fundamental Rust development principles
  - Idiomatic Rust patterns
  - Zero warnings policy
  - Memory safety focus
  - Error handling standards

- **`rust-mandatory-checks.md`** - Required validation steps
  - Formatting checks (cargo fmt)
  - Linting requirements (cargo clippy)
  - Test execution
  - Documentation building

### Development Tools

- **`rust-cargo-commands.md`** - Comprehensive cargo command reference
  - Development workflow commands
  - Code quality checks
  - Debugging and analysis tools
  - Testing commands
  - Coverage generation

### Code Quality

- **`rust-clippy-config.md`** - Clippy linting configuration
  - Enforced lint rules
  - Allowed exceptions
  - Rationale for choices

- **`rust-architecture-best-practices.md`** - Architecture patterns
  - Module organization
  - Error handling patterns
  - Testing strategies
  - Dependency management
  - Performance considerations
  - Async Rust patterns
  - Unsafe code guidelines

### Cross-Platform

- **`rust-cross-platform.md`** - Platform compatibility guidelines
  - Path handling
  - Testing on multiple platforms
  - Platform-specific code patterns

## Maintenance

### When to Update Snippets

Update these snippets when:
- Rust best practices evolve
- Project standards change
- New tools are adopted
- Common patterns emerge across agents

### Impact of Changes

When you update a snippet:
- ✅ All agents automatically benefit from the change
- ✅ Consistency maintained across Claude Code and OpenCode
- ✅ No need to update multiple agent files
- ✅ Single source of truth

### Adding New Snippets

To add a new shared snippet:

1. Create the snippet file in `.agpm/snippets/agents/`
2. Add clear, actionable content
3. Reference it in relevant agent files
4. Update this README

## Agent Architecture

### Dual-Platform Support

We maintain parallel agent sets for:

- **Claude Code** (`.claude/agents/*.md`) - Original Claude Code format
- **OpenCode** (`.opencode/agent/*.md`) - OpenCode-specific format

Both sets share the same core content via these snippets, with only frontmatter differences:

**Claude Code Format:**
```yaml
---
name: agent-name
description: Brief description
model: sonnet
tools: Task, Bash, Read, Write, Edit
---
```

**OpenCode Format:**
```yaml
---
description: Brief description
mode: subagent
model: anthropic/claude-sonnet-4-20250514
temperature: 0.2
tools:
  read: true
  write: true
  edit: true
  bash: true
permission:
  edit: allow
  bash: ask
---
```

### Agent Hierarchy

**Standard Tier (Fast, Sonnet/Haiku):**
- `rust-expert-standard` - General implementation
- `rust-linting-standard` - Quick formatting (Haiku)
- `rust-test-standard` - Test fixing
- `rust-troubleshooter-standard` - Common debugging
- `rust-doc-standard` - Documentation

**Advanced Tier (Complex, Opus):**
- `rust-expert-advanced` - Architecture & optimization
- `rust-linting-advanced` - Complex refactoring
- `rust-test-advanced` - Property testing, fuzzing
- `rust-troubleshooter-advanced` - Memory, UB, deep debugging
- `rust-doc-advanced` - Architectural documentation

## Best Practices

### For Snippet Authors

1. **Be Specific**: Provide actionable, concrete guidance
2. **Use Examples**: Include code snippets where helpful
3. **Stay Focused**: Each snippet should have a single, clear purpose
4. **Keep Current**: Update when tools or practices evolve
5. **Think DRY**: If multiple agents need it, it belongs in a snippet

### For Agent Authors

1. **Reference Relevant Snippets**: Include only what's needed for that agent
2. **Don't Duplicate**: If content exists in a snippet, reference it
3. **Add Context**: Explain how the snippet applies to this specific agent
4. **Maintain Differences**: Keep agent-specific content in the agent file

## Example Usage

An agent like `rust-expert-standard` might reference:
- Core principles (fundamental approach)
- Mandatory checks (validation requirements)
- Cargo commands (tool reference)
- Architecture best practices (design patterns)
- Cross-platform considerations (compatibility)

While `rust-linting-standard` might only need:
- Core principles (basic approach)
- Mandatory checks (what to validate)
- Cargo commands (specific to linting)
- Clippy config (linting rules)

This selective referencing keeps agents focused while maintaining consistency.
