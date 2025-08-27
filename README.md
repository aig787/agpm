# CCPM - Claude Code Package Manager

A Git-based package manager for Claude Code resources that enables reproducible installations using lockfile-based
dependency management, similar to Cargo.

## Overview

CCPM (Claude Code Package Manager) is a command-line tool written in Rust that simplifies the deployment and management
of Claude agents and snippets across different projects. Using a manifest-based approach with lockfile support, CCPM
provides reproducible, version-controlled installations of AI resources directly from Git repositories.

## Features

- **Lockfile-Based Management**: Reproducible installations using `ccpm.toml` + `ccpm.lock` (similar to Cargo)
- **Git-based Distribution**: Install resources directly from any Git repository (GitHub, GitLab, Bitbucket, etc.)
- **No Central Registry**: Fully decentralized - add any Git repository as a source
- **Dependency Resolution**: Automatic version constraint resolution with conflict detection
- **Cross-Platform Support**: Works reliably on Windows, macOS, and Linux
- **Fast & Reliable**: Written in Rust for performance and safety
- **Parallel Operations**: Automatic parallel processing for faster installation and updates
- **Comprehensive CLI**: Full-featured command-line interface with 8 commands

## Resource Types

CCPM supports multiple types of Claude Code resources:

### Agents
AI assistant configurations with prompts and behavioral definitions. Installed to `.claude/agents/ccpm/` by default.

### Snippets
Reusable code templates and documentation fragments. Installed to `.claude/ccpm/snippets/` by default.

### Scripts
Executable files (.sh, .js, .py, etc.) that can be run by hooks or independently for automation tasks. Installed to `.claude/ccpm/scripts/` by default.

### Hooks
Event-based automation configurations for Claude Code. JSON files that define when to run scripts based on tool usage or other events (PreToolUse, PostToolUse, UserPromptSubmit, etc.). Installed to `.claude/ccpm/hooks/` and automatically merged into Claude Code settings.

### MCP Servers
Model Context Protocol servers that extend Claude Code's capabilities with external tools and APIs. Configured in `.mcp.json`.

## Installation

```bash
# Install via cargo
cargo install ccpm

# Or download pre-built binary
curl -L https://github.com/aig787/ccpm/releases/latest/download/ccpm-$(uname -s)-$(uname -m) -o ccpm
chmod +x ccpm
sudo mv ccpm /usr/local/bin/

# Or build from source
git clone https://github.com/aig787/ccpm.git
cargo install --path ccpm
```

## Quick Start

### 1. Create a Project Manifest

Create a `ccpm.toml` file in your project root (or use `ccpm init` to generate a template):

```toml
# Define sources - Git repositories or local directories
[sources]
community = "https://github.com/aig787/ccpm-community.git"
local-deps = "./dependencies"  # Local directory (no Git required)

# Optional: Customize installation directories
# [target]
# agents = "path/to/agents"
# snippets = "path/to/snippets"

# Agents
[agents]
example-agent = { source = "community", path = "agents/example.md", version = "v1.0.0" }
local-agent = { source = "local-deps", path = "agents/helper.md" }  # No version for local paths

# Snippets
[snippets]
example-snippet = { source = "community", path = "snippets/example.md", version = "v1.0.0" }
local-snippet = { source = "local-deps", path = "snippets/utils.md" }

# Scripts - Executable files for automation
[scripts]
security-check = { source = "community", path = "scripts/security.sh", version = "v1.0.0" }
validation = { source = "community", path = "scripts/validate.py", version = "v1.0.0" }

# Hooks - Event-based automation for Claude Code
[hooks]
pre-bash = { source = "community", path = "hooks/pre-bash.json", version = "v1.0.0" }
post-tool = { source = "community", path = "hooks/post-tool.json", version = "v1.0.0" }

# MCP Servers
[mcp-servers]
filesystem = { command = "npx", args = ["-y", "@modelcontextprotocol/server-filesystem"] }
postgres = { command = "mcp-postgres", args = ["--connection", "${DATABASE_URL}"] }
```

### 2. Install Dependencies

