# Configuration Guide

CCPM uses a two-tier configuration system to separate sensitive data from project settings.

## Configuration Files

### Project Manifest (ccpm.toml)

The project manifest defines dependencies and is committed to version control.

**Location**: Project root directory
**Purpose**: Define sources and dependencies
**Version Control**: ✅ Commit to Git

### Global Configuration (~/.ccpm/config.toml)

The global configuration stores sensitive data like authentication tokens.

**Location**: `~/.ccpm/config.toml` (Unix/macOS) or `%USERPROFILE%\.ccpm\config.toml` (Windows)
**Purpose**: Store authentication tokens and private sources
**Version Control**: ❌ Never commit to Git

## Global Configuration

### Initial Setup

```bash
# Initialize with example configuration
ccpm config init

# Edit the config file
ccpm config edit

# Show current configuration (tokens masked)
ccpm config show
```

### Managing Sources

```bash
# Add a private source with authentication
ccpm config add-source private "https://oauth2:TOKEN@github.com/yourcompany/private-ccpm.git"

# List all global sources (tokens are masked)
ccpm config list-sources

# Remove a source
ccpm config remove-source private
```

### Global Config Format

```toml
# ~/.ccpm/config.toml
[sources]
# Private sources with authentication
private = "https://oauth2:ghp_xxxx@github.com/yourcompany/private-ccpm.git"
internal = "https://gitlab-ci-token:${CI_JOB_TOKEN}@gitlab.company.com/resources.git"

[settings]
# Optional: Override default cache directory
cache_dir = "/custom/cache/path"

# Optional: Set default parallelism
max_parallel = 4
```

## Source Priority

Sources are resolved in this order:

1. **Global sources** from `~/.ccpm/config.toml` (loaded first, contain secrets)
2. **Local sources** from `ccpm.toml` (override global, committed to Git)

This separation keeps authentication tokens out of version control while allowing teams to share project configurations.

## Authentication Methods

### SSH Authentication

For repositories accessible via SSH:

```toml
# In ccpm.toml (safe to commit)
[sources]
private = "git@github.com:mycompany/private-agents.git"
```

### HTTPS with Tokens

For repositories requiring authentication tokens:

```bash
# In global config only (never in ccpm.toml)
ccpm config add-source private "https://oauth2:ghp_xxxx@github.com/yourcompany/private-ccpm.git"
```

### Environment Variables

Use environment variables for CI/CD:

```toml
# In ~/.ccpm/config.toml
[sources]
ci-source = "https://gitlab-ci-token:${CI_JOB_TOKEN}@gitlab.company.com/resources.git"
```

## Security Best Practices

### DO ✅

- Store authentication tokens in `~/.ccpm/config.toml`
- Use SSH URLs for repositories in `ccpm.toml`
- Commit `ccpm.toml` to version control
- Use environment variables for CI/CD tokens
- Rotate tokens regularly
- Use read-only tokens when possible

### DON'T ❌

- Put tokens, passwords, or secrets in `ccpm.toml`
- Use HTTPS URLs with embedded credentials in `ccpm.toml`
- Commit `~/.ccpm/config.toml` to version control
- Share your global config file
- Use personal tokens in CI/CD
- Store tokens in plain text files

## Private Repositories

### Using SSH (Recommended)

```toml
# ccpm.toml - safe to commit
[sources]
private = "git@github.com:mycompany/private-agents.git"

[agents]
internal-tool = { source = "private", path = "agents/tool.md", version = "v1.0.0" }
```

### Using HTTPS with Tokens

```bash
# Add to global config
ccpm config add-source private "https://oauth2:TOKEN@github.com/yourcompany/private-ccpm.git"
```

```toml
# ccpm.toml - reference the source name
[agents]
internal-tool = { source = "private", path = "agents/tool.md", version = "v1.0.0" }
```

## CI/CD Configuration

### GitHub Actions

```yaml
- name: Configure CCPM
  run: |
    mkdir -p ~/.ccpm
    echo '[sources]' > ~/.ccpm/config.toml
    echo 'private = "https://oauth2:${{ secrets.GITHUB_TOKEN }}@github.com/org/private.git"' >> ~/.ccpm/config.toml

- name: Install dependencies
  run: ccpm install --frozen
```

### GitLab CI

```yaml
before_script:
  - mkdir -p ~/.ccpm
  - |
    cat > ~/.ccpm/config.toml << EOF
    [sources]
    private = "https://gitlab-ci-token:${CI_JOB_TOKEN}@gitlab.com/org/private.git"
    EOF
  - ccpm install --frozen
```

## Cache Configuration

### Custom Cache Directory

```toml
# ~/.ccpm/config.toml
[settings]
cache_dir = "/custom/cache/path"
```

### Cache Management

```bash
# View cache information
ccpm cache info

# Clean unused cache entries
ccpm cache clean

# Clear entire cache
ccpm cache clean --all

# Bypass cache for fresh clone
ccpm install --no-cache
```

## Performance Settings

### Parallelism Control

```toml
# ~/.ccpm/config.toml
[settings]
# Default parallelism for all operations
max_parallel = 4
```

Or per-command:

```bash
ccpm install --max-parallel 2
ccpm update --max-parallel 4
```

## Target Directories

Override default installation paths:

```toml
# ccpm.toml
[target]
agents = "custom/agents"
snippets = "resources/snippets"
commands = "tools/commands"
scripts = "automation/scripts"
hooks = "automation/hooks"
mcp-servers = "servers/mcp"

# Control gitignore behavior
gitignore = false  # Don't create .gitignore (default: true)
```

## Environment Variables

CCPM respects these environment variables:

- `CCPM_CONFIG` - Path to custom global config file
- `CCPM_CACHE_DIR` - Override cache directory
- `CCPM_NO_PROGRESS` - Disable progress bars
- `RUST_LOG` - Set logging level (debug, info, warn, error)

Example:

```bash
RUST_LOG=debug ccpm install
CCPM_NO_PROGRESS=1 ccpm update
CCPM_CONFIG=/custom/config.toml ccpm list
```

## Troubleshooting Configuration

### Config Not Found

```bash
# Check config location
ccpm config show

# Initialize if missing
ccpm config init
```

### Authentication Failures

```bash
# Verify source URL
ccpm config list-sources

# Test git access directly
git ls-remote https://token@github.com/org/repo.git
```

### Source Priority Issues

```bash
# Check which source is being used
RUST_LOG=debug ccpm install

# Override with local source
# In ccpm.toml, redefine the source name
```

### Token Rotation

When rotating tokens:

1. Update global config: `ccpm config edit`
2. Clear cache: `ccpm cache clean --all`
3. Test installation: `ccpm install --no-cache`