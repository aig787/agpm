# CCPM - Claude Code Package Manager

> ⚠️ **Beta Software**: This project is in active development and may contain breaking changes. Use with caution in
> production environments.

A Git-based package manager for Claude Code resources that enables reproducible installations using lockfile-based
dependency management, similar to Cargo.

## Features

- 📦 **Lockfile-based dependency management** - Reproducible installations like Cargo/npm with staleness detection
- 🌐 **Git-based distribution** - Install from any Git repository (GitHub, GitLab, Bitbucket)
- 🚀 **No central registry** - Fully decentralized approach
- 🔒 **Lockfile staleness detection** - Automatic detection of outdated or inconsistent lockfiles
- 🔧 **Six resource types** - Agents, Snippets, Commands, Scripts, Hooks, MCP Servers
- 🎯 **Pattern-based dependencies** - Use glob patterns (`agents/*.md`, `**/*.md`) for batch installation
- 🧹 **Automatic artifact cleanup** - Old files removed when paths change
- ⚠️ **Path conflict detection** - Prevents multiple dependencies from overwriting the same file
- 🖥️ **Cross-platform** - Windows, macOS, and Linux support with enhanced path handling
- 📁 **Local and remote sources** - Support for both Git repositories and local filesystem paths
- 🔄 **Transitive dependencies** - Resources declare dependencies in YAML/JSON, automatic graph-based resolution
- 🛡️ **Circular dependency detection** - Prevents circular references with comprehensive error reporting
- 🧩 **Intelligent conflict resolution** - Automatic version resolution with transparent logging

## Quick Start

### Install CCPM

**Using installer script (Recommended):**

```bash
# Unix/Linux/macOS
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/aig787/ccpm/releases/latest/download/ccpm-installer.sh | sh

# Windows PowerShell
irm https://github.com/aig787/ccpm/releases/latest/download/ccpm-installer.ps1 | iex
```

**Using Cargo:**

```bash
cargo install ccpm                                    # From crates.io
cargo binstall ccpm                                   # Pre-built binaries (faster)
cargo install --git https://github.com/aig787/ccpm.git  # Latest development
```

For more installation options, see the [Installation Guide](docs/installation.md).

### Create a Project

```bash
# Initialize a new CCPM project
ccpm init

# Or manually create ccpm.toml:
```

```toml
[sources]
community = "https://github.com/aig787/ccpm-community.git"

[agents]
# Single file - installed at .claude/agents/example.md
example-agent = { source = "community", path = "agents/example.md", version = "v1.0.0" }

# Nested path - installed at .claude/agents/ai/assistant.md (preserves structure)
ai-assistant = { source = "community", path = "agents/ai/assistant.md", version = "v1.0.0" }

[snippets]
# Pattern - each file preserves its source directory structure
react-utils = { source = "community", path = "snippets/react/*.md", version = "^1.0.0" }
```

### Install Dependencies

```bash
# Install all dependencies and generate lockfile
ccpm install

# Use exact lockfile versions (for CI/CD)
ccpm install --frozen

# Force installation when lockfile is stale
ccpm install --force

# Regenerate lockfile from scratch
ccpm install --regenerate

# Control parallelism (default: max(10, 2 × CPU cores))
ccpm install --max-parallel 8

# Bypass cache for fresh installation
ccpm install --no-cache
```

### Adding Dependencies

```bash
# Add a Git source repository
ccpm add source community https://github.com/aig787/ccpm-community.git

# Add dependencies from Git sources
ccpm add dep agent community:agents/rust-expert.md@v1.0.0
ccpm add dep snippet community:snippets/react.md --name react-utils

# Add local file dependencies
ccpm add dep agent ./local-agents/helper.md --name my-helper
ccpm add dep script ../shared/scripts/build.sh

# Add pattern dependencies (bulk installation)
ccpm add dep agent "community:agents/ai/*.md@v1.0.0" --name ai-agents
```

### Dependency Validation

CCPM provides comprehensive validation and automatic conflict resolution:

```bash
# Basic manifest validation
ccpm validate

# Full validation with all checks
ccpm validate --resolve --sources --paths --check-lock

# JSON output for CI/CD integration
ccpm validate --format json
```

#### Transitive Dependencies and Conflict Resolution

CCPM supports **transitive dependencies** - when your dependencies have their own dependencies. Resources can declare dependencies in their metadata, and CCPM automatically resolves the entire dependency tree.

**What is a Conflict?**