```bash
# Install all dependencies and generate lockfile
ccpm install

# Output:
# 📦 Installing dependencies from ccpm.toml...
# ✅ Resolved 2 dependencies
# ✅ Installed example-agent v1.0.0
# ✅ Installed example-snippet v1.0.0
# ✅ Generated ccpm.lock
```

This creates:

- `ccpm.lock` - Lockfile with exact resolved versions
- `.claude/agents/ccpm/` - Directory with installed agent files (default location)
- `.claude/ccpm/snippets/` - Directory with installed snippet files (default location)

### 3. Basic Workflows

```bash
# Update dependencies within version constraints
ccpm update

# List installed resources
ccpm list

# Validate manifest and lockfile
ccpm validate

# Use exact lockfile versions (for CI/CD)
ccpm install --frozen
```

## Core Commands

CCPM provides 8 commands for managing dependencies:

| Command    | Purpose                                                       | Key Options                                           |
|------------|---------------------------------------------------------------|-------------------------------------------------------|
| `init`     | Initialize a new ccpm.toml manifest file                      | `--name`, `--empty`                                   |
| `add`      | Add sources or dependencies to ccpm.toml                      | `source`, `dep`                                       |
| `install`  | Install dependencies from ccpm.toml, generate/update lockfile | `--frozen`, `--no-cache`, `--force`, `--max-parallel` |
| `update`   | Update dependencies within version constraints                | `--dry-run`, `--max-parallel`                         |
| `list`     | Show installed resources from lockfile or manifest            | `--manifest`, `--format json`                         |
| `validate` | Validate ccpm.toml syntax and check dependencies              | `--resolve`, `--check-lock`                           |
| `cache`    | Manage the global git cache                                   | `clean`, `clean --all`, `info`                        |
| `config`   | Manage global configuration                                   | `init`, `show`, `edit`, `add-source`, `list-sources`  |
| `mcp`      | Manage MCP (Model Context Protocol) servers                   | `list`, `clean`, `status`                              |

### Command Examples

#### Initialize a new project

```bash
# Create a manifest with example content
ccpm init

# Create an empty manifest
ccpm init --empty

# Specify project name
ccpm init --name "my-project"
```

#### Add dependencies dynamically

```bash
# Add a source repository
ccpm add source community https://github.com/aig787/ccpm-community.git

# Add an agent dependency
ccpm add dep agents example-agent --source community --path agents/example.md --version v1.0.0

# Add a snippet dependency
ccpm add dep snippets util-snippet --source community --path snippets/util.md --version v1.0.0
```

#### Manage MCP servers

```bash
# List all MCP servers (shows which are CCPM-managed)
ccpm mcp list

# Check MCP configuration status
ccpm mcp status

# Remove CCPM-managed servers from .mcp.json
ccpm mcp clean
```

## Manifest Format

### ccpm.toml Structure

```toml
# Sources can be Git repositories or local directories
[sources]
community = "https://github.com/aig787/ccpm-community.git"
private = "git@github.com:mycompany/private-agents.git"
local = "./local-resources"  # Local directory (no Git required)

# Target directories (optional - defaults shown)
[target]
agents = ".claude/agents/ccpm"      # Where to install agents
snippets = ".claude/ccpm/snippets"   # Where to install snippets
scripts = ".claude/ccpm/scripts"     # Where to install scripts
hooks = ".claude/ccpm/hooks"         # Where to install hooks

# Agents
[agents]
example-agent = { source = "community", path = "agents/example.md", version = "v1.0.0" }
custom-agent = { source = "private", path = "agents/custom.md", version = "v2.1.0" }

# Snippets
[snippets]
example-snippet = { source = "community", path = "snippets/example.md", version = "v1.2.0" }
helper = { source = "community", path = "snippets/helper.md", version = "v1.0.0" }

# Scripts - Executable files for automation
[scripts]
security = { source = "community", path = "scripts/security.sh", version = "v1.0.0" }
validator = { source = "private", path = "scripts/validate.py", version = "v2.0.0" }

# Hooks - Event-based automation configurations
[hooks]
pre-bash = { source = "community", path = "hooks/pre-bash.json", version = "v1.0.0" }
validation = { source = "private", path = "hooks/validation.json", version = "v1.1.0" }
```

