# AGPM - Claude Code Package Manager

> ⚠️ **Beta Software**: This project is in active development and may contain breaking changes. Use with caution in
> production environments.

A Git-based package manager for Claude Code resources that enables reproducible installations using lockfile-based
dependency management, similar to Cargo.

## Features

- 📦 **Lockfile-based dependency management** - Reproducible installations like Cargo with auto-update
- 🌐 **Git-based distribution** - Install from any Git repository (GitHub, GitLab, Bitbucket)
- 🚀 **No central registry** - Fully decentralized approach
- 🔒 **Cargo-style lockfile handling** - Auto-updates by default, strict validation with `--frozen`
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

### Install AGPM

**Using installer script (Recommended):**

```bash
# Unix/Linux/macOS
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/aig787/agpm/releases/latest/download/agpm-installer.sh | sh

# Windows PowerShell
irm https://github.com/aig787/agpm/releases/latest/download/agpm-installer.ps1 | iex
```

**Using Cargo:**

```bash
cargo install agpm                                    # From crates.io
cargo binstall agpm                                   # Pre-built binaries (faster)
cargo install --git https://github.com/aig787/agpm.git  # Latest development
```

For more installation options, see the [Installation Guide](docs/installation.md).

### Create a Project

```bash
# Initialize a new AGPM project
agpm init

# Or manually create agpm.toml:
```

```toml
[sources]
community = "https://github.com/aig787/agpm-community.git"

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
# Install all dependencies (auto-updates lockfile like Cargo)
agpm install

# Use exact lockfile versions (for CI/CD - like cargo build --locked)
agpm install --frozen

# Control parallelism (default: max(10, 2 × CPU cores))
agpm install --max-parallel 8

# Bypass cache for fresh installation
agpm install --no-cache
```

### Adding Dependencies

```bash
# Add a Git source repository
agpm add source community https://github.com/aig787/agpm-community.git

# Add dependencies from Git sources
agpm add dep agent community:agents/rust-expert.md@v1.0.0
agpm add dep snippet community:snippets/react.md --name react-utils

# Add local file dependencies
agpm add dep agent ./local-agents/helper.md --name my-helper
agpm add dep script ../shared/scripts/build.sh

# Add pattern dependencies (bulk installation)
agpm add dep agent "community:agents/ai/*.md@v1.0.0" --name ai-agents
```

### Dependency Validation

AGPM provides comprehensive validation and automatic conflict resolution:

```bash
# Basic manifest validation
agpm validate

# Full validation with all checks
agpm validate --resolve --sources --paths --check-lock

# JSON output for CI/CD integration
agpm validate --format json
```

#### Transitive Dependencies and Conflict Resolution

AGPM supports **transitive dependencies** - when your dependencies have their own dependencies. Resources can declare dependencies in their metadata, and AGPM automatically resolves the entire dependency tree.

**What is a Conflict?**

A conflict occurs when the same resource (same source and path) is required at different versions:
- **Direct conflict**: Your manifest requires `helper.md@v1.0.0` and `helper.md@v2.0.0`
- **Transitive conflict**: Agent A depends on `helper.md@v1.0.0`, Agent B depends on `helper.md@v2.0.0`

**Automatic Resolution Strategy:**

When conflicts are detected, AGPM automatically resolves them:
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

**When Auto-Resolution Fails:**

If constraints have no compatible version, installation stops with an error similar to:

```text
Error: Version conflict for agents/helper.md
  requested: v1.0.0 (manifest)
  requested: v2.0.0 (transitive via agents/deploy.md)
  resolution: no compatible tag satisfies both constraints
```

Use `agpm validate --resolve --format json` or `RUST_LOG=debug agpm install` to see the exact dependency chain. Typical fixes:
- Pin the manifest entry to a single version (`version = "v2.0.0"`) and run `agpm install` to auto-update.
- Split competing resources into separate manifests or disable the conflicting dependency in one branch.
- If a transitive dependency is too new, override it by forking the source repo or requesting an upstream fix.
- For duplicate install paths reported during expansion, add `filename` or `target` overrides so each resource installs cleanly.

**Circular Dependencies:**

AGPM detects and prevents circular dependencies in the dependency graph:
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
- Override transitive version mismatches by declaring an explicit `version` in the resource metadata or by pinning the parent entry in `agpm.toml`

**Lockfile Format:**

Dependencies are tracked in `agpm.lock` using the format `resource_type/name@version`:
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
| `agpm init`     | Initialize a new project                                     |
| `agpm install`  | Install dependencies from agpm.toml with parallel processing |
| `agpm update`   | Update dependencies within version constraints               |
| `agpm outdated` | Check for available updates to installed dependencies        |
| `agpm upgrade`  | Self-update AGPM to the latest version                       |
| `agpm list`     | List installed resources                                     |
| `agpm validate` | Validate manifest and dependencies                           |
| `agpm add`      | Add sources or dependencies                                  |
| `agpm remove`   | Remove sources or dependencies                               |
| `agpm config`   | Manage global configuration                                  |
| `agpm cache`    | Manage the Git cache                                         |