A conflict occurs when the same resource (same source and path) is required at different versions:
- **Direct conflict**: Your manifest requires `helper.md@v1.0.0` and `helper.md@v2.0.0`
- **Transitive conflict**: Agent A depends on `helper.md@v1.0.0`, Agent B depends on `helper.md@v2.0.0`

**Automatic Resolution Strategy:**

When conflicts are detected, CCPM automatically resolves them:
1. **Specific over "latest"**: If one version is specific and another is "latest", use the specific version
2. **Higher version**: When both are specific versions, use the higher version
3. **Transparent logging**: All conflict resolutions are logged for visibility

**Example Conflict Resolution:**
```text
Direct dependencies:
  - app-agent requires helper.md v1.0.0
  - tool-agent requires helper.md v2.0.0
→ Resolved: Using helper.md v2.0.0 (higher version)

Transitive dependencies:
  - agent-a → depends on → helper.md v1.5.0
  - agent-b → depends on → helper.md v2.0.0
→ Resolved: Using helper.md v2.0.0 (higher version)
```

**Circular Dependencies:**

CCPM detects and prevents circular dependencies in the dependency graph:
```text
Error: Circular dependency detected: A → B → C → A
```

**No Conflicts:**

When there are no conflicts, all dependencies are installed as requested. The system builds a complete dependency graph and installs resources in topological order (dependencies before dependents).