### Dependency Specification

Dependencies can be specified in multiple ways:

```toml
# Exact version (with or without 'v' prefix)
agent1 = { source = "community", path = "agents/agent1.md", version = "1.0.0" }
agent2 = { source = "community", path = "agents/agent2.md", version = "v1.0.0" }

# Semver ranges (standard cargo/npm-style version constraints)
agent3 = { source = "community", path = "agents/agent3.md", version = "^1.2.0" }  # Compatible: 1.2.0, 1.3.0, etc. (not 2.0.0)
agent4 = { source = "community", path = "agents/agent4.md", version = "~1.2.0" }  # Patch only: 1.2.0, 1.2.1, etc. (not 1.3.0)
agent5 = { source = "community", path = "agents/agent5.md", version = ">=1.0.0" } # At least 1.0.0
agent6 = { source = "community", path = "agents/agent6.md", version = ">=1.0.0, <2.0.0" } # Range

# Special keywords
latest-agent = { source = "community", path = "agents/latest.md", version = "latest" }  # Latest stable (no pre-releases)
beta-agent = { source = "community", path = "agents/beta.md", version = "latest-prerelease" }  # Latest including pre-releases
any-agent = { source = "community", path = "agents/any.md", version = "*" }  # Any version

# Git references
dev-agent = { source = "community", path = "agents/dev.md", branch = "main" }
fixed-agent = { source = "community", path = "agents/fixed.md", rev = "abc123def" }

# Local file (no source needed, no version support)
local-agent = { path = "../local-agents/helper.md" }
```

#### Version Range Syntax

CCPM supports standard semantic versioning (semver) ranges, following the same conventions as Cargo and npm:

| Syntax              | Example                       | Matches              | Description           |
|---------------------|-------------------------------|----------------------|-----------------------|
| `1.2.3` or `v1.2.3` | `version = "1.2.3"`           | Exactly 1.2.3        | Both formats accepted |
| `^1.2.3`            | `version = "^1.2.3"`          | >=1.2.3, <2.0.0      | Compatible releases   |
| `~1.2.3`            | `version = "~1.2.3"`          | >=1.2.3, <1.3.0      | Patch releases only   |
| `>=1.2.3`           | `version = ">=1.2.3"`         | Any version >= 1.2.3 | Minimum version       |
| `>1.2.3`            | `version = ">1.2.3"`          | Any version > 1.2.3  | Greater than          |
| `<=1.2.3`           | `version = "<=1.2.3"`         | Any version <= 1.2.3 | Maximum version       |
| `<1.2.3`            | `version = "<1.2.3"`          | Any version < 1.2.3  | Less than             |
| `>=1.0.0, <2.0.0`   | `version = ">=1.0.0, <2.0.0"` | 1.x.x versions       | Complex ranges        |
| `*`                 | `version = "*"`               | Any version          | Wildcard              |
| `latest`            | `version = "latest"`          | Latest stable        | Excludes pre-releases |

## Versioning in CCPM

CCPM uses Git-based versioning, which means version constraints apply at the repository level, not individual files. Understanding how versioning works is crucial for effectively managing dependencies.

### How Versioning Works

1. **Repository-Level Versioning**: When you specify a version (e.g., `version = "v1.0.0"`), you're referencing a Git tag on the entire repository, not individual files. All resources from that repository at that tag share the same version.

2. **Git References**: CCPM supports multiple ways to reference specific points in a repository's history:
   - **Git Tags** (recommended): Semantic versions like `v1.0.0`, `v2.1.3`
   - **Git Branches**: Branch names like `main`, `develop`, `feature/xyz`
   - **Git Commits**: Specific commit hashes like `abc123def`
   - **Special Keywords**: `latest` (newest tag), `*` (any version)

3. **No Versioning for Local Directories**: Local directory sources and direct file paths don't support versioning because they're not Git repositories. They always use the current state of the files.

### Version Reference Types

#### Git Tags (Recommended)

Git tags are the primary versioning mechanism in CCPM. They provide stable, semantic version numbers:

```toml
[agents]
# Exact version using a git tag
stable-agent = { source = "community", path = "agents/example.md", version = "v1.0.0" }

# Version ranges using semantic versioning
compatible-agent = { source = "community", path = "agents/helper.md", version = "^1.2.0" }  # 1.2.0 to <2.0.0
patch-only-agent = { source = "community", path = "agents/util.md", version = "~1.2.3" }    # 1.2.3 to <1.3.0
```

**How it works**: When CCPM resolves `version = "v1.0.0"`, it:
1. Looks for a Git tag named `v1.0.0` in the repository
2. Checks out the repository at that tag
3. Retrieves the specified file from that tagged state

#### Git Branches

Branches reference the latest commit on a specific branch:

```toml
[agents]
# Track the main branch (updates with each install/update)
dev-agent = { source = "community", path = "agents/dev.md", branch = "main" }

# Track a feature branch
feature-agent = { source = "community", path = "agents/new.md", branch = "feature/new-capability" }
```

**Important**: Branch references are mutable - they update to the latest commit each time you run `ccpm update`. Use tags for stable, reproducible builds.

#### Git Commit Hashes

For absolute reproducibility, reference specific commits:

```toml
[agents]
# Pin to exact commit (immutable)
fixed-agent = { source = "community", path = "agents/stable.md", rev = "abc123def456" }
```

**Use cases**:
- Debugging specific versions
- Pinning to commits between releases
- Maximum reproducibility when tags aren't available

#### Local Resources (No Versioning)

Local resources don't support versioning because they're not in Git:

```toml
[sources]
# Local directory source - no Git, no versions
local-deps = "./dependencies"

[agents]
# ✅ VALID - Local source without version
local-agent = { source = "local-deps", path = "agents/helper.md" }

# ❌ INVALID - Can't use version with local directory source
# bad-agent = { source = "local-deps", path = "agents/helper.md", version = "v1.0.0" }  # ERROR!

# Direct file path - also no version support
direct-agent = { path = "../agents/my-agent.md" }

# ❌ INVALID - Can't use version with direct path
# bad-direct = { path = "../agents/my-agent.md", version = "v1.0.0" }  # ERROR!
```

### Version Resolution Process

When CCPM installs dependencies, it follows this resolution process:

1. **Parse Version Constraint**: Interpret the version specification (tag, range, branch, etc.)
2. **Fetch Repository Metadata**: Get list of tags/branches from the Git repository
3. **Match Versions**: Find all versions that satisfy the constraint
4. **Select Best Match**: Choose the highest version that satisfies constraints
5. **Lock Version**: Record the exact commit hash in `ccpm.lock`

### Version Constraints and Ranges

CCPM supports sophisticated version constraints using semantic versioning:

| Constraint | Example | Resolves To | Use Case |
|------------|---------|-------------|----------|
| Exact | `"1.2.3"` or `"v1.2.3"` | Exactly version 1.2.3 | Pinning to specific release |
| Caret | `"^1.2.3"` | >=1.2.3, <2.0.0 | Allow compatible updates |
| Tilde | `"~1.2.3"` | >=1.2.3, <1.3.0 | Allow patch updates only |
| Greater | `">1.2.3"` | Any version >1.2.3 | Minimum version requirement |
| Range | `">=1.0.0, <2.0.0"` | 1.x.x versions | Complex constraints |
| Latest | `"latest"` | Newest stable tag | Always use newest stable |
| Wildcard | `"*"` | Any version | No constraints |

### Lockfile and Reproducibility

The `ccpm.lock` file ensures reproducible installations by recording:

```toml
[[agents]]
name = "example-agent"
source = "community"
path = "agents/example.md"
version = "v1.0.0"                    # Original constraint
resolved_commit = "abc123def..."      # Exact commit hash
resolved_version = "v1.0.0"           # Actual tag/version used
```

Key points:
- **Original constraint** (`version`): What was requested in `ccpm.toml`
- **Resolved commit** (`resolved_commit`): Exact Git commit hash
- **Resolved version** (`resolved_version`): The tag/branch that was resolved

### Version Selection Examples

