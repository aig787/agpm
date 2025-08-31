# CCPM Command Reference

This document provides detailed information about all CCPM commands and their options.

## Global Options

```
ccpm [OPTIONS] <COMMAND>

Options:
  -v, --verbose           Enable verbose output
  -q, --quiet             Suppress non-error output
  -h, --help              Print help information
  -V, --version           Print version information
```

## Commands

### `ccpm init`

Initialize a new CCPM project by creating a `ccpm.toml` manifest file.

```bash
ccpm init [OPTIONS]

Options:
      --path <PATH>    Initialize in specific directory (default: current directory)
      --force          Overwrite existing ccpm.toml file
  -h, --help           Print help information
```

**Example:**
```bash
# Initialize in current directory
ccpm init

# Initialize in specific directory
ccpm init --path ./my-project

# Force overwrite existing manifest
ccpm init --force
```

### `ccpm install`

Install dependencies from `ccpm.toml` and generate/update `ccpm.lock`.

```bash
ccpm install [OPTIONS]

Options:
      --frozen                   Use exact lockfile versions without updates
      --no-cache                 Bypass cache and fetch directly from sources
      --max-parallel <NUMBER>    Maximum parallel operations (default: 4)
      --manifest-path <PATH>     Path to ccpm.toml (default: ./ccpm.toml)
  -h, --help                     Print help information
```

**Examples:**
```bash
# Standard installation
ccpm install

# Use exact lockfile versions (CI/production)
ccpm install --frozen

# Bypass cache for fresh fetch
ccpm install --no-cache

# Limit parallelism
ccpm install --max-parallel 2

# Use custom manifest path
ccpm install --manifest-path ./configs/ccpm.toml
```

### `ccpm update`

Update dependencies to latest versions within version constraints.

```bash
ccpm update [OPTIONS] [DEPENDENCY]

Arguments:
  [DEPENDENCY]    Update specific dependency (default: update all)

Options:
      --dry-run               Preview changes without applying
      --max-parallel <NUMBER> Maximum parallel operations (default: 4)
      --manifest-path <PATH>  Path to ccpm.toml (default: ./ccpm.toml)
  -h, --help                  Print help information
```

**Examples:**
```bash
# Update all dependencies
ccpm update

# Update specific dependency
ccpm update rust-expert

# Preview changes
ccpm update --dry-run

# Update with limited parallelism
ccpm update --max-parallel 2
```

### `ccpm list`

List installed resources from `ccpm.lock`.

```bash
ccpm list [OPTIONS]

Options:
      --format <FORMAT>       Output format: table, json (default: table)
      --type <TYPE>           Filter by resource type: agents, snippets, commands, scripts, hooks, mcp-servers
      --manifest-path <PATH>  Path to ccpm.toml (default: ./ccpm.toml)
  -h, --help                  Print help information
```

**Examples:**
```bash
# List all resources in table format
ccpm list

# List only agents
ccpm list --type agents

# Output as JSON
ccpm list --format json

# Use custom manifest path
ccpm list --manifest-path ./configs/ccpm.toml
```

### `ccpm validate`

Validate `ccpm.toml` syntax and dependency resolution.

```bash
ccpm validate [OPTIONS]

Options:
      --check-lock            Also validate lockfile consistency
      --resolve               Perform full dependency resolution
      --manifest-path <PATH>  Path to ccpm.toml (default: ./ccpm.toml)
  -h, --help                  Print help information
```

**Examples:**
```bash
# Basic syntax validation
ccpm validate

# Validate with lockfile consistency check
ccpm validate --check-lock

# Full validation with dependency resolution
ccpm validate --resolve

# Validate custom manifest
ccpm validate --manifest-path ./configs/ccpm.toml
```

### `ccpm add`

Add sources or dependencies to `ccpm.toml`.

#### Add Source

```bash
ccpm add source <NAME> <URL> [OPTIONS]

Arguments:
  <NAME>    Source name
  <URL>     Git repository URL or local path

Options:
      --manifest-path <PATH>  Path to ccpm.toml (default: ./ccpm.toml)
  -h, --help                  Print help information
```

#### Add Dependency

