# CCPM Command Reference

This document provides detailed information about all CCPM commands and their options.

## Global Options

```
ccpm [OPTIONS] <COMMAND>

Options:
  -v, --verbose              Enable verbose output
  -q, --quiet                Suppress non-error output
      --config <PATH>        Path to custom global configuration file
      --manifest-path <PATH> Path to the manifest file (ccpm.toml)
      --no-progress          Disable progress bars and spinners
  -h, --help                 Print help information
  -V, --version              Print version information
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

Install dependencies from `ccpm.toml` and generate/update `ccpm.lock`. Uses centralized version resolution and SHA-based worktree optimization for maximum performance.

```bash
ccpm install [OPTIONS]

Options:
  -f, --force                    Bypass lockfile staleness checks and force installation
      --regenerate               Delete and regenerate the lockfile from scratch
      --no-lock                  Don't write lockfile after installation
      --frozen                   Use exact lockfile versions without updates
      --no-cache                 Bypass cache and fetch directly from sources
      --max-parallel <NUMBER>    Maximum parallel operations (default: max(10, 2 × CPU cores))
      --manifest-path <PATH>     Path to ccpm.toml (default: ./ccpm.toml)
  -h, --help                     Print help information
```

**Examples:**
```bash
# Standard installation
ccpm install

# Use exact lockfile versions (CI/production)
ccpm install --frozen

# Bypass lockfile staleness checks (useful when you know lockfile is safe)
ccpm install --force

# Regenerate lockfile from scratch (removes and recreates)
ccpm install --regenerate

# Install without creating lockfile
ccpm install --no-lock

# Bypass cache for fresh fetch
ccpm install --no-cache

# Control parallelism (default: max(10, 2 × CPU cores))
ccpm install --max-parallel 8

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
  -f, --force                 Bypass lockfile staleness checks and force update
      --dry-run               Preview changes without applying
      --max-parallel <NUMBER> Maximum parallel operations (default: max(10, 2 × CPU cores))
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

# Update with custom parallelism
ccpm update --max-parallel 6
```

### `ccpm outdated`

Check for available updates to installed dependencies. Analyzes the lockfile against available versions in Git repositories to identify dependencies with newer versions available.

```bash
ccpm outdated [OPTIONS] [DEPENDENCIES]...

Arguments:
  [DEPENDENCIES]...    Check specific dependencies (default: check all)

Options:
      --format <FORMAT>       Output format: table, json (default: table)
      --check                 Exit with error code 1 if updates are available
      --no-fetch             Use cached repository data without fetching updates
      --max-parallel <NUMBER> Maximum parallel operations (default: max(10, 2 × CPU cores))
      --manifest-path <PATH>  Path to ccpm.toml (default: ./ccpm.toml)
      --no-progress          Disable progress bars and spinners
  -h, --help                  Print help information
```

**Examples:**
```bash
# Check all dependencies for updates
ccpm outdated

# Check specific dependencies
ccpm outdated rust-expert my-agent

# Use in CI - exit with error if outdated
ccpm outdated --check

# Use cached data without fetching
ccpm outdated --no-fetch

# JSON output for scripting
ccpm outdated --format json

# Control parallelism
ccpm outdated --max-parallel 5
```

**Output Information:**

The command displays:
- **Current**: The version currently installed (from lockfile)
- **Latest**: The newest version that satisfies the manifest's version constraint
- **Available**: The absolute newest version available in the repository
- **Type**: The resource type (agent, snippet, command, script, hook, mcp-server)

**Version Analysis:**

The outdated command performs sophisticated version comparison:
1. **Compatible Updates**: Versions that satisfy the current version constraint in ccpm.toml
2. **Major Updates**: Newer versions that exceed the constraint (require manual manifest update)
3. **Up-to-date**: Dependencies already on the latest compatible version

**JSON Output Format:**

When using `--format json`, the output includes:
```json
{
  "outdated": [
    {
      "name": "my-agent",
      "type": "agent",
      "source": "community",
      "current": "v1.0.0",
      "latest": "v1.2.0",
      "latest_available": "v2.1.0",
      "constraint": "^1.0.0",
      "has_update": true,
      "has_major_update": true
    }
  ],
  "summary": {
    "total": 5,
    "outdated": 2,
    "with_updates": 1,
    "with_major_updates": 1,
    "up_to_date": 3
  }
}
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
ccpm add dep <RESOURCE_TYPE> <SPEC> [OPTIONS]

