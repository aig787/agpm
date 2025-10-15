# User Guide

This guide covers common AGPM workflows. See [Installation](installation.md) for setup instructions.

## Getting Started

1. **Initialize a new project:**

```bash
agpm init
```

This creates a basic `agpm.toml` file with example dependencies.

2. **Install dependencies:**

```bash
agpm install
```

This will:
- Clone the required Git repositories to `~/.agpm/cache/`
- Copy resources to your project directories
- Generate a `agpm.lock` file with exact versions

3. **Verify installation:**

```bash
agpm list
```

## Basic Concepts

### Manifest File (agpm.toml)

The manifest defines your project's dependencies:

```toml
[sources]
# Define Git repositories to pull resources from
community = "https://github.com/aig787/agpm-community.git"

[agents]
# Install AI agents - path preservation maintains directory structure
example = { source = "community", path = "agents/example.md", version = "v1.0.0" }
# → Installed as: .claude/agents/example.md

nested = { source = "community", path = "agents/ai/helper.md", version = "v1.0.0" }
# → Installed as: .claude/agents/ai/helper.md (preserves ai/ subdirectory)
```

See the [Manifest Reference](manifest-reference.md) for a complete field-by-field breakdown and CLI mapping guidance.

### Lockfile (agpm.lock)

The lockfile records exact versions for reproducible installations:
- Generated automatically by `agpm install`
- Should be committed to version control
- Ensures team members get identical versions

#### Lifecycle and Guarantees

- `agpm install` always re-runs dependency resolution using the current manifest and lockfile. If nothing has changed, it writes the same resolved versions and SHAs back to disk—versions do **not** automatically advance just because you reinstalled.
- Resolution only diverges when the manifest changed, a tag/branch now points somewhere else, or a dependency was missing from the previous lockfile.
- Use `agpm install --no-lock` when you want to verify installs without touching `agpm.lock` (e.g., local experiments).
- Use `agpm install --frozen` in CI or release pipelines to assert that the existing lockfile matches the manifest exactly. The command fails instead of regenerating when staleness is detected.

#### Detecting Staleness

AGPM automatically checks for stale lockfiles during install and via `agpm validate --check-lock`:
- Duplicate entries or source URL drift (security-critical issues)
- Manifest entries missing from the lockfile
- Version/path changes that have not been resolved yet

If any of these occur, rerun `agpm install` (without `--frozen`) to regenerate the lockfile so teammates stay in sync.

### Sources

Sources are Git repositories containing resources:
- Can be public (GitHub, GitLab) or private
- Can be local directories for development
- Authentication handled via global config

## Multi-Tool Support

AGPM supports multiple AI coding assistants from a single manifest using the tool configuration system.

> ⚠️ **Alpha Feature**: OpenCode support is currently in alpha. While functional, it may have incomplete features or breaking
> changes in future releases. Use with caution in production environments. Claude Code support is stable and production-ready.

### Supported Tools

- **Claude Code** (default) - Full support for agents, commands, scripts, hooks, MCP servers, and snippets ✅ **Stable**
- **OpenCode** - Support for agents, commands, and MCP servers 🚧 **Alpha**
- **AGPM** - Shared snippets that can be referenced by multiple tools ✅ **Stable**
- **Custom** - Define your own custom tools via configuration

### Using Multiple Tools

By default, resources install for Claude Code. To target a different tool, add the `tool` field:

```toml
[sources]
community = "https://github.com/aig787/agpm-community.git"

[agents]
# Claude Code agent (default - no tool field needed)
rust-expert = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0" }
# → Installs to .claude/agents/rust-expert.md

# OpenCode agent (explicit tool)
rust-expert-oc = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0", tool = "opencode" }
# → Installs to .opencode/agent/rust-expert.md

[snippets]
# Shared snippet (accessible to both tools)
rust-patterns = { source = "community", path = "snippets/rust-patterns.md", version = "v1.0.0", tool = "agpm" }
# → Installs to .agpm/snippets/rust-patterns.md
```

### Directory Differences

**Important**: OpenCode uses singular directory names while Claude Code uses plural:

| Resource | Claude Code | OpenCode (Alpha) |
|----------|-------------|------------------|
| Agents | `.claude/agents/` | 🚧 `.opencode/agent/` |
| Commands | `.claude/commands/` | 🚧 `.opencode/command/` |
| MCP Servers | `.mcp.json` | 🚧 `opencode.json` |

AGPM handles this automatically based on the `tool` field.

### Multi-Tool Project Example