```bash
ccpm add dep <RESOURCE_TYPE> <SOURCE>:<PATH> [OPTIONS]

Arguments:
  <RESOURCE_TYPE>  Resource type: agent, snippet, command, script, hook, mcp-server
  <SOURCE>:<PATH>  Source name and file path (e.g., community:agents/rust.md)

Options:
      --name <NAME>           Dependency name (default: derived from path)
      --version <VERSION>     Version constraint (default: latest)
      --branch <BRANCH>       Git branch to track
      --rev <COMMIT>          Specific commit hash
      --target <PATH>         Custom installation path
      --manifest-path <PATH>  Path to ccpm.toml (default: ./ccpm.toml)
  -h, --help                  Print help information
```

**Examples:**
```bash
# Add a source repository
ccpm add source community https://github.com/aig787/ccpm-community.git

# Add an agent dependency
ccpm add dep agent community:agents/rust-expert.md --name rust-expert --version "v1.0.0"

# Add a snippet with custom name
ccpm add dep snippet tools:snippets/react.md --name react-utils

# Add script tracking a branch
ccpm add dep script local:scripts/build.sh --branch main

# Add hook with custom target
ccpm add dep hook security:hooks/pre-commit.json --target custom/hooks/security.json
```

### `ccpm remove`

Remove sources or dependencies from `ccpm.toml`.

#### Remove Source

```bash
ccpm remove source <NAME> [OPTIONS]

Arguments:
  <NAME>    Source name to remove

Options:
      --manifest-path <PATH>  Path to ccpm.toml (default: ./ccpm.toml)
  -h, --help                  Print help information
```

#### Remove Dependency

```bash
ccpm remove dep <RESOURCE_TYPE> <NAME> [OPTIONS]

Arguments:
  <RESOURCE_TYPE>  Resource type: agent, snippet, command, script, hook, mcp-server
  <NAME>           Dependency name to remove

Options:
      --manifest-path <PATH>  Path to ccpm.toml (default: ./ccpm.toml)
  -h, --help                  Print help information
```

**Examples:**
```bash
# Remove a source
ccpm remove source old-repo

# Remove an agent
ccpm remove dep agent old-agent

# Remove a snippet
ccpm remove dep snippet unused-snippet
```

### `ccpm config`

Manage global configuration in `~/.ccpm/config.toml`.

#### Show Configuration

```bash
ccpm config show [OPTIONS]

Options:
      --no-mask    Show actual token values (use with caution)
  -h, --help       Print help information
```

#### Initialize Configuration

```bash
ccpm config init [OPTIONS]

Options:
      --force      Overwrite existing configuration
  -h, --help       Print help information
```

#### Edit Configuration

```bash
ccpm config edit [OPTIONS]

Options:
  -h, --help    Print help information
```

#### Manage Sources

```bash
# Add source with authentication
ccpm config add-source <NAME> <URL>

# List all global sources (tokens masked)
ccpm config list-sources

# Remove source
ccpm config remove-source <NAME>
```

**Examples:**
```bash
# Show current configuration (tokens masked)
ccpm config show

# Initialize config with examples
ccpm config init

# Edit config in default editor
ccpm config edit

# Add private source with token
ccpm config add-source private "https://oauth2:ghp_xxxx@github.com/org/private.git"

# List all sources
ccpm config list-sources

# Remove a source
ccpm config remove-source old-private
```

### `ccpm cache`

Manage the global Git repository cache in `~/.ccpm/cache/`.

#### Cache Information

```bash
ccpm cache info [OPTIONS]

Options:
  -h, --help    Print help information
```

#### Clean Cache

```bash
ccpm cache clean [OPTIONS]

Options:
      --all       Remove all cached repositories
      --unused    Remove unused repositories only (default)
  -h, --help      Print help information
```

#### List Cache

```bash
ccpm cache list [OPTIONS]

Options:
  -h, --help    Print help information
```

**Examples:**
```bash
# Show cache statistics
ccpm cache info

# Clean unused repositories
ccpm cache clean

# Remove all cached repositories
ccpm cache clean --all

# List cached repositories
ccpm cache list
```