Given available tags: `v1.0.0`, `v1.1.0`, `v1.2.0`, `v2.0.0`

| Constraint | Selected Version | Explanation |
|------------|------------------|-------------|
| `"^1.0.0"` | `v1.2.0` | Highest 1.x.x version |
| `"~1.0.0"` | `v1.0.0` | Only 1.0.x allowed |
| `">=1.1.0, <2.0.0"` | `v1.2.0` | Highest within range |
| `"latest"` | `v2.0.0` | Newest stable tag |
| `">1.0.0"` | `v2.0.0` | Highest available |

### Best Practices for Versioning

1. **Use Semantic Version Tags**: Tag releases with semantic versions (`v1.0.0`, `v2.1.3`)
2. **Prefer Tags Over Branches**: Tags are immutable; branches change over time
3. **Use Caret Ranges**: `^1.0.0` allows compatible updates while preventing breaking changes
4. **Lock for Production**: Commit `ccpm.lock` and use `--frozen` flag in CI/CD
5. **Document Breaking Changes**: Use major version bumps (v1.x.x → v2.x.x) for breaking changes
6. **Test Before Updating**: Use `ccpm update --dry-run` to preview changes

### Common Versioning Scenarios

#### Scenario 1: Development vs Production

```toml
# Development - track latest changes
[agents.dev]
cutting-edge = { source = "community", path = "agents/new.md", branch = "main" }

# Production - stable versions only
[agents]
stable = { source = "community", path = "agents/proven.md", version = "^1.0.0" }
```

#### Scenario 2: Gradual Updates

```toml
# Start conservative
agent = { source = "community", path = "agents/example.md", version = "~1.2.0" }  # Patches only

# After testing, allow minor updates
agent = { source = "community", path = "agents/example.md", version = "^1.2.0" }  # Compatible updates

# Eventually, allow any 1.x version
agent = { source = "community", path = "agents/example.md", version = ">=1.2.0, <2.0.0" }
```

#### Scenario 3: Mixed Sources

```toml
[sources]
stable-repo = "https://github.com/org/stable-resources.git"     # Tagged releases
dev-repo = "https://github.com/org/dev-resources.git"           # Active development
local = "./local-resources"                                     # Local directory

[agents]
production = { source = "stable-repo", path = "agents/prod.md", version = "v2.1.0" }  # Specific tag
experimental = { source = "dev-repo", path = "agents/exp.md", branch = "develop" }    # Branch tracking
workspace = { source = "local", path = "agents/wip.md" }                             # No version (local)
```

### Local Dependencies

#### 1. Local Directory Sources (NEW)

You can use local directories as sources without requiring Git. This is perfect for development and testing:

```toml
[sources]
# Local directory as a source - no Git required
local-deps = "./dependencies"
shared-resources = "../shared-resources"
absolute-local = "/home/user/ccpm-resources"

[agents]
# Dependencies from local directory sources don't need versions
local-agent = { source = "local-deps", path = "agents/helper.md" }
shared-agent = { source = "shared-resources", path = "agents/common.md" }
```

**Security Note**: For security, local paths are restricted to:
- Within the current project directory
- Within the CCPM cache directory (`~/.ccpm/cache`)
- Within `/tmp` for testing

#### 2. Direct File Paths (Legacy)

You can reference individual `.md` files directly without a source:

```toml
# Direct file paths - NO version support
local-agent = { path = "../agents/my-agent.md" }
local-snippet = { path = "./snippets/util.md" }

# ❌ INVALID - versions not allowed for direct paths
# local-versioned = { path = "../agents/agent.md", version = "v1.0.0" }  # ERROR!
```

#### 3. Local Git Repositories (file:// URLs)

Use `file://` URLs in sources to reference local git repositories with full git functionality:

```toml
[sources]
# Local git repository with full version support
local-repo = "file:///home/user/my-git-repo"

[agents]
# Can use versions, branches, tags with local git repos
local-git-agent = { source = "local-repo", path = "agents/agent.md", version = "v1.0.0" }
local-branch-agent = { source = "local-repo", path = "agents/dev.md", branch = "develop" }
```

**Important Notes:**

