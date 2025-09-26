# CCPM - Claude Code Package Manager

> ⚠️ **Beta Software**: This project is in active development and may contain breaking changes. Use with caution in
> production environments.

A Git-based package manager for Claude Code resources that enables reproducible installations using lockfile-based
dependency management, similar to Cargo.

## Features

- 📦 **Lockfile-based dependency management** - Reproducible installations like Cargo/npm
- 🌐 **Git-based distribution** - Install from any Git repository (GitHub, GitLab, Bitbucket)
- 🚀 **No central registry** - Fully decentralized approach
- 🔧 **Six resource types** - Agents, Snippets, Commands, Scripts, Hooks, MCP Servers
- 🎯 **Pattern-based dependencies** - Use glob patterns (`agents/*.md`, `**/*.md`) for batch installation
- 🖥️ **Cross-platform** - Windows, macOS, and Linux support with enhanced path handling
- 📁 **Local and remote sources** - Support for both Git repositories and local filesystem paths

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
example-agent = { source = "community", path = "agents/example.md", version = "v1.0.0" }

[snippets]
react-utils = { source = "community", path = "snippets/react/*.md", version = "^1.0.0" }
```

### Install Dependencies

```bash
# Install all dependencies and generate lockfile
ccpm install

# Use exact lockfile versions (for CI/CD)
ccpm install --frozen

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

See the [Command Reference](docs/command-reference.md#add-dependency) for all supported dependency formats.

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
# Single file
rust-expert = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0" }

# Pattern matching - install multiple files
ai-agents = { source = "community", path = "agents/ai/*.md", version = "^1.0.0" }

[snippets]
react-hooks = { source = "community", path = "snippets/react-hooks.md", version = "~1.2.0" }

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