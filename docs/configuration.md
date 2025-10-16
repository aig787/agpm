# Configuration Guide

AGPM uses a two-tier configuration system to separate sensitive data from project settings.

## Configuration Files

### Project Manifest (agpm.toml)

The project manifest defines dependencies and is committed to version control.

**Location**: Project root directory
**Purpose**: Define sources and dependencies
**Version Control**: ✅ Commit to Git

### Global Configuration (~/.agpm/config.toml)

The global configuration stores sensitive data like authentication tokens.

**Location**: `~/.agpm/config.toml` (Unix/macOS) or `%USERPROFILE%\.agpm\config.toml` (Windows)
**Purpose**: Store authentication tokens and private sources
**Version Control**: ❌ Never commit to Git

### Private Configuration (agpm.private.toml)

User-level patches and overrides that should not be shared with the team.

**Location**: Project root directory (next to agpm.toml)
**Purpose**: Personal resource field overrides and customizations
**Version Control**: ❌ Never commit to Git (add to .gitignore)

## Global Configuration

### Initial Setup

```bash
# Initialize with example configuration
agpm config init

# Edit the config file
agpm config edit

# Show current configuration (tokens masked)
agpm config show
```

### Managing Sources

```bash
# Add a private source with authentication
agpm config add-source private "https://oauth2:TOKEN@github.com/yourcompany/private-agpm.git"

# List all global sources (tokens are masked)
agpm config list-sources

# Remove a source
agpm config remove-source private
```

### Global Config Format

```toml
# ~/.agpm/config.toml
[sources]
# Private sources with authentication
private = "https://oauth2:ghp_xxxx@github.com/yourcompany/private-agpm.git"
internal = "https://gitlab-ci-token:${CI_JOB_TOKEN}@gitlab.company.com/resources.git"

[settings]
# Optional: Override default cache directory
cache_dir = "/custom/cache/path"

# Optional: Set default parallelism (default: max(10, 2 × CPU cores))
max_parallel = 8

# Optional: Enable enhanced progress reporting
enhanced_progress = true
```

## Default Tool Configuration

AGPM allows you to override which tool is used by default for each resource type. This is useful when you work primarily with one tool (e.g., Claude Code only) or want to customize the default routing behavior.

### Overview

By default, AGPM uses these tool assignments:
- **Snippets** → `agpm` (shared infrastructure)
- **All other resources** (agents, commands, scripts, hooks, mcp-servers) → `claude-code`

You can override these defaults using the `[default-tools]` section in your manifest.

### Configuration

Add the `[default-tools]` section to your `agpm.toml`:

```toml
# agpm.toml
[default-tools]
snippets = "claude-code"  # Claude-only users: install snippets to .claude/snippets/
agents = "claude-code"    # Explicit (already the default)
commands = "opencode"     # Default to OpenCode for commands
```

### Supported Resource Types

You can configure defaults for any resource type:

- `agents` - Default tool for agent resources
- `snippets` - Default tool for snippet resources
- `commands` - Default tool for command resources
- `scripts` - Default tool for script resources
- `hooks` - Default tool for hook resources
- `mcp-servers` - Default tool for MCP server resources

### Use Cases

#### Claude Code Only Users

If you only use Claude Code and want snippets in `.claude/snippets/` instead of `.agpm/snippets/`:

```toml
[default-tools]
snippets = "claude-code"

[snippets]
# Now installs to .claude/snippets/ by default
rust-patterns = { source = "community", path = "snippets/rust.md", version = "v1.0.0" }
```

#### OpenCode Preferred

If you primarily use OpenCode:

```toml
[default-tools]
agents = "opencode"
commands = "opencode"

[agents]
# Now installs to .opencode/agent/ by default
helper = { source = "community", path = "agents/helper.md", version = "v1.0.0" }
```

#### Mixed Workflows

Configure different defaults for different resource types:

```toml
[default-tools]
snippets = "agpm"        # Shared snippets (default, shown for clarity)
agents = "claude-code"   # Claude Code agents
commands = "opencode"    # OpenCode commands
```

### Overriding Defaults

Dependencies with explicit `tool` fields always override the configured defaults:

```toml
[default-tools]
agents = "claude-code"   # Default for agents

[agents]
# Uses default: installs to .claude/agents/
default-agent = { source = "community", path = "agents/helper.md", version = "v1.0.0" }

# Explicit override: installs to .opencode/agent/
opencode-agent = { source = "community", path = "agents/helper.md", version = "v1.0.0", tool = "opencode" }
```

### Validation

The configured tool names must be valid tool identifiers:

```bash
# Validate manifest including default-tools configuration
agpm validate

# Install with configured defaults
agpm install
```

**Valid tool names:**
- `claude-code` (built-in)
- `opencode` (built-in, alpha)
- `agpm` (built-in)
- Custom tool names defined in `[tools.custom-name]` sections

## Private Configuration

The `agpm.private.toml` file enables user-level customization without affecting team configuration.

### Purpose

Use private configuration for:
- Personal API keys and credentials
- Custom model preferences (temperature, max_tokens)
- Development-specific settings
- Local endpoint overrides
- Any field you don't want to share with your team

### Location and Setup

**File**: `agpm.private.toml` (same directory as `agpm.toml`)

**Add to .gitignore**:
```gitignore
# User-specific AGPM configuration
agpm.private.toml
```

### Syntax

Private patches use the same syntax as project patches:

```toml
# agpm.private.toml
[patch.agents.rust-expert]
model = "claude-3-opus"              # Personal model preference
temperature = "0.9"                  # Custom temperature
api_key = "${ANTHROPIC_API_KEY}"    # Personal API key
custom_endpoint = "https://proxy.internal"

[patch.commands.deploy]
dry_run = true                       # Always dry-run in your environment
notification_email = "me@example.com"
```

### How Private Patches Work

Private patches **extend** project patches rather than replacing them:

**Project patch** (agpm.toml - committed):
```toml
[patch.agents.rust-expert]
model = "claude-3-haiku"
max_tokens = "4096"
```

**Private patch** (agpm.private.toml - not committed):
```toml
[patch.agents.rust-expert]
temperature = "0.9"
api_key = "${MY_KEY}"
```

**Result**: All four fields are applied:
- `model` = "claude-3-haiku" (from project)
- `max_tokens` = "4096" (from project)
- `temperature` = "0.9" (from private)
- `api_key` = "${MY_KEY}" (from private)

### Conflict Detection

If the **same field** appears in both files, installation fails immediately:

```text
Error: Patch conflict for agents/rust-expert
  Field 'model' appears in both agpm.toml and agpm.private.toml
  Resolution: Remove the field from one of the files
```

This prevents silent overwrites and ensures explicit configuration.

### Common Use Cases

#### Personal API Keys

```toml
# agpm.private.toml
[patch.agents.claude-assistant]
api_key = "${ANTHROPIC_API_KEY}"
organization_id = "org-123"

[patch.mcp-servers.openai]
api_key = "${OPENAI_API_KEY}"
```

#### Model Preferences

```toml
# Team uses Haiku by default, you prefer Opus
[patch.agents.code-reviewer]
model = "claude-3-opus"
temperature = "0.8"
```

#### Development Settings

```toml
# Enable debug mode and verbose logging for local development
[patch.agents.deployer]
debug = true
log_level = "debug"
dry_run = true

[patch.commands.test]
parallel = false  # Sequential execution for easier debugging
```

#### Endpoint Overrides

```toml
# Use local proxy or custom endpoint
[patch.agents.assistant]
custom_endpoint = "https://localhost:8080/v1"
verify_ssl = false
```

### Security Considerations

**DO** ✅:
- Add `agpm.private.toml` to `.gitignore`
- Use environment variables for sensitive values (`${VAR_NAME}`)
- Keep credentials in `agpm.private.toml`, never in `agpm.toml`
- Document required private patches in project README
- Use read-only or scoped API keys when possible

**DON'T** ❌:
- Commit `agpm.private.toml` to version control
- Hard-code sensitive values (use env vars instead)
- Share your `agpm.private.toml` file
- Put team-wide configuration in private patches
- Use admin or full-access API keys

### Validation

Private patches are validated with the same rules as project patches:

```bash
# Validate both project and private patches
agpm validate

# Install and apply all patches
agpm install
```

Validation checks:
1. Syntax is valid TOML
2. Patched dependencies exist in manifest
3. No conflicting fields between project and private patches
4. All field types are valid

### Example: Complete Setup