Arguments:
  <RESOURCE_TYPE>  Resource type: agent, snippet, command, script, hook, mcp-server
  <SPEC>           Dependency specification (see formats below)

Options:
      --name <NAME>           Dependency name (default: derived from path)
  -f, --force                 Force overwrite if dependency exists
      --manifest-path <PATH>  Path to ccpm.toml (default: ./ccpm.toml)
  -h, --help                  Print help information
```

**Dependency Specification Formats:**

The `<SPEC>` argument supports multiple formats for different source types:

1. **Git Repository Dependencies** - `source:path[@version]`
   - `source`: Name of a Git source defined in `[sources]` section
   - `path`: Path to file(s) within the repository
   - `version`: Optional Git ref (tag/branch/commit), defaults to "main"

2. **Local File Dependencies** - Direct file paths
   - Absolute paths: `/home/user/agents/local.md`, `C:\Users\name\agent.md`
   - Relative paths: `./agents/local.md`, `../shared/snippet.md`
   - File URLs: `file:///home/user/script.sh`

3. **Pattern Dependencies** - Using glob patterns
   - `source:agents/*.md@v1.0.0` - All .md files in agents directory
   - `source:snippets/**/*.md` - All .md files recursively
   - `./local/**/*.json` - All JSON files from local directory

**Examples:**
```bash
# Add a source repository first
ccpm add source community https://github.com/aig787/ccpm-community.git

# Git repository dependencies
ccpm add dep agent community:agents/rust-expert.md@v1.0.0
ccpm add dep agent community:agents/rust-expert.md  # Uses "main" branch
ccpm add dep snippet community:snippets/react.md@feature-branch

# Local file dependencies
ccpm add dep agent ./local-agents/helper.md --name my-helper
ccpm add dep script /usr/local/scripts/build.sh
ccpm add dep hook ../shared/hooks/pre-commit.json

# Pattern dependencies (bulk installation)
ccpm add dep agent "community:agents/ai/*.md@v1.0.0" --name ai-agents
ccpm add dep snippet "community:snippets/**/*.md" --name all-snippets
ccpm add dep script "./scripts/*.sh" --name local-scripts

# Windows paths
ccpm add dep agent C:\Resources\agents\windows.md
ccpm add dep script "file://C:/Users/name/scripts/build.ps1"

# Custom names (recommended for patterns)
ccpm add dep agent community:agents/reviewer.md --name code-reviewer
ccpm add dep snippet "community:snippets/python/*.md" --name python-utils

# Force overwrite existing dependency
ccpm add dep agent community:agents/new-version.md --name existing-agent --force
```

**Name Derivation:**

If `--name` is not provided, the dependency name is automatically derived from the file path:
- `agents/reviewer.md` → name: "reviewer"
- `snippets/utils.md` → name: "utils"
- `/path/to/helper.md` → name: "helper"

For pattern dependencies, you should typically provide a custom name since multiple files will be installed.

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

### `ccpm upgrade`

Self-update CCPM to the latest version or a specific version. Includes automatic backup and rollback capabilities with built-in security features.

```bash
ccpm upgrade [OPTIONS] [VERSION]

Arguments:
  [VERSION]    Target version to upgrade to (e.g., "0.4.0" or "v0.4.0")

Options:
      --check       Check for updates without installing
  -s, --status      Show current version and latest available
  -f, --force       Force upgrade even if already on latest version
      --rollback    Rollback to previous version from backup
      --no-backup   Skip creating a backup before upgrade
  -h, --help        Print help information
```