```toml
[sources]
community = "https://github.com/aig787/agpm-community.git"

[agents]
# Install the same agent for both tools
helper-cc = { source = "community", path = "agents/helper.md", version = "v1.0.0" }
helper-oc = { source = "community", path = "agents/helper.md", version = "v1.0.0", tool = "opencode" }  # Alpha

# Rust experts for both
rust-expert-cc = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0" }
rust-expert-oc = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0", tool = "opencode" }  # Alpha

[commands]
# Deployment commands for both tools
deploy-cc = { source = "community", path = "commands/deploy.md", version = "v2.0.0" }
deploy-oc = { source = "community", path = "commands/deploy.md", version = "v2.0.0", tool = "opencode" }  # Alpha

[mcp-servers]
# MCP servers (automatically routed to correct config file)
filesystem-cc = { source = "community", path = "mcp/filesystem.json", version = "v1.0.0" }
filesystem-oc = { source = "community", path = "mcp/filesystem.json", version = "v1.0.0", tool = "opencode" }  # Alpha

[snippets]
# Shared snippets usable by both tools
shared-patterns = { source = "community", path = "snippets/patterns/*.md", version = "v1.0.0", tool = "agpm" }
```

### Benefits

- **Unified Management**: One manifest for all your AI assistant resources
- **Consistent Versioning**: Keep all tools synchronized to the same resource versions
- **Shared Infrastructure**: Reuse snippets and patterns across tools
- **Easy Migration**: Switch between tools without recreating your resource setup

## Common Workflows

### Adding Dependencies

#### Method 1: Edit agpm.toml directly

```toml
[agents]
my-agent = { source = "community", path = "agents/helper.md", version = "v1.0.0" }
```

Then run:
```bash
agpm install
```

#### Method 2: Use CLI commands

```bash
# Add a source
agpm add source community https://github.com/aig787/agpm-community.git

# Add a dependency
agpm add dep agent community:agents/helper.md --name my-agent
```

### Checking for Updates

Before updating, check what updates are available:

```bash
# Check all dependencies for available updates
agpm outdated

# Check specific dependencies
agpm outdated my-agent other-agent

# Use in CI to fail if updates are available
agpm outdated --check

# Get JSON output for automation
agpm outdated --format json
```

The `outdated` command shows:
- Current version from your lockfile
- Latest available version in the repository
- Compatible updates within your version constraints
- Major updates that would require constraint changes

### Updating Dependencies

Update all dependencies within version constraints:

```bash
agpm update
```

Update specific dependency:

```bash
agpm update my-agent
```

Preview updates without making changes:

```bash
agpm update --dry-run
```

### Working with Local Resources

For development, use local directories:

```toml
[sources]
local = "./my-resources"

[agents]
dev-agent = { source = "local", path = "agents/dev.md" }
```

Or reference files directly:

```toml
[agents]
local-agent = { path = "../agents/my-agent.md" }
```

### Private Repositories

For private repositories, configure authentication globally:

```bash
# Add private source with token
agpm config add-source private "https://oauth2:TOKEN@github.com/org/private.git"
```

Then reference in your manifest:

```toml
[agents]
internal = { source = "private", path = "agents/internal.md", version = "v1.0.0" }
```

## Version Management

### Version Constraints

AGPM supports flexible version constraints:

```toml
# Exact version
agent1 = { source = "community", path = "agents/a1.md", version = "v1.0.0" }

# Compatible updates (1.x.x)
agent2 = { source = "community", path = "agents/a2.md", version = "^1.0.0" }

# Patch updates only (1.0.x)
agent3 = { source = "community", path = "agents/a3.md", version = "~1.0.0" }

# Latest stable
agent4 = { source = "community", path = "agents/a4.md", version = "latest" }
```

### Branch Tracking

For development, track branches:

```toml
[agents]
dev-agent = { source = "community", path = "agents/dev.md", branch = "main" }
```

⚠️ **Note**: Branches update on each `agpm update`. Use tags for stability.

## Team Collaboration

### Setting Up

1. Create and configure `agpm.toml`
2. Run `agpm install` to generate `agpm.lock`
3. Commit both files to Git:

```bash
git add agpm.toml agpm.lock
git commit -m "Add AGPM dependencies"
```

### Team Member Setup

Team members clone the repository and run:

```bash
# Install exact versions from lockfile
agpm install --frozen
```

### Updating Dependencies

When updating dependencies:

1. Update version constraints in `agpm.toml`
2. Run `agpm update`
3. Test the changes
4. Commit the updated `agpm.lock`

## CI/CD Integration

