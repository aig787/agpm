# User Guide

This guide will help you get started with CCPM and cover common workflows.

## Getting Started

### Prerequisites

- Git 2.0 or later installed
- Claude Code installed
- (Optional) Rust toolchain for building from source

### Installation

The quickest way to install CCPM:

```bash
# If you have Rust installed
cargo install ccpm

# For latest development version
cargo install --git https://github.com/aig787/ccpm.git

# Or download pre-built binaries
# See the Installation Guide for platform-specific instructions
```

### Your First Project

1. **Initialize a new project:**

```bash
ccpm init
```

This creates a basic `ccpm.toml` file with example dependencies.

2. **Install dependencies:**

```bash
ccpm install
```

This will:
- Clone the required Git repositories to `~/.ccpm/cache/`
- Copy resources to your project directories
- Generate a `ccpm.lock` file with exact versions

3. **Verify installation:**

```bash
ccpm list
```

## Basic Concepts

### Manifest File (ccpm.toml)

The manifest defines your project's dependencies:

```toml
[sources]
# Define Git repositories to pull resources from
community = "https://github.com/aig787/ccpm-community.git"

[agents]
# Install AI agents - path preservation maintains directory structure
example = { source = "community", path = "agents/example.md", version = "v1.0.0" }
# → Installed as: .claude/agents/example.md

nested = { source = "community", path = "agents/ai/helper.md", version = "v1.0.0" }
# → Installed as: .claude/agents/ai/helper.md (preserves ai/ subdirectory)
```

### Lockfile (ccpm.lock)

The lockfile records exact versions for reproducible installations:
- Generated automatically by `ccpm install`
- Should be committed to version control
- Ensures team members get identical versions

### Sources

Sources are Git repositories containing resources:
- Can be public (GitHub, GitLab) or private
- Can be local directories for development
- Authentication handled via global config

## Common Workflows

### Adding Dependencies

#### Method 1: Edit ccpm.toml directly

```toml
[agents]
my-agent = { source = "community", path = "agents/helper.md", version = "v1.0.0" }
```

Then run:
```bash
ccpm install
```

#### Method 2: Use CLI commands

```bash
# Add a source
ccpm add source community https://github.com/aig787/ccpm-community.git

# Add a dependency
ccpm add dep agent community:agents/helper.md --name my-agent
```

### Checking for Updates

Before updating, check what updates are available:

```bash
# Check all dependencies for available updates
ccpm outdated

# Check specific dependencies
ccpm outdated my-agent other-agent

# Use in CI to fail if updates are available
ccpm outdated --check

# Get JSON output for automation
ccpm outdated --format json
```

The `outdated` command shows:
- Current version from your lockfile
- Latest available version in the repository
- Compatible updates within your version constraints
- Major updates that would require constraint changes

### Updating Dependencies

Update all dependencies within version constraints:

```bash
ccpm update
```

Update specific dependency:

```bash
ccpm update my-agent
```

Preview updates without making changes:

```bash
ccpm update --dry-run
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
ccpm config add-source private "https://oauth2:TOKEN@github.com/org/private.git"
```

Then reference in your manifest:

```toml
[agents]
internal = { source = "private", path = "agents/internal.md", version = "v1.0.0" }
```

## Version Management

### Version Constraints

CCPM supports flexible version constraints:

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

⚠️ **Note**: Branches update on each `ccpm update`. Use tags for stability.

## Team Collaboration

### Setting Up

1. Create and configure `ccpm.toml`
2. Run `ccpm install` to generate `ccpm.lock`
3. Commit both files to Git:

```bash
git add ccpm.toml ccpm.lock
git commit -m "Add CCPM dependencies"
```

### Team Member Setup

Team members clone the repository and run:

```bash
# Install exact versions from lockfile
ccpm install --frozen
```

### Updating Dependencies

When updating dependencies:

1. Update version constraints in `ccpm.toml`
2. Run `ccpm update`
3. Test the changes
4. Commit the updated `ccpm.lock`

## CI/CD Integration

### GitHub Actions

```yaml
- name: Install CCPM
  run: cargo install --git https://github.com/aig787/ccpm.git

- name: Install dependencies
  run: ccpm install --frozen
```

### With Authentication

```yaml
- name: Configure CCPM
  run: |
    mkdir -p ~/.ccpm
    echo '[sources]' > ~/.ccpm/config.toml
    echo 'private = "https://oauth2:${{ secrets.GITHUB_TOKEN }}@github.com/org/private.git"' >> ~/.ccpm/config.toml

- name: Install dependencies
  run: ccpm install --frozen
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
# snippets/python/utils.md → .claude/ccpm/snippets/python/utils.md
# snippets/python/django/models.md → .claude/ccpm/snippets/python/django/models.md
python = { source = "community", path = "snippets/python/**/*.md", version = "v1.0.0" }
```

## Transitive Dependencies

Resources can declare their own dependencies, and CCPM will automatically resolve the entire dependency tree.

### Declaring Dependencies

**In Markdown files (.md)**, use YAML frontmatter:
```markdown
---
title: My Agent
description: Helper agent with dependencies
dependencies:
  agents:
    - path: agents/utils.md
      version: v1.0.0
  snippets:
    - path: snippets/helpers.md
---

# Agent content here
```

**In JSON files (.json)**, use a top-level field:
```json
{
  "events": ["SessionStart"],
  "type": "command",
  "command": "echo 'Starting'",
  "dependencies": {
    "commands": [
      {"path": "commands/setup.md", "version": "v1.0.0"}
    ]
  }
}
```

### How It Works