- Plain directory paths (`../`, `./`, `/`) are for simple file references only
- `file://` URLs must point to valid git repositories (containing `.git` directory)
- Plain paths as sources are NOT allowed - sources must be git repositories

## Lockfile Format

The `ccpm.lock` file tracks exact resolved versions:

```toml
# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "community"
url = "https://github.com/aig787/ccpm-community.git"
commit = "abc123def..."
fetched_at = "2024-01-01T00:00:00Z"

[[agents]]
name = "example-agent"
source = "community"
path = "agents/example.md"
version = "v1.0.0"
resolved_commit = "abc123def..."
checksum = "sha256:abcdef..."
installed_at = "agents/example-agent.md"
```

## MCP Server Support

CCPM can manage MCP (Model Context Protocol) server configurations for Claude Code. MCP servers provide integrations with external systems like databases, APIs, and development tools.

### Configuring MCP Servers

Define MCP servers in your `ccpm.toml`:

```toml
[mcp-servers]
# NPX-based server
filesystem = {
    command = "npx",
    args = ["-y", "@modelcontextprotocol/server-filesystem", "--root", "./data"]
}

# Python package via uvx
github = {
    command = "uvx",
    args = ["run", "mcp-server-github@v0.1.0"],
    env = { "GITHUB_TOKEN" = "${GITHUB_TOKEN}" }
}

# Direct binary command
postgres = {
    command = "mcp-postgres",
    args = ["--connection", "${DATABASE_URL}"]
}

# Python script
custom = {
    command = "python",
    args = ["./scripts/mcp_server.py"]
}
```

### How It Works

1. **Configuration Management**: CCPM updates the `.mcp.json` file that Claude Code reads
2. **Non-destructive**: CCPM only manages servers it installs, preserving user-added servers
3. **Environment Variables**: Supports `${VAR}` expansion in arguments
4. **Tracking**: Lockfile tracks configured servers for reproducibility

### Important Notes

- MCP servers are **configured**, not installed as files
- The `.mcp.json` file may contain both CCPM-managed and user-managed servers
- CCPM adds metadata (`_ccpm`) to track which servers it manages
- Servers require their runtimes to be installed (Node.js for `npx`, Python for `uvx`, etc.)

### Example .mcp.json

After running `ccpm install`, your `.mcp.json` might look like:

```json
{
  "mcpServers": {
    "my-manual-server": {
      "command": "node",
      "args": ["./custom.js"]
    },
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "--root", "./data"],
      "_ccpm": {
        "managed": true,
        "version": "latest",
        "installed_at": "2024-01-15T10:30:00Z"
      }
    }
  }
}
```

## Scripts and Hooks Examples

### Script Example
Scripts are executable files that can perform various automation tasks:

```bash
# scripts/security-check.sh
#!/bin/bash
echo "Checking for sensitive data..."
grep -r "API_KEY\|SECRET\|PASSWORD" --exclude-dir=.git .
if [ $? -eq 0 ]; then
    echo "Warning: Potential sensitive data found!"
    exit 1
fi
echo "Security check passed!"
```

### Hook Configuration Example
Hooks are JSON files that configure when scripts should run:

```json
{
  "events": ["PreToolUse"],
  "matcher": "Bash|Write|Edit",
  "type": "command",
  "command": ".claude/ccpm/scripts/security-check.sh",
  "timeout": 5000,
  "description": "Security validation before file operations"
}
```

This hook will run the security check script before Claude Code uses the Bash, Write, or Edit tools.

### Hook Events
Available hook events in Claude Code:
- `PreToolUse` - Before a tool is executed
- `PostToolUse` - After a tool completes
- `UserPromptSubmit` - When user submits a prompt
- `UserPromptReceive` - When prompt is received
- `AssistantResponseReceive` - When assistant responds

### Installing Scripts and Hooks
```toml
# ccpm.toml
[scripts]
security = { source = "security-tools", path = "scripts/security-check.sh", version = "v1.0.0" }
validator = { source = "security-tools", path = "scripts/validate.py", version = "v1.0.0" }

[hooks]
pre-bash = { source = "security-tools", path = "hooks/pre-bash.json", version = "v1.0.0" }
file-guard = { source = "security-tools", path = "hooks/file-guard.json", version = "v1.0.0" }
```