See the [Command Reference](docs/command-reference.md#add-dependency) for all supported dependency formats.

### Declaring Dependencies in Resource Files

Resources can declare their own dependencies within their files, creating a complete dependency graph:

**Markdown files (.md)** use YAML frontmatter:
```markdown
---
title: My Agent
description: An example agent with dependencies
dependencies:
  agents:
    - path: agents/helper.md
      version: v1.0.0
  snippets:
    - path: snippets/utils.md
      version: v2.0.0
---

# Agent content here
```

**JSON files (.json)** use a top-level `dependencies` field:
```json
{
  "events": ["SessionStart"],
  "type": "command",
  "command": "echo 'Starting session'",
  "dependencies": {
    "commands": [
      {
        "path": "commands/setup.md",
        "version": "v1.0.0"
      }
    ]
  }
}
```

**Key Features:**
- Dependencies inherit the source from their parent resource
- Version is optional - defaults to parent's version if not specified
- Supports all resource types: agents, snippets, commands, scripts, hooks, mcp-servers
- Graph-based resolution with topological ordering ensures correct installation order
- Circular dependency detection prevents infinite loops

**Lockfile Format:**

Dependencies are tracked in `ccpm.lock` using the format `resource_type/name@version`:
```toml
[[commands]]
name = "my-command"
path = "commands/my-command.md"
dependencies = [
    "agents/helper@v1.0.0",
    "snippets/utils@v2.0.0"
]
```

## Core Commands

| Command         | Description                                                  |
|-----------------|--------------------------------------------------------------|
| `ccpm init`     | Initialize a new project                                     |
| `ccpm install`  | Install dependencies from ccpm.toml with parallel processing |
| `ccpm update`   | Update dependencies within version constraints               |
| `ccpm outdated` | Check for available updates to installed dependencies        |
| `ccpm upgrade`  | Self-update CCPM to the latest version                       |
| `ccpm list`     | List installed resources                                     |
| `ccpm validate` | Validate manifest and dependencies                           |
| `ccpm add`      | Add sources or dependencies                                  |
| `ccpm remove`   | Remove sources or dependencies                               |
| `ccpm config`   | Manage global configuration                                  |
| `ccpm cache`    | Manage the Git cache                                         |

Run `ccpm --help` for full command reference.

## Resource Types

CCPM manages six types of resources:

- **Agents** - AI assistant configurations (`.claude/agents/`)
- **Snippets** - Reusable code templates (`.claude/ccpm/snippets/`)
- **Commands** - Claude Code slash commands (`.claude/commands/`)
- **Scripts** - Executable automation files (`.claude/ccpm/scripts/`)
- **Hooks** - Event-based automation (merged into `.claude/settings.local.json`)
- **MCP Servers** - Model Context Protocol servers (merged into `.mcp.json`)

## Documentation

- 📚 **[Installation Guide](docs/installation.md)** - All installation methods and requirements
- 🚀 **[User Guide](docs/user-guide.md)** - Getting started and basic workflows
- 📖 **[Command Reference](docs/command-reference.md)** - Complete command syntax and options
- 🔧 **[Resources Guide](docs/resources.md)** - Working with different resource types
- 🔢 **[Versioning Guide](docs/versioning.md)** - Version constraints and Git references
- ⚙️ **[Configuration Guide](docs/configuration.md)** - Global config and authentication
- 🏗️ **[Architecture](docs/architecture.md)** - Technical details and design decisions
- ❓ **[FAQ](docs/faq.md)** - Frequently asked questions
- 🐛 **[Troubleshooting](docs/troubleshooting.md)** - Common issues and solutions

## Example Project

```toml
# ccpm.toml
[sources]
community = "https://github.com/aig787/ccpm-community.git"
local = "./local-resources"

[agents]
# Single file - installed at .claude/agents/rust-expert.md
rust-expert = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0" }

# Nested path - installed at .claude/agents/ai/code-reviewer.md (preserves structure)
code-reviewer = { source = "community", path = "agents/ai/code-reviewer.md", version = "v1.0.0" }

# Pattern matching - each file preserves its source directory structure
# agents/ai/assistant.md → .claude/agents/ai/assistant.md
# agents/ai/analyzer.md → .claude/agents/ai/analyzer.md
ai-agents = { source = "community", path = "agents/ai/*.md", version = "^1.0.0" }

[snippets]
# Single file - installed at .claude/ccpm/snippets/react-hooks.md
react-hooks = { source = "community", path = "snippets/react-hooks.md", version = "~1.2.0" }

# Nested pattern - snippets/python/utils.md → .claude/ccpm/snippets/python/utils.md
python-tools = { source = "community", path = "snippets/python/*.md", version = "v1.0.0" }

[scripts]
build = { source = "local", path = "scripts/build.sh" }

[hooks]
pre-commit = { source = "community", path = "hooks/pre-commit.json", version = "v1.0.0" }

[mcp-servers]
filesystem = { source = "community", path = "mcp/filesystem.json", version = "latest" }
```

## Performance Architecture

CCPM v0.3.2+ features a high-performance SHA-based architecture:

### Centralized Version Resolution

- **VersionResolver**: Batch resolution of all dependency versions to commit SHAs
- **Minimal Git Operations**: Single fetch per repository per command
- **Upfront Resolution**: All versions resolved before any worktree operations

### SHA-Based Worktree Deduplication

- **Commit-Level Caching**: Worktrees keyed by commit SHA, not version reference
- **Maximum Reuse**: Multiple tags/branches pointing to same commit share one worktree
- **Parallel Safety**: Independent worktrees enable conflict-free concurrent operations

## Versioning

CCPM uses Git-based versioning at the repository level with enhanced constraint support:

- **Git tags** (recommended): `version = "v1.0.0"` or `version = "^1.0.0"`
- **Semver constraints**: `^1.0`, `~2.1`, `>=1.0.0, <2.0.0`
- **Branches**: `branch = "main"` (mutable, updates on each install)
- **Commits**: `rev = "abc123def"` (immutable, exact commit)
- **Local paths**: No versioning, uses current files

See the [Versioning Guide](docs/versioning.md) for details.

## Security

CCPM separates credentials from project configuration:

- ✅ **Project manifest** (`ccpm.toml`) - Safe to commit
- ❌ **Global config** (`~/.ccpm/config.toml`) - Contains secrets, never commit

```bash
# Add private source with authentication (global config only)
ccpm config add-source private "https://oauth2:TOKEN@github.com/org/private.git"
```

## Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

## Project Status

CCPM is actively developed with comprehensive test coverage and automated releases:

- ✅ All core commands implemented
- ✅ Cross-platform support (Windows, macOS, Linux)
- ✅ Comprehensive test suite (70%+ coverage)
- ✅ Specialized Rust development agents (standard/advanced tiers)
- ✅ Automated semantic releases with conventional commits
- ✅ Cross-platform binary builds for all releases
- ✅ Publishing to crates.io (automated via semantic-release)

### Automated Releases

CCPM uses [semantic-release](https://semantic-release.gitbook.io/) for automated versioning and publishing:

- **Conventional Commits**: Version bumps based on commit messages (`feat:` → minor, `fix:` → patch)
- **Cross-Platform Binaries**: Automatic builds for Linux, macOS, and Windows
- **Automated Publishing**: Releases to both GitHub and crates.io
- **Changelog Generation**: Automatic changelog from commit history

## License

MIT License - see [LICENSE.md](LICENSE.md) for details.

## Support

- 🐛 [Issue Tracker](https://github.com/aig787/ccpm/issues)
- 💬 [Discussions](https://github.com/aig787/ccpm/discussions)
- 📖 [Documentation](docs/user-guide.md)

---

Built with Rust 🦀 for reliability and performance