1. **Automatic Discovery**: When installing resources, CCPM scans their contents for dependency declarations
2. **Graph Building**: All dependencies (direct and transitive) are collected into a dependency graph
3. **Cycle Detection**: Circular dependencies are detected and reported as errors
4. **Topological Ordering**: Dependencies are installed before their dependents
5. **Version Resolution**: Conflicts are automatically resolved using the highest compatible version

### Dependency Inheritance

Transitive dependencies inherit properties from their parent:
- **Source**: Always inherits from the parent resource's source
- **Version**: Defaults to parent's version if not specified

### Example

```toml
# ccpm.toml
[sources]
community = "https://github.com/aig787/ccpm-community.git"

[commands]
deploy = { source = "community", path = "commands/deploy.md", version = "v1.0.0" }
```

If `deploy.md` declares:
```markdown
---
dependencies:
  agents:
    - path: agents/deploy-helper.md
  snippets:
    - path: snippets/aws-utils.md
      version: v2.0.0
---
```

Running `ccpm install` will automatically install:
1. `deploy.md` (direct dependency)
2. `agents/deploy-helper.md` (transitive, inherits v1.0.0)
3. `snippets/aws-utils.md` (transitive, uses v2.0.0)

### Lockfile Tracking

Transitive dependencies are tracked in `ccpm.lock`:
```toml
[[commands]]
name = "deploy"
path = "commands/deploy.md"
version = "v1.0.0"
dependencies = [
    "agents/deploy-helper@v1.0.0",
    "snippets/aws-utils@v2.0.0"
]
```

### Conflict Resolution

When multiple resources depend on different versions of the same resource:
- CCPM automatically selects the highest compatible version
- Conflicts are logged for transparency
- No manual intervention required

Example:
```text
Agent A requires utils.md v1.0.0
Agent B requires utils.md v2.0.0
→ Resolved: Using utils.md v2.0.0
```

## Resource Organization

### Default Locations

Resources are installed to these default locations, with source directory structure preserved:

- Agents: `.claude/agents/` (e.g., `agents/ai/helper.md` → `.claude/agents/ai/helper.md`)
- Snippets: `.claude/ccpm/snippets/` (e.g., `snippets/react/hooks.md` → `.claude/ccpm/snippets/react/hooks.md`)
- Commands: `.claude/commands/` (e.g., `commands/build/deploy.md` → `.claude/commands/build/deploy.md`)
- Scripts: `.claude/ccpm/scripts/` (e.g., `scripts/ci/test.sh` → `.claude/ccpm/scripts/ci/test.sh`)
- Hooks: `.claude/ccpm/hooks/`
- MCP Servers: `.claude/ccpm/mcp-servers/`

**Path Preservation**: The relative directory structure from the source repository is maintained during installation. This means:
- `agents/example.md` → `.claude/agents/example.md`
- `agents/ai/code-reviewer.md` → `.claude/agents/ai/code-reviewer.md`
- `agents/specialized/rust/expert.md` → `.claude/agents/specialized/rust/expert.md`

### Custom Locations

Override defaults in `ccpm.toml`:

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

CCPM v0.3.2+ includes significant performance optimizations:

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
ccpm install --max-parallel 8

# Use all available cores
ccpm install --max-parallel 0

# Single-threaded execution for debugging
ccpm install --max-parallel 1
```

### Cache Management
```bash
# View cache statistics
ccpm cache list

# Clean old cache entries
ccpm cache clean

# Bypass cache for fresh installation
ccpm install --no-cache
```

### Automatic Artifact Cleanup

CCPM automatically removes old resource files when:
- A dependency is removed from the manifest
- A resource's path changes in the manifest
- A resource is renamed

**Example:**
```toml
# Initial ccpm.toml
[agents]
helper = { source = "community", path = "agents/helper.md", version = "v1.0.0" }
# Installed as: .claude/agents/helper.md
```

After updating the path:
```toml
# Updated ccpm.toml
[agents]
helper = { source = "community", path = "agents/ai/helper.md", version = "v1.0.0" }
# Now installed as: .claude/agents/ai/helper.md
```

When you run `ccpm install`:
1. The old file at `.claude/agents/helper.md` is automatically removed
2. The new file is installed at `.claude/agents/ai/helper.md`
3. Empty parent directories are cleaned up (`.claude/agents/` only if empty)

**Benefits:**
- No manual cleanup required
- Prevents stale files from accumulating
- Maintains clean project structure

## Best Practices

1. **Always commit ccpm.lock** for reproducible builds
2. **Use semantic versioning** (`v1.0.0`) instead of branches
3. **Validate before committing**: Run `ccpm validate`
4. **Use --frozen in production**: `ccpm install --frozen`
5. **Keep secrets in global config**, never in `ccpm.toml`
6. **Document custom sources** with comments
7. **Check for outdated dependencies** regularly: `ccpm outdated`
8. **Test updates locally** before committing

## Troubleshooting

### Common Issues

**Manifest not found:**
```bash
ccpm init  # Create a new manifest
```

**Version conflicts:**
```bash
ccpm validate --resolve  # Check for conflicts
```

**Authentication issues:**
```bash
ccpm config list-sources  # Verify source configuration
```

**Lockfile out of sync:**
```bash
ccpm install  # Regenerate lockfile
```

### Getting Help

- Run `ccpm --help` for command help
- Check the [FAQ](faq.md) for common questions
- See [Troubleshooting Guide](troubleshooting.md) for detailed solutions
- Visit [GitHub Issues](https://github.com/aig787/ccpm/issues) for support

## Next Steps

- Explore [available commands](command-reference.md)
- Learn about [resource types](resources.md)
- Understand [versioning](versioning.md)
- Configure [authentication](configuration.md)