After running `ccpm install`, hooks are automatically merged into Claude Code's settings, preserving any existing user-configured hooks.

## Advanced Usage

### Global Configuration

CCPM supports a global configuration file at `~/.ccpm/config.toml` for storing sensitive data like authentication tokens
that shouldn't be committed to version control.

#### Setting Up Global Config

```bash
# Initialize with example configuration
ccpm config init

# Edit the config file
ccpm config edit

# Show current configuration
ccpm config show
```

#### Global Config Example

```toml
# ~/.ccpm/config.toml
[sources]
# Private sources with authentication
private = "https://oauth2:ghp_xxxx@github.com/yourcompany/private-ccpm.git"
```

#### Managing Global Sources

```bash
# Add a private source with authentication
ccpm config add-source private "https://oauth2:TOKEN@github.com/yourcompany/private-ccpm.git"

# List all global sources (tokens are masked)
ccpm config list-sources

# Remove a source
ccpm config remove-source private
```

#### Source Priority

Sources are resolved in this order:

1. **Global sources** from `~/.ccpm/config.toml` (loaded first)
2. **Local sources** from `ccpm.toml` (can override global)

This allows teams to share `ccpm.toml` without exposing credentials.

### Security Best Practices

**IMPORTANT**: Never put credentials in `ccpm.toml`

- ✅ **DO**: Store authentication tokens in `~/.ccpm/config.toml` (global config)
- ✅ **DO**: Use SSH URLs for public repositories in `ccpm.toml`
- ✅ **DO**: Commit `ccpm.toml` to version control
- ❌ **DON'T**: Put tokens, passwords, or secrets in `ccpm.toml`
- ❌ **DON'T**: Use HTTPS URLs with embedded credentials in `ccpm.toml`
- ❌ **DON'T**: Commit `~/.ccpm/config.toml` to version control

The separation between project manifest (`ccpm.toml`) and global config (`~/.ccpm/config.toml`) ensures that sensitive
credentials never end up in your repository.

### Private Repositories

For repositories that don't require secrets, you can define them directly in `ccpm.toml`:

Use SSH authentication:

```toml
[sources]
private = "git@github.com:mycompany/private-agents.git"
```

For repositories with secrets, use the global config instead:

```bash
# Add to global config (not committed to git)
ccpm config add-source private "https://token:ghp_xxxx@github.com/yourcompany/private-ccpm.git"
```

### Reproducible Builds

For team consistency, commit your lockfile:

```bash
git add ccpm.lock
git commit -m "Lock dependency versions"
```

Team members install exact versions:

```bash
# Install exact versions from lockfile (recommended for CI/CD)
ccpm install --frozen
```

### Performance Optimization

CCPM automatically uses parallel operations for maximum performance:

```bash
# Installation uses parallel operations by default
ccpm install

# Control parallelism level if needed
ccpm install --max-parallel 4
```

### Development Workflow

```bash
# Update specific dependencies
ccpm update example-agent

# Preview updates without making changes
ccpm update --dry-run

# Validate before committing
ccpm validate --resolve
```

## Project Structure

After installation, your project structure looks like:

```
my-project/
├── ccpm.toml           # Dependency manifest
├── ccpm.lock           # Resolved versions (commit this!)
├── .claude/
│   ├── agents/
│   │   └── ccpm/       # CCPM-installed agents (default)
│   │       └── example-agent.md
│   └── ccpm/
│       └── snippets/   # CCPM-installed snippets (default)
│           └── example-snippet.md
```