**agpm.toml** (team configuration, committed):
```toml
[sources]
community = "https://github.com/aig787/agpm-community.git"

[agents]
rust-expert = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0" }
code-reviewer = { source = "community", path = "agents/code-reviewer.md", version = "v1.0.0" }

# Team-wide model settings
[patch.agents.rust-expert]
model = "claude-3-haiku"
max_tokens = "4096"

[patch.agents.code-reviewer]
model = "claude-3-opus"
```

**agpm.private.toml** (personal configuration, not committed):
```toml
# Personal model preferences
[patch.agents.rust-expert]
temperature = "0.9"                  # I prefer higher creativity
api_key = "${ANTHROPIC_API_KEY}"    # My personal API key

[patch.agents.code-reviewer]
temperature = "0.7"                  # Different preference for reviews
custom_endpoint = "https://my-proxy.internal"
```

**.gitignore**:
```gitignore
# AGPM private configuration
agpm.private.toml
```

**README.md** (document requirements):
```markdown
## Setup

1. Clone the repository
2. Run `agpm install`
3. (Optional) Create `agpm.private.toml` for personal settings:
   - Add `api_key` fields if using personal API keys
   - Customize `temperature` or `model` preferences
   - See `docs/configuration.md` for examples
```

### Troubleshooting

**Private patches not applied**:
- Verify `agpm.private.toml` is in the same directory as `agpm.toml`
- Check file syntax: `agpm validate`
- Ensure patched dependencies exist in manifest

**Conflict errors**:
- Identify which field appears in both files
- Decide which file should own that field
- Remove from the other file

**Environment variables not expanded**:
- Ensure variables are set in your shell
- Use `${VAR_NAME}` syntax in TOML
- Test with: `echo $VAR_NAME`

## Source Priority

Sources are resolved in this order:

1. **Global sources** from `~/.agpm/config.toml` (loaded first, contain secrets)
2. **Local sources** from `agpm.toml` (override global, committed to Git)

This separation keeps authentication tokens out of version control while allowing teams to share project configurations.

## Authentication Methods

### SSH Authentication

For repositories accessible via SSH:

```toml
# In agpm.toml (safe to commit)
[sources]
private = "git@github.com:mycompany/private-agents.git"
```

### HTTPS with Tokens

For repositories requiring authentication tokens:

```bash
# In global config only (never in agpm.toml)
agpm config add-source private "https://oauth2:ghp_xxxx@github.com/yourcompany/private-agpm.git"
```

### Environment Variables

Use environment variables for CI/CD:

```toml
# In ~/.agpm/config.toml
[sources]
ci-source = "https://gitlab-ci-token:${CI_JOB_TOKEN}@gitlab.company.com/resources.git"
```

## Security Best Practices

### DO ✅

- Store authentication tokens in `~/.agpm/config.toml`
- Use SSH URLs for repositories in `agpm.toml`
- Commit `agpm.toml` to version control
- Use environment variables for CI/CD tokens
- Rotate tokens regularly
- Use read-only tokens when possible

### DON'T ❌

- Put tokens, passwords, or secrets in `agpm.toml`
- Use HTTPS URLs with embedded credentials in `agpm.toml`
- Commit `~/.agpm/config.toml` to version control
- Share your global config file
- Use personal tokens in CI/CD
- Store tokens in plain text files

## Private Repositories

### Using SSH (Recommended)

```toml
# agpm.toml - safe to commit
[sources]
private = "git@github.com:mycompany/private-agents.git"

[agents]
internal-tool = { source = "private", path = "agents/tool.md", version = "v1.0.0" }
```

### Using HTTPS with Tokens

```bash
# Add to global config
agpm config add-source private "https://oauth2:TOKEN@github.com/yourcompany/private-agpm.git"
```

```toml
# agpm.toml - reference the source name
[agents]
internal-tool = { source = "private", path = "agents/tool.md", version = "v1.0.0" }
```

## CI/CD Configuration

### GitHub Actions

```yaml
- name: Configure AGPM
  run: |
    mkdir -p ~/.agpm
    echo '[sources]' > ~/.agpm/config.toml
    echo 'private = "https://oauth2:${{ secrets.GITHUB_TOKEN }}@github.com/org/private.git"' >> ~/.agpm/config.toml

- name: Install dependencies
  run: agpm install --frozen
```

