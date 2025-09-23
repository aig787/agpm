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
# Install AI agents
example = { source = "community", path = "agents/example.md", version = "v1.0.0" }
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

Install multiple resources using glob patterns:

```toml
[agents]
# All markdown files in agents/ai/
ai-agents = { source = "community", path = "agents/ai/*.md", version = "v1.0.0" }

# All review agents recursively
review-agents = { source = "community", path = "agents/**/review*.md", version = "v1.0.0" }

[snippets]
# All Python snippets
python = { source = "community", path = "snippets/python/*.md", version = "v1.0.0" }
```

## Resource Organization

### Default Locations

Resources are installed to these default locations:

- Agents: `.claude/agents/`
- Snippets: `.claude/ccpm/snippets/`
- Commands: `.claude/commands/`
- Scripts: `.claude/ccpm/scripts/`
- Hooks: `.claude/ccpm/hooks/`
- MCP Servers: `.claude/ccpm/mcp-servers/`

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

## Best Practices

1. **Always commit ccpm.lock** for reproducible builds
2. **Use semantic versioning** (`v1.0.0`) instead of branches
3. **Validate before committing**: Run `ccpm validate`
4. **Use --frozen in production**: `ccpm install --frozen`
5. **Keep secrets in global config**, never in `ccpm.toml`
6. **Document custom sources** with comments
7. **Test updates locally** before committing

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