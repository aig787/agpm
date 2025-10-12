# AGPM - AGentic Package Manager

> ⚠️ **Beta Software**: This project is in active development and may contain breaking changes. Use with caution in
> production environments.
> 
> 🚧 **OpenCode Support**: OpenCode integration is currently in **alpha**. While functional, it may have incomplete features or breaking changes. Claude Code support is stable and production-ready.

A Git-based package manager for AI coding assistants (Claude Code, OpenCode, and more) that enables reproducible
installations using lockfile-based dependency management, similar to Cargo. AGPM supports multiple tools through a
pluggable system, allowing you to manage resources for different AI assistants from a single manifest.

## Features

> 🚧 **OpenCode Alpha**: Multi-tool support includes OpenCode in alpha. See [Multi-Tool Support](#multi-tool-support) for details.

- 📦 **Lockfile-based dependency management** - Reproducible installations like Cargo with auto-update
- 🌐 **Git-based distribution** - Install from any Git repository (GitHub, GitLab, Bitbucket)
- 🚀 **No central registry** - Fully decentralized approach
- 🔒 **Cargo-style lockfile handling** - Auto-updates by default, strict validation with `--frozen`
- 🤖 **Multi-tool support** - Manage resources for Claude Code, OpenCode (🚧 alpha), and custom tools from one manifest
- 🔧 **Six resource types** - Agents, Snippets, Commands, Scripts, Hooks, MCP Servers
- 🎯 **Pattern-based dependencies** - Use glob patterns (`agents/*.md`, `**/*.md`) for batch installation
- 🖥️ **Cross-platform** - Windows, macOS, and Linux support with enhanced path handling
- 📁 **Local and remote sources** - Support for both Git repositories and local filesystem paths
- 🔄 **Transitive dependencies** - Resources declare dependencies in YAML/JSON, automatic graph-based resolution

## Requirements

- **Rust 1.85.0+** (MSRV for edition 2024)
- Git 2.0+ (for repository operations)

## Quick Start

### Install AGPM

**Via Homebrew (macOS and Linux):**

```bash
brew install aig787/agpm/agpm
```

**Using installer script:**

```bash
# Unix/Linux/macOS
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/aig787/agpm/releases/latest/download/agpm-installer.sh | sh

# Windows PowerShell
irm https://github.com/aig787/agpm/releases/latest/download/agpm-installer.ps1 | iex
```

**Using Cargo:**

```bash
cargo install agpm-cli                                # From crates.io
cargo binstall agpm-cli                               # Pre-built binaries (faster)
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
# Claude Code (default) - installed at .claude/agents/example.md
example-agent = { source = "community", path = "agents/example.md", version = "v1.0.0" }

# OpenCode (alpha) - installed at .opencode/agent/example.md
example-agent-oc = { source = "community", path = "agents/example.md", version = "v1.0.0", tool = "opencode" }

# Nested path (claude-code) - installed at .claude/agents/ai/assistant.md (preserves structure)
ai-assistant = { source = "community", path = "agents/ai/assistant.md", version = "v1.0.0" }

[snippets]
# AGPM shared (default) - installed at .agpm/snippets/react/*.md (each file preserves its source directory structure)
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

# Batch mode: Add multiple dependencies without installing, then install all at once
agpm add dep agent --no-install community:agents/rust-expert.md@v1.0.0
agpm add dep snippet --no-install community:snippets/utils.md@v1.0.0
agpm install  # Install all dependencies at once
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

AGPM supports **transitive dependencies** - when your dependencies have their own dependencies. Resources can declare
dependencies in their metadata, and AGPM automatically resolves the entire dependency tree.

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

Use `agpm validate --resolve --format json` or `RUST_LOG=debug agpm install` to see the exact dependency chain. Typical
fixes:

- Pin the manifest entry to a single version (`version = "v2.0.0"`) and run `agpm install` to auto-update.
- Split competing resources into separate manifests or disable the conflicting dependency in one branch.
- If a transitive dependency is too new, override it by forking the source repo or requesting an upstream fix.
- For duplicate install paths reported during expansion, add `filename` or `target` overrides so each resource installs
  cleanly.

**Circular Dependencies:**

AGPM detects and prevents circular dependencies in the dependency graph:

```text
Error: Circular dependency detected: A → B → C → A
```

**No Conflicts:**

When there are no conflicts, all dependencies are installed as requested. The system builds a complete dependency graph
and installs resources in topological order (dependencies before dependents).

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
  "events": [
    "SessionStart"
  ],
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
- Override transitive version mismatches by declaring an explicit `version` in the resource metadata or by pinning the
  parent entry in `agpm.toml`

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

## Multi-Tool Support

> 🚧 **Important Notice**: OpenCode support is currently in **alpha**. While functional, it may have incomplete features or breaking changes in future releases. Claude Code support is stable and production-ready.

AGPM supports multiple AI coding assistants through a pluggable tool system. You can manage resources for different
tools from a single manifest, enabling shared workflows and infrastructure.

### Supported Tools

- **claude-code** - Claude Code resources (agents, commands, scripts, hooks, MCP servers) ✅ **Stable**
  - Default for: agents, commands, scripts, hooks, mcp-servers
- **opencode** - OpenCode resources for agents, commands, and MCP servers 🚧 **Alpha**
  - **Note**: Alpha status - features may change. Use with caution in production.
- **agpm** - Shared snippets and templates usable across tools ✅ **Stable**
  - Default for: snippets
- **custom** - Define your own tools via configuration

### Resource Type Support Matrix

| Resource    | Claude Code                  | OpenCode (Alpha)              | AGPM                 |
|-------------|------------------------------|-------------------------------|----------------------|
| Agents      | ✅ `.claude/agents/`          | 🚧 `.opencode/agent/`          | ❌                    |
| Commands    | ✅ `.claude/commands/`        | 🚧 `.opencode/command/`        | ❌                    |
| Scripts     | ✅ `.claude/scripts/`         | ❌                             | ❌                    |
| Hooks       | ✅ `.claude/hooks/`           | ❌                             | ❌                    |
| MCP Servers | ✅ `.mcp.json`                | 🚧 `opencode.json`             | ❌                    |
| Snippets    | ✅ `.claude/agpm/snippets/`   | ❌                             | ✅ `.agpm/snippets/` (default) |

### Multi-Tool Manifest Example

```toml
[sources]
community = "https://github.com/aig787/agpm-community.git"

[agents]
# Claude Code agent (default - no tool field needed)
claude-helper = { source = "community", path = "agents/helper.md", version = "v1.0.0" }

# OpenCode agent (explicit tool field) - 🚧 Alpha feature
opencode-helper = { source = "community", path = "agents/helper.md", version = "v1.0.0", tool = "opencode" }

# Both tools can share the same source file - AGPM installs to the correct location based on tool

[snippets]
# Shared snippets (usable by both tools via references)
rust-patterns = { source = "community", path = "snippets/rust/*.md", version = "v1.0.0" }
```

### How It Works

1. **Default Behavior**:
   - **Snippets** default to `agpm` (shared infrastructure at `.agpm/snippets/`)
   - **All other resources** default to `claude-code`
2. **Explicit Routing**: Add `tool = "opencode"` or `tool = "claude-code"` to override defaults
3. **Shared Content**: Snippets use `.agpm/snippets/` by default for cross-tool sharing
4. **Tool-Specific MCP**: MCP servers automatically merge into the correct configuration file

### Example: Mixed-Tool Project

```toml
[sources]
community = "https://github.com/aig787/agpm-community.git"

[agents]
# Rust experts for both tools
rust-expert-cc = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0" }
rust-expert-oc = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0", tool = "opencode" }

[commands]
# Deployment command for Claude Code
deploy-cc = { source = "community", path = "commands/deploy.md", version = "v1.0.0" }
# Same command for OpenCode
deploy-oc = { source = "community", path = "commands/deploy.md", version = "v1.0.0", tool = "opencode" }

[mcp-servers]
# MCP servers for both tools (automatically routed to correct config file)
filesystem-cc = { source = "community", path = "mcp/filesystem.json", version = "v1.0.0" }
filesystem-oc = { source = "community", path = "mcp/filesystem.json", version = "v1.0.0", tool = "opencode" }  # 🚧 Alpha

[snippets]
# Snippets default to agpm (shared across all tools)
shared-patterns = { source = "community", path = "snippets/patterns/*.md", version = "v1.0.0" }
# No tool field needed - installs to .agpm/snippets/ by default
```

**Installation Results:**
- `rust-expert-cc` → `.claude/agents/rust-expert.md`
- `rust-expert-oc` → `.opencode/agent/rust-expert.md` (note: singular "agent") 🚧 Alpha
- `filesystem-cc` → Merged into `.mcp.json`
- `filesystem-oc` → Merged into `opencode.json` 🚧 Alpha
- `shared-patterns` → `.agpm/snippets/patterns/*.md` (shared infrastructure)

### Benefits of Multi-Tool Support

- **Unified Workflow**: Manage all AI assistant resources from one place
- **Shared Infrastructure**: Reuse common snippets and patterns across tools
- **Consistent Versioning**: Lock all tools to the same resource versions
- **Easy Migration**: Switch between tools without recreating resource infrastructure

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
| `agpm migrate`  | Migrate from legacy CCPM naming to AGPM                      |

Run `agpm --help` for full command reference.

## Resource Types

AGPM manages six types of resources that can target different AI coding assistants:

- **Agents** - AI assistant configurations (`.claude/agents/` or `.opencode/agent/` 🚧)
- **Snippets** - Reusable code templates (`.claude/agpm/snippets/` or `.agpm/snippets/`)
- **Commands** - Slash commands (`.claude/commands/` or `.opencode/command/` 🚧)
- **Scripts** - Executable automation files (`.claude/agpm/scripts/`)
- **Hooks** - Event-based automation (merged into `.claude/settings.local.json`)
- **MCP Servers** - Model Context Protocol servers (merged into `.mcp.json` or `opencode.json` 🚧)

> 🚧 **Note**: Paths marked with 🚧 indicate OpenCode alpha support. See [Multi-Tool Support](#multi-tool-support) for details on stability status.

Resources route to the appropriate directory based on the `type` field.

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
# Claude Code (default) - installed at .claude/agents/rust-expert.md
rust-expert = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0" }

# OpenCode (alpha) - installed at .opencode/agent/rust-expert.md
rust-expert-oc = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0", tool = "opencode" }

# Nested path (claude-code) - installed at .claude/agents/ai/code-reviewer.md (preserves structure)
code-reviewer = { source = "community", path = "agents/ai/code-reviewer.md", version = "v1.0.0" }

# Pattern matching (claude-code) - each file preserves its source directory structure
# agents/ai/assistant.md → .claude/agents/ai/assistant.md
# agents/ai/analyzer.md → .claude/agents/ai/analyzer.md
ai-agents = { source = "community", path = "agents/ai/*.md", version = "^1.0.0" }

[snippets]
# AGPM shared (default) - installed at .agpm/snippets/react-hooks.md
react-hooks = { source = "community", path = "snippets/react-hooks.md", version = "~1.2.0" }

# Nested pattern (agpm) - snippets/python/utils.md → .agpm/snippets/python/utils.md
python-tools = { source = "community", path = "snippets/python/*.md", version = "v1.0.0" }

[scripts]
# Claude Code (default) - installed at .claude/agpm/scripts/build.sh
build = { source = "local", path = "scripts/build.sh" }

[hooks]
# Claude Code (default) - merged into .claude/settings.local.json
pre-commit = { source = "community", path = "hooks/pre-commit.json", version = "v1.0.0" }

[mcp-servers]
# Claude Code (default) - merged into .mcp.json
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
- ✅ Automated semantic releases with conventional commits
- ✅ Cross-platform binary builds for all releases
- ✅ Publishing to crates.io

### Automated Releases

AGPM uses GitHub Actions for automated versioning and publishing:

- **Conventional Commits**: Version bumps based on commit messages (`feat:` → minor, `fix:` → patch)
- **Cross-Platform Binaries**: Automatic builds for Linux, macOS, and Windows
- **Automated Publishing**: Releases to both GitHub and crates.io

## License

MIT License - see [LICENSE.md](LICENSE.md) for details.

## Support

- 🐛 [Issue Tracker](https://github.com/aig787/agpm/issues)
- 💬 [Discussions](https://github.com/aig787/agpm/discussions)
- 📖 [Documentation](docs/user-guide.md)

---

Built with Rust 🦀 for reliability and performance