### GitLab CI

```yaml
before_script:
  - mkdir -p ~/.agpm
  - |
    cat > ~/.agpm/config.toml << EOF
    [sources]
    private = "https://gitlab-ci-token:${CI_JOB_TOKEN}@gitlab.com/org/private.git"
    EOF
  - agpm install --frozen
```

## Cache Configuration

### Custom Cache Directory

```toml
# ~/.agpm/config.toml
[settings]
cache_dir = "/custom/cache/path"
```

### Cache Management

```bash
# View cache information
agpm cache info

# Clean unused cache entries
agpm cache clean

# Clear entire cache
agpm cache clean --all

# Bypass cache for fresh clone
agpm install --no-cache
```

## Performance Settings

### Parallelism Control

AGPM provides flexible parallelism control at multiple levels:

```toml
# ~/.agpm/config.toml
[settings]
# Default parallelism for all operations (default: max(10, 2 × CPU cores))
max_parallel = 8
```

Per-command override:

```bash
# Override global setting for specific commands
agpm install --max-parallel 12
agpm update --max-parallel 6
```

Environment variable override:

```bash
# Set default for current session
export AGPM_MAX_PARALLEL=16
agpm install  # Uses 16 parallel operations
```

#### Parallelism Guidelines

- **Default behavior**: AGPM automatically sets reasonable limits based on your system
- **High-end systems**: Can safely use 16-32 parallel operations
- **Resource-constrained environments**: Consider lowering to 4-8 operations
- **CI/CD environments**: May need lower limits depending on container resources
- **Network-limited environments**: Lower parallelism reduces network congestion

## Target Directories

Override default installation paths:

```toml
# agpm.toml
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

AGPM respects these environment variables for configuration and debugging:

### Configuration Variables

- `AGPM_CONFIG` - Path to custom global config file
- `AGPM_CACHE_DIR` - Override cache directory location
- `AGPM_MAX_PARALLEL` - Default parallelism level (overridden by --max-parallel flag)

### User Interface Variables

- `AGPM_NO_PROGRESS` - Disable progress bars (useful for CI/CD)
- `AGPM_ENHANCED_PROGRESS` - Enable enhanced progress reporting with phase details

### Debugging Variables

- `RUST_LOG` - Set logging level (debug, info, warn, error)
- `RUST_LOG_STYLE` - Control log formatting (auto, always, never)

### Git Operation Variables

- `GIT_TRACE` - Enable Git command tracing
- `GIT_TRACE_PERFORMANCE` - Enable Git performance tracing

### Examples

```bash
# Debug mode with detailed Git operation logging
RUST_LOG=debug agpm install

# CI/CD mode: no progress bars, custom parallelism
AGPM_NO_PROGRESS=1 AGPM_MAX_PARALLEL=4 agpm install --frozen

# Enhanced progress with custom config location
AGPM_ENHANCED_PROGRESS=1 AGPM_CONFIG=/custom/config.toml agpm install

# High-performance mode for powerful systems
AGPM_MAX_PARALLEL=20 agpm install --max-parallel 32

# Debugging Git worktree operations
RUST_LOG=debug GIT_TRACE=1 agpm install
```

## Troubleshooting Configuration

### Config Not Found

```bash
# Check config location
agpm config show

# Initialize if missing
agpm config init
```

### Authentication Failures

```bash
# Verify source URL
agpm config list-sources

# Test git access directly
git ls-remote https://token@github.com/org/repo.git
```

### Source Priority Issues

```bash
# Check which source is being used (with worktree context)
RUST_LOG=debug agpm install

# Trace Git operations to see repository access patterns
GIT_TRACE=1 RUST_LOG=debug agpm install

# Override with local source
# In agpm.toml, redefine the source name
```

### Parallel Operation Issues

```bash
# Reduce parallelism if experiencing resource contention
agpm install --max-parallel 2

# Debug worktree creation issues
RUST_LOG=debug agpm install --no-cache

# Monitor system resources during installation
top -p $(pgrep agpm) &
agpm install --max-parallel 16
```

### Token Rotation

When rotating tokens:

1. Update global config: `agpm config edit`
2. Clear cache: `agpm cache clean --all`
3. Test installation: `agpm install --no-cache`