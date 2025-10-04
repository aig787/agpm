# AGPM Frequently Asked Questions

## Table of Contents

- [General Questions](#general-questions)
- [Installation & Setup](#installation--setup)
- [Dependencies & Versions](#dependencies--versions)
- [Resource Management](#resource-management)
- [Version Control](#version-control)
- [Team Collaboration](#team-collaboration)
- [Troubleshooting](#troubleshooting)
- [Advanced Usage](#advanced-usage)

## General Questions

### What is AGPM?
AGPM (Claude Code Package Manager) is a Git-based package manager for Claude Code resources. It enables reproducible installations of AI agents, snippets, commands, scripts, hooks, and MCP servers from Git repositories using a lockfile-based approach similar to Cargo or npm.

### How does AGPM differ from other package managers?
Unlike traditional package managers with central registries, AGPM is fully decentralized and Git-based. Resources are distributed directly from Git repositories, and versioning is tied to Git tags, branches, or commits. This makes it perfect for managing AI-related resources that may be proprietary or experimental.

### What types of resources can AGPM manage?
AGPM manages six resource types:
- **Direct Installation**: Agents, Snippets, Commands, Scripts (copied directly to target directories)
- **Configuration-Merged**: Hooks, MCP Servers (installed then merged into Claude Code config files)

### Do I need Git installed to use AGPM?
Yes, AGPM uses your system's Git command for all repository operations. This ensures maximum compatibility and respects your existing Git configuration (SSH keys, credentials, etc.).

## Installation & Setup

### How do I install AGPM?

AGPM is published to crates.io with automated releases. You can install via:

**All Platforms (via Cargo):**
```bash
# Requires Rust toolchain
cargo install agpm  # Published to crates.io
# Or from GitHub for latest development
cargo install --git https://github.com/aig787/agpm.git
```

**Unix/macOS (Pre-built Binary):**
```bash
# Download pre-built binary (automatically detects architecture)
curl -L https://github.com/aig787/agpm/releases/latest/download/agpm-$(uname -m)-$(uname -s | tr '[:upper:]' '[:lower:]').tar.gz | tar xz
chmod +x agpm
sudo mv agpm /usr/local/bin/
```

**Windows (Pre-built Binary):**
```powershell
# PowerShell
Invoke-WebRequest -Uri "https://github.com/aig787/agpm/releases/latest/download/agpm-x86_64-windows.zip" -OutFile agpm.zip
Expand-Archive -Path agpm.zip -DestinationPath .
# Move to a directory in PATH or add to PATH manually
Move-Item agpm.exe "$env:LOCALAPPDATA\agpm\bin\"
```

### How do I start a new AGPM project?
```bash
agpm init  # Creates agpm.toml
# or
agpm init --path ./my-project  # Creates in specific directory
```

### What's the difference between agpm.toml and agpm.lock?
- **agpm.toml**: Your project manifest that declares dependencies and their version constraints (you edit this)
- **agpm.lock**: Auto-generated file with exact resolved versions for reproducible builds (don't edit manually)

## Dependencies & Versions

### Can I use specific versions of resources?
Yes! AGPM supports multiple versioning strategies:
```toml
exact = { source = "repo", path = "file.md", version = "v1.0.0" }      # Exact tag
range = { source = "repo", path = "file.md", version = "^1.0.0" }      # Compatible versions
branch = { source = "repo", path = "file.md", branch = "main" }        # Track branch
commit = { source = "repo", path = "file.md", rev = "abc123" }         # Specific commit
```

### What version constraints are supported?
AGPM uses semantic versioning constraints like Cargo:
- `^1.2.3` - Compatible updates (>=1.2.3, <2.0.0)
- `~1.2.3` - Patch updates only (>=1.2.3, <1.3.0)
- `>=1.0.0, <2.0.0` - Custom ranges
- `latest` - Latest stable tag
- `*` - Any version

### How do I check for available updates?
```bash
agpm outdated            # Check all dependencies for updates
agpm outdated agent-name # Check specific dependency
agpm outdated --check    # Exit with error code if updates available (for CI)
agpm outdated --format json # JSON output for scripting
```

The `outdated` command shows current versions, latest available, and whether updates are compatible with your version constraints.

### How do I update dependencies?
```bash
agpm update              # Update all to latest compatible versions
agpm update agent-name   # Update specific dependency
agpm update --dry-run    # Preview changes without applying
```

### Can I use local files without Git?
Yes! You can reference local directories or individual files:
```toml
[sources]
local = "./local-resources"  # Local directory (no Git required)

[agents]
from-dir = { source = "local", path = "agents/helper.md" }  # From local source
direct = { path = "../agents/my-agent.md" }                 # Direct file path
```

## Resource Management

### Where are resources installed?
Default installation directories:
- Agents: `.claude/agents/`
- Snippets: `.claude/agpm/snippets/`
- Commands: `.claude/commands/`
- Scripts: `.claude/agpm/scripts/`
- Hooks: `.claude/agpm/hooks/` (then merged into `.claude/settings.local.json`)
- MCP Servers: `.claude/agpm/mcp-servers/` (then merged into `.mcp.json`)

### Can I customize installation directories?
Yes, there are two ways to customize where resources are installed:

**1. Global defaults** - Use the `[target]` section in agpm.toml:
```toml
[target]
agents = "custom/agents/path"
snippets = "custom/snippets/path"
commands = "custom/commands/path"
```

**2. Per-dependency override** - Use the `target` attribute on individual dependencies:
```toml
[agents]
# Uses default from [target] or built-in default
standard-agent = { source = "repo", path = "agents/standard.md" }

# Override installation path for this specific agent
special-agent = { source = "repo", path = "agents/special.md", target = "special/location/agent.md" }
```

The per-dependency `target` takes precedence over global `[target]` settings.

### How are installed files named?
Files are named based on the dependency key in agpm.toml, not their source filename:
```toml
[agents]
my-helper = { source = "repo", path = "agents/assistant.md" }
# Installs as: .claude/agents/my-helper.md (not assistant.md)
```

### What's the difference between hooks and scripts?
- **Scripts**: Executable files (.sh, .js, .py) that perform tasks
- **Hooks**: JSON configurations that define when to run scripts based on Claude Code events

### How do hooks and MCP servers get configured?
These are "configuration-merged" resources:
1. JSON files are installed to `.claude/agpm/`
2. Configurations are automatically merged into Claude Code's settings
3. User-configured entries are preserved

## Version Control

### What should I commit to Git?
Commit these files:
- `agpm.toml` - Your dependency manifest
- `agpm.lock` - Locked versions for reproducible builds

Don't commit:
- `.claude/` directory (auto-generated, gitignored by default)
- `~/.agpm/config.toml` (contains secrets)

### Why are my installed files gitignored?
By default, AGPM creates `.gitignore` entries to prevent installed dependencies from being committed. This follows the pattern of other package managers where you commit the manifest but not the installed packages.

### Can I commit installed resources to Git?
Yes, set `gitignore = false` in agpm.toml:
```toml
[target]
gitignore = false  # Don't create .gitignore
```

### How does AGPM handle the .gitignore file?
AGPM manages a section in `.gitignore` marked with "AGPM managed entries". It preserves any user entries outside this section while updating its own entries based on installed resources.

## Team Collaboration

### How do team members get the same versions?
1. Commit both `agpm.toml` and `agpm.lock` to your repository
2. Team members run `agpm install --frozen` to install exact lockfile versions
3. This ensures everyone has identical resource versions

### How do I handle private repositories?
For repositories requiring authentication:
```bash
# Add to global config (not committed)
agpm config add-source private "https://oauth2:TOKEN@github.com/org/private.git"

# Or use SSH in agpm.toml (safe to commit)
[sources]
private = "git@github.com:org/private.git"
```

### What's the difference between global and local sources?
- **Global sources** (`~/.agpm/config.toml`): For credentials and private repos, not committed
- **Local sources** (`agpm.toml`): Project-specific sources, safe to commit

Sources are resolved with global sources first, then local sources can override.

### What's the --frozen flag for?
`agpm install --frozen` uses exact versions from agpm.lock without checking for updates. Use this in CI/CD and production environments for deterministic builds.

## Troubleshooting

### Installation fails with "No manifest found"
Create a agpm.toml file:
```bash
agpm init  # Creates agpm.toml
```

### How do I debug installation issues?
```bash
# Run with debug logging
RUST_LOG=debug agpm install

# Validate manifest and sources
agpm validate --resolve

# Check cache status
agpm cache info
```

### Resources aren't being installed
1. Check agpm.toml syntax: `agpm validate`
2. Verify source repositories are accessible
3. Check Git authentication for private repos
4. Clear cache if corrupted: `agpm cache clean --all`

### How do I handle version conflicts?
```bash
# Check for conflicts
agpm validate --resolve

# Optional: inspect the graph
agpm validate --resolve --format json

# Auto-update the lockfile after adjusting constraints
agpm install
```

If the error reports incompatible constraints, update `agpm.toml` (or the resource metadata) so all dependents agree on a version. Prefer explicitly pinning the parent entry; AGPM will propagate that version to transitive dependencies. For duplicate install paths reported during resolution, add a `filename` or `target` override so each dependency gets a unique destination.

### Can I uninstall resources?
AGPM doesn't have an uninstall command. To remove resources:
1. Remove the dependency from agpm.toml
2. Run `agpm install` to update
3. Manually delete the installed files if needed

### What if my existing Claude Code settings conflict?
AGPM preserves user-configured settings in:
- `.claude/settings.local.json` (for hooks)
- `.mcp.json` (for MCP servers)

Only AGPM-managed entries (marked with metadata) are updated.

## Advanced Usage

### Can I use multiple sources for redundancy?
Yes, you can define multiple sources and use different ones for different dependencies:
```toml
[sources]
primary = "https://github.com/org/resources.git"
backup = "https://gitlab.com/org/resources.git"
local = "./local-resources"
```

### How do I bypass the cache?
```bash
agpm install --no-cache  # Fetch directly from sources
```

### Can I control parallel downloads?
```bash
agpm install --max-parallel 4  # Limit concurrent operations
```

### How do I clean up the cache?
```bash
agpm cache clean       # Remove unused repositories
agpm cache clean --all # Clear entire cache
agpm cache info        # View cache statistics
```

### Can I reference resources from subdirectories?
Yes, use the path field to specify subdirectories:
```toml
[agents]
nested = { source = "repo", path = "deep/path/to/agent.md" }
```

### How do environment variables work in configurations?
MCP server and hook configurations support `${VAR}` expansion:
```json
{
  "command": "node",
  "env": {
    "API_KEY": "${MY_API_KEY}"
  }
}
```

### Can I use AGPM in CI/CD pipelines?
Yes! Best practices for CI/CD:
1. Commit agpm.lock to your repository
2. Use `agpm install --frozen` in CI
3. Set authentication in environment or global config
4. Use `--max-parallel` to control resource usage

### What platforms does AGPM support?
AGPM is tested and supported on:
- macOS (x86_64, aarch64)
- Linux (x86_64, aarch64)
- Windows (x86_64)

### Where can I find example resources?
Check out community repositories:
- [ccpm-community](https://github.com/aig787/agpm-community) - Official community resources
- Search GitHub for repositories with "agpm" topics

## Still Have Questions?

If your question isn't answered here:
1. Check the [full documentation](README.md)
2. Search [existing issues](https://github.com/aig787/agpm/issues)
3. Ask in [GitHub Discussions](https://github.com/aig787/agpm/discussions)
4. Report bugs via [GitHub Issues](https://github.com/aig787/agpm/issues/new)