**Examples:**
```bash
# Upgrade to latest version
ccpm upgrade

# Check for available updates
ccpm upgrade --check

# Show current and latest version
ccpm upgrade --status

# Upgrade to specific version
ccpm upgrade 0.4.0

# Force reinstall latest version
ccpm upgrade --force

# Rollback to previous version
ccpm upgrade --rollback

# Upgrade without creating backup
ccpm upgrade --no-backup
```

#### Security Features

The upgrade command implements multiple security measures to ensure safe updates:

- **GitHub Integration**: Only downloads binaries from official CCPM GitHub releases
- **HTTPS Downloads**: Uses secure HTTPS connections for all network operations
- **Platform-Specific Archives**: Downloads appropriate archive format for your platform (.tar.xz for Unix, .zip for Windows)
- **Atomic Operations**: Minimizes vulnerability windows during binary replacement
- **Permission Preservation**: Maintains original file permissions and ownership
- **Backup Protection**: Creates backups with appropriate permissions before any modifications

### `ccpm cache`

Manage the global Git repository cache in `~/.ccpm/cache/`. The cache uses SHA-based worktrees for optimal deduplication and performance.

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

CCPM manages six types of resources with optimized parallel installation:

### Direct Installation Resources

- **Agents**: AI assistant configurations (installed to `.claude/agents/`)
- **Snippets**: Reusable code templates (installed to `.claude/ccpm/snippets/`)
- **Commands**: Claude Code slash commands (installed to `.claude/commands/`)
- **Scripts**: Executable automation files (installed to `.claude/ccpm/scripts/`)

### Configuration-Merged Resources

- **Hooks**: Event-based automation (installed to `.claude/ccpm/hooks/`, merged into `.claude/settings.local.json`)
- **MCP Servers**: Model Context Protocol servers (installed to `.claude/ccpm/mcp-servers/`, merged into `.mcp.json`)

### Parallel Installation Features

- **Worktree-based processing**: Each resource uses an isolated Git worktree for safe concurrent installation
- **Configurable concurrency**: Use `--max-parallel` to control the number of simultaneous operations
- **Real-time progress**: Multi-phase progress tracking shows installation status across all parallel operations
- **Instance-level optimization**: Worktrees are cached and reused within a single command for maximum efficiency

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

## Parallelism Control

CCPM v0.3.0 introduces advanced parallelism control for optimal performance:

### --max-parallel Flag

Available on `install` and `update` commands to control concurrent operations:

- **Default**: `max(10, 2 × CPU cores)` - Automatically scales with system capacity
- **Range**: 1 to 100 parallel operations
- **Use Cases**:
  - High-performance systems: Increase for faster operations
  - Limited bandwidth: Reduce to avoid overwhelming network
  - CI/CD environments: Tune based on available resources

**Examples:**
```bash
# Use default parallelism (recommended)
ccpm install

# High-performance system with fast network
ccpm install --max-parallel 20

# Limited bandwidth or shared resources
ccpm install --max-parallel 3

# Single-threaded operation (debugging)
ccpm install --max-parallel 1
```

### Performance Characteristics

- **Worktree-Based**: Uses Git worktrees for parallel-safe repository access
- **Instance Caching**: Per-command fetch cache reduces redundant network operations
- **Smart Batching**: Dependencies from same source share operations where possible
- **Memory Efficient**: Each parallel operation uses minimal memory overhead

## Environment Variables

CCPM respects these environment variables:

- `CCPM_CONFIG` - Path to custom global config file
- `CCPM_CACHE_DIR` - Override cache directory
- `CCPM_NO_PROGRESS` - Disable progress bars
- `CCPM_MAX_PARALLEL` - Default parallelism level (overridden by --max-parallel flag)
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