### GitHub Actions

```yaml
- name: Install AGPM
  run: cargo install --git https://github.com/aig787/agpm.git

- name: Install dependencies
  run: agpm install --frozen
```

### With Authentication

```yaml
- name: Configure AGPM
  run: |
    mkdir -p ~/.agpm
    echo '[sources]' > ~/.agpm/config.toml
    echo 'private = "https://oauth2:${{ secrets.GITHUB_TOKEN }}@github.com/org/private.git"' >> ~/.agpm/config.toml

- name: Install dependencies
  run: agpm install --frozen
```

## Pattern Matching

Install multiple resources using glob patterns. Each matched file preserves its source directory structure:

```toml
[agents]
# All markdown files in agents/ai/ - each preserves its path
# agents/ai/assistant.md → .claude/agents/ai/assistant.md
# agents/ai/analyzer.md → .claude/agents/ai/analyzer.md
ai-agents = { source = "community", path = "agents/ai/*.md", version = "v1.0.0" }

# All review agents recursively - nested structure maintained
# agents/code/review-expert.md → .claude/agents/code/review-expert.md
# agents/security/review-scanner.md → .claude/agents/security/review-scanner.md
review-agents = { source = "community", path = "agents/**/review*.md", version = "v1.0.0" }

[snippets]
# All Python snippets - directory structure preserved
# snippets/python/utils.md → .agpm/snippets/python/utils.md
# snippets/python/django/models.md → .agpm/snippets/python/django/models.md
python = { source = "community", path = "snippets/python/**/*.md", version = "v1.0.0" }
```

During `agpm install`, AGPM expands each glob, installs every concrete match, and records them individually in `agpm.lock` under the pattern dependency. Lockfile entries use the `resource_type/name@resolved_version` format so you can track the exact files that were installed.

> **Tip**: Pair pattern entries with descriptive keys (like `ai-agents`) and review the resolved output with `agpm list` or by inspecting `agpm.lock` to confirm the matches.

## Transitive Dependencies

Resources can declare their own dependencies, and AGPM will automatically resolve the entire dependency tree.

### Declaring Dependencies

**Markdown files (.md)** - YAML frontmatter:
```markdown
---
dependencies:
  agents:
    - path: ./helper.md
      version: v1.0.0
  snippets:
    - path: ../shared/utils.md
---
```

**JSON files (.json)** - top-level field:
```json
{
  "dependencies": {
    "commands": [
      {"path": "./setup.md", "version": "v1.0.0"}
    ]
  }
}
```

### Path Resolution

All transitive dependency paths are **file-relative** - resolved from the parent resource file's location.

Examples: `./sibling.md`, `../parent/file.md`, `../../shared/common.md`

### Inheritance

Dependencies inherit from their parent:
- **Source**: Git URL or local path
- **Version**: Defaults to parent's version (Git-backed only)
- **Tool**: Parent's tool if compatible, otherwise resource type default

### Examples

**Git-backed:**
```toml
[sources]
community = "https://github.com/aig787/agpm-community.git"

[commands]
deploy = { source = "community", path = "commands/deploy.md", version = "v1.0.0" }
```

`deploy.md` declares:
```markdown
---
dependencies:
  agents:
    - path: ../agents/helper.md
  snippets:
    - path: ../snippets/utils.md
      version: v2.0.0
---
```

Installs: `deploy.md`, `agents/helper.md` (inherits v1.0.0), `snippets/utils.md` (v2.0.0)

**Path-only:**
```toml
[agents]
local-agent = { path = "../local-agents/main.md" }
```

`main.md` declares:
```markdown
---
dependencies:
  agents:
    - path: ./helper.md
    - path: ../shared/common.md
---
```

Installs: `main.md`, `helper.md`, `common.md`

### Notes

- Scripts require executable permissions
- Hooks merge into `.claude/settings.local.json`
- MCP servers inherit `command`/`args` from file
- Override versions by specifying explicit `version:` in metadata
- Path-only deps don't support version constraints

### Conflict Resolution

Compatible version constraints resolve to the highest satisfying version. Incompatible constraints fail with an error.

```text
Error: Version conflict for agents/helper.md
  requested: v1.0.0 (manifest)
  requested: v2.0.0 (transitive via agents/deploy.md)
```

**Fix conflicts:**
- Run `agpm validate --resolve` to see the dependency graph
- Update version constraints in manifest or resource metadata
- Add `filename`/`target` overrides for duplicate install paths

## Resource Organization

### Default Locations

Resources are installed to these default locations, with source directory structure preserved:

- Agents: `.claude/agents/` (e.g., `agents/ai/helper.md` → `.claude/agents/ai/helper.md`)
- Snippets: `.agpm/snippets/` (default to agpm tool; e.g., `snippets/react/hooks.md` → `.agpm/snippets/react/hooks.md`)
- Commands: `.claude/commands/` (e.g., `commands/build/deploy.md` → `.claude/commands/build/deploy.md`)
- Scripts: `.claude/scripts/` (e.g., `scripts/ci/test.sh` → `.claude/scripts/ci/test.sh`)
- Hooks: `.claude/hooks/`
- MCP Servers: Merged into `.mcp.json` (no separate directory)

**Path Preservation**: The relative directory structure from the source repository is maintained during installation. This means:
- `agents/example.md` → `.claude/agents/example.md`
- `agents/ai/code-reviewer.md` → `.claude/agents/ai/code-reviewer.md`
- `agents/specialized/rust/expert.md` → `.claude/agents/specialized/rust/expert.md`

### Custom Locations

Override defaults in `agpm.toml`:

```toml
[target]
agents = "custom/agents"
snippets = "resources/snippets"
commands = "tools/commands"
```

### Version Control

By default, installed files are gitignored. To commit them:

```toml
[target]
gitignore = false  # Don't create .gitignore
```

## Performance Features

AGPM v0.3.2+ includes significant performance optimizations:

### Centralized Version Resolution
- **Batch processing**: All version constraints resolved in a single operation per repository
- **Automatic deduplication**: Identical version requirements processed only once
- **Minimal Git operations**: Single fetch per repository per command

### SHA-Based Worktree Optimization
- **Maximum reuse**: Multiple dependencies with same commit SHA share one worktree
- **Parallel safety**: Independent worktrees enable conflict-free concurrent operations
- **Smart caching**: Command-instance caching prevents redundant network operations

### Controlling Parallelism
```bash
# Control number of parallel operations (default: max(10, 2 × CPU cores))
agpm install --max-parallel 8

# Use all available cores
agpm install --max-parallel 0

# Single-threaded execution for debugging
agpm install --max-parallel 1
```

### Cache Management
```bash
# View cache statistics
agpm cache list

# Clean old cache entries
agpm cache clean

# Bypass cache for fresh installation
agpm install --no-cache
```

### Automatic Artifact Cleanup

AGPM automatically removes old resource files when:
- A dependency is removed from the manifest
- A resource's path changes in the manifest
- A resource is renamed

**Example:**
```toml
# Initial agpm.toml
[agents]
helper = { source = "community", path = "agents/helper.md", version = "v1.0.0" }
# Installed as: .claude/agents/helper.md
```

After updating the path:
```toml
# Updated agpm.toml
[agents]
helper = { source = "community", path = "agents/ai/helper.md", version = "v1.0.0" }
# Now installed as: .claude/agents/ai/helper.md
```

When you run `agpm install`:
1. The old file at `.claude/agents/helper.md` is automatically removed
2. The new file is installed at `.claude/agents/ai/helper.md`
3. Empty parent directories are cleaned up (`.claude/agents/` only if empty)

**Benefits:**
- No manual cleanup required
- Prevents stale files from accumulating
- Maintains clean project structure

## Best Practices

1. **Always commit agpm.lock** for reproducible builds
2. **Use semantic versioning** (`v1.0.0`) instead of branches
3. **Validate before committing**: Run `agpm validate`
4. **Use --frozen in production**: `agpm install --frozen`
5. **Keep secrets in global config**, never in `agpm.toml`
6. **Document custom sources** with comments
7. **Check for outdated dependencies** regularly: `agpm outdated`
8. **Test updates locally** before committing

## Troubleshooting

### Common Issues

**Manifest not found:**
```bash
agpm init  # Create a new manifest
```

**Version conflicts:**
```bash
agpm validate --resolve  # Check for conflicts
```

**Authentication issues:**
```bash
agpm config list-sources  # Verify source configuration
```

**Lockfile out of sync:**
```bash
agpm install  # Regenerate lockfile
```

### Getting Help

- Run `agpm --help` for command help
- Check the [FAQ](faq.md) for common questions
- See [Troubleshooting Guide](troubleshooting.md) for detailed solutions
- Visit [GitHub Issues](https://github.com/aig787/agpm/issues) for support

## Next Steps

- Explore [available commands](command-reference.md)
- Learn about [resource types](resources.md)
- Understand [versioning](versioning.md)
- Configure [authentication](configuration.md)