Cache location: `~/.ccpm/cache/` (Unix/macOS) or `%LOCALAPPDATA%\ccpm\cache\` (Windows)

## Design Decisions

### Installation Model

CCPM copies files from cache to project directories rather than using symlinks:

- **Maximum Compatibility**: Works identically on Windows, macOS, and Linux
- **Git-Friendly**: Real files can be tracked and committed
- **Editor-Friendly**: No symlink confusion in IDEs
- **User-Friendly**: Edit installed files without affecting the cache

### Cache Management

CCPM maintains a global cache at `~/.ccpm/cache/` for cloned repositories:

```bash
ccpm cache info           # View cache statistics
ccpm cache clean          # Clean unused repositories
ccpm cache clean --all    # Clear entire cache
ccpm install --no-cache   # Bypass cache for fresh clones
```

Benefits:

- Fast reinstalls from cached repositories
- Offline work once cached
- Bandwidth efficient with incremental updates
- Clean projects without heavy directories

## Documentation

- **[USAGE.md](USAGE.md)** - Comprehensive usage guide with all commands and examples
- **[CONTRIBUTING.md](CONTRIBUTING.md)** - Guidelines for contributing to CCPM
- **[LICENSE.md](LICENSE.md)** - MIT License terms

## Building from Source

Requirements:

- Rust 1.70 or later
- Git 2.0 or later

```bash
# Clone the repository
git clone https://github.com/aig787/ccpm.git
cd ccpm

# Build in release mode
cargo build --release

# Run tests
cargo test

# Install locally
cargo install --path .

# Build for different targets
cargo build --target x86_64-apple-darwin --release
cargo build --target x86_64-unknown-linux-gnu --release
```

## Architecture

CCPM is built with Rust and uses system Git for compatibility. Key components:

- **manifest**: Parses ccpm.toml files
- **lockfile**: Manages ccpm.lock files
- **resolver**: Dependency resolution and conflict detection
- **git**: Git operations with authentication support
- **cache**: Global repository cache with file locking

### Concurrent Operations & File Locking

CCPM uses file locking (similar to Cargo) to prevent cache corruption during concurrent operations. Each cached
repository has a lock file at `~/.ccpm/cache/.locks/<source-name>.lock` that ensures:

- Safe parallel installations from different sources
- No git index corruption
- Automatic lock management
- Cross-platform compatibility via fs4

## Versioning

CCPM uses repository-level versioning (like GitHub Actions and Cargo workspaces):

- Versions apply to entire repositories, not individual files
- When you specify `version = "v1.1.0"`, you get the repository at that tag
- Files unchanged between versions still report the repository version

This provides simplicity and Git-native compatibility while trading off per-file version tracking.

## Best Practices

1. **Always commit ccpm.lock** - Ensures reproducible builds across team
2. **Use semantic versions** - Prefer `version = "v1.0.0"` over branches
3. **Validate before commits** - Run `ccpm validate` to catch issues early
4. **Use --frozen in CI/CD** - Ensures deterministic builds in automation
5. **Leverage parallel operations** - Automatic parallel processing speeds up installations
6. **Document custom sources** - Add comments in ccpm.toml for team clarity

## Troubleshooting

### Common Issues

**No manifest found:**

```bash
ccpm init  # Create a ccpm.toml with example content
# Or manually create ccpm.toml with your dependencies
```

**Version conflict:**

```bash
ccpm validate --resolve  # Check for conflicts
# Update version constraints in ccpm.toml
```

**Authentication failure:**

```bash
# For SSH: Ensure SSH keys are configured
# For HTTPS: Use personal access tokens in URL
```

**Lockfile out of sync:**

```bash
ccpm install  # Regenerate lockfile
```

**Performance issues:**

```bash
# Parallel processing is automatic
ccpm install

# Adjust parallelism if needed
ccpm install --max-parallel 2
```

## Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

```bash
# Fork and clone the repository
git clone https://github.com/aig787/ccpm.git

# Create a feature branch
git checkout -b feature/my-feature

# Make your changes and run tests
cargo test

# Submit a pull request
```

## License

MIT License - see [LICENSE.md](LICENSE.md) file for details.

## Inspiration

CCPM draws inspiration from:

- [Cargo](https://crates.io/) - Rust package manager (lockfile approach)
- [npm](https://npmjs.com/) - Node.js package manager
- [Helm](https://helm.sh/) - Kubernetes package manager

## Support

For issues and feature requests, please use the [GitHub issue tracker](https://github.com/aig787/ccpm/issues).

For questions and discussions, join our [GitHub Discussions](https://github.com/aig787/ccpm/discussions).