Run `agpm --help` for full command reference.

## Resource Types

AGPM manages six types of resources:

- **Agents** - AI assistant configurations (`.claude/agents/`)
- **Snippets** - Reusable code templates (`.claude/agpm/snippets/`)
- **Commands** - Claude Code slash commands (`.claude/commands/`)
- **Scripts** - Executable automation files (`.claude/agpm/scripts/`)
- **Hooks** - Event-based automation (merged into `.claude/settings.local.json`)
- **MCP Servers** - Model Context Protocol servers (merged into `.mcp.json`)

## Documentation

- 📚 **[Installation Guide](docs/installation.md)** - All installation methods and requirements
- 🚀 **[User Guide](docs/user-guide.md)** - Getting started and basic workflows
- 📖 **[Command Reference](docs/command-reference.md)** - Complete command syntax and options
- 🔧 **[Resources Guide](docs/resources.md)** - Working with different resource types
- 🔢 **[Versioning Guide](docs/versioning.md)** - Version constraints and Git references
- ⚙️ **[Configuration Guide](docs/configuration.md)** - Global config and authentication
- 🗂️ **[Manifest Reference](docs/manifest-reference.md)** - Field-by-field manifest schema and CLI mapping
- 🏗️ **[Architecture](docs/architecture.md)** - Technical details and design decisions
- ❓ **[FAQ](docs/faq.md)** - Frequently asked questions
- 🐛 **[Troubleshooting](docs/troubleshooting.md)** - Common issues and solutions

## Example Project

```toml
# agpm.toml
[sources]
community = "https://github.com/aig787/agpm-community.git"
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
# Single file - installed at .claude/agpm/snippets/react-hooks.md
react-hooks = { source = "community", path = "snippets/react-hooks.md", version = "~1.2.0" }

# Nested pattern - snippets/python/utils.md → .claude/agpm/snippets/python/utils.md
python-tools = { source = "community", path = "snippets/python/*.md", version = "v1.0.0" }

[scripts]
build = { source = "local", path = "scripts/build.sh" }

[hooks]
pre-commit = { source = "community", path = "hooks/pre-commit.json", version = "v1.0.0" }

[mcp-servers]
filesystem = { source = "community", path = "mcp/filesystem.json", version = "latest" }
```

## Performance Architecture

AGPM v0.3.2+ features a high-performance SHA-based architecture:

### Centralized Version Resolution

- **VersionResolver**: Batch resolution of all dependency versions to commit SHAs
- **Minimal Git Operations**: Single fetch per repository per command
- **Upfront Resolution**: All versions resolved before any worktree operations

### SHA-Based Worktree Deduplication

- **Commit-Level Caching**: Worktrees keyed by commit SHA, not version reference
- **Maximum Reuse**: Multiple tags/branches pointing to same commit share one worktree
- **Parallel Safety**: Independent worktrees enable conflict-free concurrent operations

## Versioning

AGPM uses Git-based versioning at the repository level with enhanced constraint support:

- **Git tags** (recommended): `version = "v1.0.0"` or `version = "^1.0.0"`
- **Semver constraints**: `^1.0`, `~2.1`, `>=1.0.0, <2.0.0`
- **Branches**: `branch = "main"` (mutable, updates on each install)
- **Commits**: `rev = "abc123def"` (immutable, exact commit)
- **Local paths**: No versioning, uses current files

See the [Versioning Guide](docs/versioning.md) for details.

## Security

AGPM separates credentials from project configuration:

- ✅ **Project manifest** (`agpm.toml`) - Safe to commit
- ❌ **Global config** (`~/.agpm/config.toml`) - Contains secrets, never commit

```bash
# Add private source with authentication (global config only)
agpm config add-source private "https://oauth2:TOKEN@github.com/org/private.git"
```

## Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

## Project Status

AGPM is actively developed with comprehensive test coverage and automated releases:

- ✅ All core commands implemented
- ✅ Cross-platform support (Windows, macOS, Linux)
- ✅ Comprehensive test suite (70%+ coverage)
- ✅ Specialized Rust development agents (standard/advanced tiers)
- ✅ Automated semantic releases with conventional commits
- ✅ Cross-platform binary builds for all releases
- ✅ Publishing to crates.io (automated via semantic-release)

### Automated Releases

AGPM uses [semantic-release](https://semantic-release.gitbook.io/) for automated versioning and publishing:

- **Conventional Commits**: Version bumps based on commit messages (`feat:` → minor, `fix:` → patch)
- **Cross-Platform Binaries**: Automatic builds for Linux, macOS, and Windows
- **Automated Publishing**: Releases to both GitHub and crates.io
- **Changelog Generation**: Automatic changelog from commit history

## License

MIT License - see [LICENSE.md](LICENSE.md) for details.

## Support

- 🐛 [Issue Tracker](https://github.com/aig787/agpm/issues)
- 💬 [Discussions](https://github.com/aig787/agpm/discussions)
- 📖 [Documentation](docs/user-guide.md)

---

Built with Rust 🦀 for reliability and performance