## Resource Types

CCPM manages six types of resources:

### Direct Installation Resources

- **Agents**: AI assistant configurations (installed to `.claude/agents/`)
- **Snippets**: Reusable code templates (installed to `.claude/ccpm/snippets/`)
- **Commands**: Claude Code slash commands (installed to `.claude/commands/`)
- **Scripts**: Executable automation files (installed to `.claude/ccpm/scripts/`)

### Configuration-Merged Resources

- **Hooks**: Event-based automation (installed to `.claude/ccpm/hooks/`, merged into `.claude/settings.local.json`)
- **MCP Servers**: Model Context Protocol servers (installed to `.claude/ccpm/mcp-servers/`, merged into `.mcp.json`)

## Version Constraints

CCPM supports semantic version constraints:

| Syntax | Description | Example |
|--------|-------------|---------|
| `1.0.0` | Exact version | `version = "1.0.0"` |
| `^1.0.0` | Compatible releases | `version = "^1.0.0"` (>=1.0.0, <2.0.0) |
| `~1.0.0` | Patch releases only | `version = "~1.0.0"` (>=1.0.0, <1.1.0) |
| `>=1.0.0` | Minimum version | `version = ">=1.0.0"` |
| `latest` | Latest stable tag | `version = "latest"` |
| `*` | Any version | `version = "*"` |

## Git References

Alternative to semantic versions:

- **Branches**: `branch = "main"` (mutable, updates on install)
- **Commits**: `rev = "abc123"` (immutable, exact commit)
- **Local paths**: No versioning, uses current files

## Pattern Dependencies

Use glob patterns to install multiple resources:

```toml
[agents]
# Install all AI agents
ai-tools = { source = "community", path = "agents/ai/*.md", version = "v1.0.0" }

# Install all review tools recursively
review-tools = { source = "community", path = "agents/**/review*.md", version = "v1.0.0" }
```

## Environment Variables

CCPM respects these environment variables:

- `CCPM_CONFIG` - Path to custom global config file
- `CCPM_CACHE_DIR` - Override cache directory
- `CCPM_NO_PROGRESS` - Disable progress bars
- `RUST_LOG` - Set logging level (debug, info, warn, error)

## Exit Codes

CCPM uses these exit codes:

- `0` - Success
- `1` - General error
- `2` - Invalid arguments or command usage
- `3` - Manifest validation error
- `4` - Dependency resolution error
- `5` - Git operation error
- `6` - File I/O error
- `101` - Panic or critical error

## Configuration Examples

### Basic Project

```toml
# ccpm.toml
[sources]
community = "https://github.com/aig787/ccpm-community.git"

[agents]
rust-expert = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0" }

[snippets]
react-hooks = { source = "community", path = "snippets/react-hooks.md", version = "^1.0.0" }
```

### Advanced Project

```toml
# ccpm.toml
[sources]
community = "https://github.com/aig787/ccpm-community.git"
tools = "https://github.com/myorg/ccpm-tools.git"
local = "./local-resources"

[agents]
# Pattern-based dependency
ai-agents = { source = "community", path = "agents/ai/*.md", version = "v1.0.0" }
# Single file dependency
custom-agent = { source = "tools", path = "agents/custom.md", version = "^2.0.0" }

[snippets]
python-utils = { source = "community", path = "snippets/python/*.md", version = "~1.2.0" }

[commands]
deploy = { source = "tools", path = "commands/deploy.md", branch = "main" }

[scripts]
build = { source = "local", path = "scripts/build.sh" }

[hooks]
pre-commit = { source = "community", path = "hooks/pre-commit.json", version = "v1.0.0" }

[mcp-servers]
filesystem = { source = "community", path = "mcp/filesystem.json", version = "latest" }

[target]
# Custom installation paths
agents = "custom/agents"
snippets = "resources/snippets"
# Disable gitignore generation
gitignore = false
```

## Getting Help

- Run `ccpm --help` for general help
- Run `ccpm <command> --help` for command-specific help
- Check the [FAQ](docs/faq.md) for common questions
- Visit [GitHub Issues](https://github.com/aig787/ccpm/issues) for support