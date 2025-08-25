# CCPM Usage Guide

This guide covers all workflows for using CCPM (Claude Code Package Manager) with the lockfile-based dependency management system.

## Overview
CCPM follows a Cargo-like model with `ccpm.toml` defining dependencies and `ccpm.lock` tracking exact resolved versions. All resources are fetched directly from Git repositories with no central registry required.

## Quick Start

### 1. Initial Project Setup

Initialize a new CCPM project:

```bash
# Create a basic ccpm.toml file
ccpm init

# Or create with example entries
ccpm init --with-examples

# Force overwrite existing manifest
ccpm init --force

# Initialize in a specific directory
ccpm init --path /path/to/project
```

Or manually create a `ccpm.toml` file in your project root:

```toml
[sources]
official = "https://github.com/example-org/ccpm-official.git"
community = "https://github.com/example-org/ccpm-community.git"

[agents]
code-reviewer = { source = "official", path = "agents/code-reviewer.md", version = "v1.0.0" }
test-writer = { source = "community", path = "agents/test-writer.md", version = "v2.1.0" }

[snippets]
utils = { source = "official", path = "snippets/utils.md", version = "v1.2.0" }
```

### 2. Add Dependencies

Add sources and dependencies to your manifest:

```bash
# Add a source repository
ccpm add source official https://github.com/example-org/ccpm-official.git

# Add an agent dependency
ccpm add dep official:agents/code-reviewer.md@v1.0.0 --agent

# Add a snippet dependency with custom name
ccpm add dep official:snippets/utils.md@latest --snippet --name my-utils

# Add a local file dependency
ccpm add dep ../local-agents/helper.md --agent --name local-helper

# Force overwrite existing dependency
ccpm add dep official:agents/test.md@v1.0.0 --agent --force
```

### 3. Install Dependencies

Install dependencies:

```bash
# Install all dependencies
ccpm install

# Output:
# 📦 Installing dependencies from ccpm.toml...
# ✅ Resolved 3 dependencies
# ✅ Installed code-reviewer v1.0.0
# ✅ Installed test-writer v2.1.0
# ✅ Installed utils v1.2.0
# ✅ Generated ccpm.lock
```

This creates:
- `ccpm.lock` - Lockfile with resolved versions
- `agents/` directory with installed agents
- `snippets/` directory with installed snippets

## Core Commands

CCPM provides essential commands for dependency management:

## 1. Init Command

**Purpose**: Initialize a new CCPM manifest file

```bash
ccpm init [OPTIONS]
```

### Init Options

| Option | Description |
|--------|-------------|
| `--path <PATH>` | Directory to create manifest in (defaults to current) |
| `--force` | Overwrite existing manifest |
| `--with-examples` | Add example entries to manifest |

### Init Examples

```bash
# Create basic ccpm.toml in current directory
ccpm init

# Create with example dependencies
ccpm init --with-examples

# Initialize in specific directory
ccpm init --path ./my-project

# Force overwrite existing manifest
ccpm init --force
```

### Init Output

**Basic initialization:**
```bash
$ ccpm init
✓ Initialized ccpm.toml at ./ccpm.toml

Next steps:
  1. Edit ccpm.toml to add sources
  2. Add agent and snippet dependencies
  3. Run 'ccpm install' to install dependencies
```

**With examples:**
```bash
$ ccpm init --with-examples
✓ Initialized ccpm.toml at ./ccpm.toml

Example entries added:
  - Official source repository
  - Example agent and snippet dependencies

Edit ccpm.toml to add your own sources and dependencies
```

## 2. Add Command

**Purpose**: Add sources or dependencies to your manifest

```bash
ccpm add <SUBCOMMAND> [OPTIONS]
```

### Add Subcommands

**Add Source:**
```bash
ccpm add source <NAME> <URL>
```

**Add Dependency:**
```bash
ccpm add dep <SPEC> [OPTIONS]
```

### Add Dependency Options

| Option | Description |
|--------|-------------|
| `--agent` | Add as an agent dependency |
| `--snippet` | Add as a snippet dependency |
| `--name <NAME>` | Custom name (defaults to filename) |
| `--force` | Overwrite existing dependency |

### Dependency Specification Format

- **Remote dependency:** `source:path@version`
  - Example: `official:agents/code-reviewer.md@v1.0.0`
- **Local file (plain directory):** `path` (NO version support)
  - Example: `../local-agents/helper.md`
  - Example: `./snippets/local.md`
  - Example: `/absolute/path/to/agent.md`

### Add Examples

```bash
# Add a source repository
ccpm add source official https://github.com/example-org/ccpm-official.git
ccpm add source community https://github.com/example-org/ccpm-community.git

# Add agent from remote source
ccpm add dep official:agents/code-reviewer.md@v1.0.0 --agent

# Add snippet with custom name
ccpm add dep official:snippets/utils.md@latest --snippet --name my-utils

# Add local file as agent (no version support)
ccpm add dep ../local-agents/helper.md --agent

# Add local file with absolute path
ccpm add dep /home/user/agents/local.md --agent --name local-agent

# Force overwrite existing
ccpm add dep official:agents/test.md@v2.0.0 --agent --force
```

### Add Output

**Adding source:**
```bash
$ ccpm add source official https://github.com/example-org/ccpm-official.git
✓ Added source 'official': https://github.com/example-org/ccpm-official.git
```

**Adding dependency:**
```bash
$ ccpm add dep official:agents/code-reviewer.md@v1.0.0 --agent
✓ Added agent 'code-reviewer'

$ ccpm add dep official:snippets/utils.md@latest --snippet --name my-utils
✓ Added snippet 'my-utils'
```

### Local Dependencies

CCPM supports two types of local dependencies:

#### Plain Directory Dependencies
Simple file paths that point directly to `.md` files. These do NOT support versions:

```bash
# Add plain directory dependency - NO version allowed
ccpm add dep ../agents/my-agent.md --agent
ccpm add dep ./snippets/util.md --snippet
ccpm add dep /absolute/path/to/helper.md --agent

# ❌ INVALID - versions not supported for plain paths
# ccpm add dep ../agents/agent.md@v1.0.0 --agent  # ERROR!
```

#### Local Git Repositories
Use `file://` URLs in sources for local git repositories with full version support:

```bash
# Add local git repository as source
ccpm add source local-repo file:///home/user/my-git-repo

# Now use it with versions, branches, tags
ccpm add dep local-repo:agents/agent.md@v1.0.0 --agent
ccpm add dep local-repo:agents/dev.md@main --agent
```

**Important:**
- Plain paths (`../`, `./`, `/`) are for simple file references only
- `file://` URLs must point to valid git repositories
- Plain paths cannot be used as sources

## 3. Install Command

**Purpose**: Install dependencies from ccpm.toml and generate/update lockfile

```bash
ccpm install [OPTIONS]
```

### Install Options

| Option | Description |
|--------|-------------|
| `--frozen` | Use exact versions from lockfile (fail if lockfile missing/outdated) |
| `--serial` | Disable parallel processing (process sequentially) |
| `--force` | Force re-download even if files are cached |
| `--no-lock` | Don't update lockfile after installation |

### Install Examples

```bash
# Basic installation - install all dependencies (parallel by default)
ccpm install

# Use exact lockfile versions (for CI/CD)
ccpm install --frozen

# Sequential installation (disable parallel processing)
ccpm install --serial

# Force re-download everything
ccpm install --force

# Install without updating lockfile
ccpm install --no-lock
```

### Install Behavior

1. **Reads ccpm.toml** - Parses manifest for dependencies
2. **Resolves versions** - Determines exact versions for each dependency
3. **Fetches sources** - Clones/updates Git repositories as needed
4. **Installs resources** - Copies .md files to local project directories
5. **Updates lockfile** - Writes/updates ccpm.lock with resolved versions
6. **Verifies integrity** - Checksums installed files

## 4. Update Command

**Purpose**: Update dependencies within version constraints

```bash
ccpm update [OPTIONS] [DEPENDENCIES...]
```

### Update Options

| Option | Description |
|--------|-------------|
| `--dry-run` | Preview changes without actually updating |
| `--serial` | Disable parallel processing (process sequentially) |

### Update Examples

```bash
# Update all dependencies within constraints (parallel by default)
ccpm update

# Update specific dependencies
ccpm update code-reviewer test-writer

# Preview updates without making changes
ccpm update --dry-run

# Sequential updates (disable parallel processing)
ccpm update --serial

# Dry-run update of specific dependency
ccpm update --dry-run code-reviewer
```

### Update Behavior

1. **Checks constraints** - Respects version constraints in ccpm.toml
2. **Finds updates** - Identifies available updates within constraints
3. **Shows changes** - Displays before/after version comparisons
4. **Updates dependencies** - Downloads and installs new versions
5. **Updates lockfile** - Writes new resolved versions to ccpm.lock

### Update Output

```bash
$ ccpm update
🔄 Updating dependencies...
✅ code-reviewer v1.0.0 → v1.2.0
✅ test-writer v2.1.0 → v2.2.0
✅ Updated ccpm.lock
```

## 5. List Command

**Purpose**: Show installed resources from lockfile or manifest

```bash
ccpm list [OPTIONS]
```

### List Options

| Option | Description |
|--------|-------------|
| `--manifest` | Show dependencies from ccpm.toml instead of lockfile |
| `--format [table\|json\|simple]` | Output format (default: table) |
| `--agents` | Show only agents |
| `--snippets` | Show only snippets |

### List Examples

```bash
# List from lockfile (shows installed versions)
ccpm list

# List from manifest (shows configured dependencies)
ccpm list --manifest

# JSON output
ccpm list --format json

# Simple output format
ccpm list --format simple

# Show only agents
ccpm list --agents

# Show only snippets
ccpm list --snippets
```

### List Output

**Table format (default):**
```bash
$ ccpm list
Installed dependencies:

Agents:
  code-reviewer  v1.0.0  (official)  agents/code-reviewer.md
  test-writer    v2.1.0  (community) agents/test-writer.md

Snippets:
  utils          v1.2.0  (official)  snippets/utils.md
```

**JSON format:**
```bash
$ ccpm list --format json
{
  "agents": [
    {
      "name": "code-reviewer",
      "version": "v1.0.0",
      "source": "official",
      "path": "agents/code-reviewer.md",
      "checksum": "sha256:abc123..."
    }
  ],
  "snippets": [
    {
      "name": "utils",
      "version": "v1.2.0",
      "source": "official", 
      "path": "snippets/utils.md",
      "checksum": "sha256:def456..."
    }
  ]
}
```

**Simple format:**
```bash
$ ccpm list --format simple
code-reviewer v1.0.0
test-writer v2.1.0
utils v1.2.0
```

## 6. Validate Command

**Purpose**: Validate ccpm.toml syntax and check dependencies

```bash
ccpm validate [OPTIONS]
```

### Validate Options

| Option | Description |
|--------|-------------|
| `--resolve` | Check if all dependencies can be resolved |
| `--check-lock` | Verify lockfile consistency with manifest |

### Validate Examples

```bash
# Basic manifest validation
ccpm validate

# Check dependency resolution
ccpm validate --resolve

# Verify lockfile consistency
ccpm validate --check-lock

# Full validation (all checks)
ccpm validate --resolve --check-lock
```

### Validate Output

**Success:**
```bash
$ ccpm validate
🔍 Validating ccpm.toml...
✅ Manifest structure is valid
✅ All sources are accessible
✅ Manifest validation passed
```

**With resolution check:**
```bash
$ ccpm validate --resolve
🔍 Validating ccpm.toml...
✅ Manifest structure is valid
✅ All dependencies can be resolved
✅ No version conflicts detected
✅ Validation passed
```

**With errors:**
```bash
$ ccpm validate --resolve
🔍 Validating ccpm.toml...
✅ Manifest structure is valid
❌ Version conflict: agent 'test-writer'
  - Constraint: v2.1.0
  - Available: v1.0.0, v1.1.0, v2.0.0
❌ Validation failed
```

## Advanced Usage


### Reproducible Installations

For team consistency and CI/CD, commit your lockfile:

```bash
git add ccpm.lock
git commit -m "Lock dependency versions"
```

Team members use exact versions:

```bash
# Install exact versions from lockfile
ccpm install --frozen
```

### Private Repositories

Use SSH authentication:

```toml
[sources]
private = "git@github.com:mycompany/private-agents.git"
```

Use HTTPS with token:

```toml
[sources]
private = "https://token:ghp_xxxx@github.com/mycompany/private-agents.git"
```

### Local Dependencies

Use local files during development:

```toml
[agents]
# Local file (no source needed)
my-local-agent = { path = "../local-agents/helper.md" }

# Mixed with remote
remote-agent = { source = "official", path = "agents/remote.md", version = "v1.0.0" }
```

### Performance Optimization

Parallel processing is enabled by default for faster operations:

```bash
# Default parallel installation
ccpm install

# Default parallel updates
ccpm update

# Sequential processing (if needed)
ccpm install --serial
ccpm update --serial
```

Parallel processing provides significant performance improvements when:
- Installing many dependencies
- Updating multiple resources
- Working with slow network connections
- Fetching from multiple Git repositories

## Command Reference Summary

### Primary Commands

| Command | Purpose | Key Options |
|---------|---------|-------------|
| `init` | Initialize a new manifest file | `--path`, `--force`, `--with-examples` |
| `add` | Add sources or dependencies | `source <name> <url>` or `dep <spec> --agent/--snippet` |
| `install` | Install dependencies from ccpm.toml | `--frozen`, `--serial`, `--force`, `--no-lock` |
| `update` | Update dependencies within constraints | `--dry-run`, `--serial`, `[deps...]` |
| `list` | Show installed resources | `--manifest`, `--format`, `--agents`, `--snippets` |
| `validate` | Check manifest validity | `--resolve`, `--check-lock` |

### Installation Options

- `--frozen` - Use exact versions from lockfile (fail if missing/outdated)
- `--serial` - Disable parallel processing (sequential operations)
- `--force` - Force re-download even if cached
- `--no-lock` - Don't write lockfile after installation

### Update Options

- `--dry-run` - Preview changes without updating
- `--serial` - Disable parallel processing (sequential operations)

### List Options

- `--manifest` - Show from ccpm.toml instead of lockfile
- `--format [table|json|simple]` - Output format
- `--agents` - Show only agents
- `--snippets` - Show only snippets

### Validate Options

- `--resolve` - Check if all dependencies can be resolved
- `--check-lock` - Verify lockfile consistency with manifest

## File Structure

Your project structure after installation:

```
my-project/
├── ccpm.toml        # Your dependency manifest (user-created)
├── ccpm.lock        # Resolved versions (auto-generated)
├── agents/          # Installed agents
│   ├── code-reviewer.md
│   └── test-writer.md
└── snippets/        # Installed snippets
    └── utils.md
```

Cache location (automatically managed):

```
~/.ccpm/
└── cache/
    └── sources/     # Cloned Git repositories
```

## Complete ccpm.toml Example

```toml
# Define Git repository sources
[sources]
official = "https://github.com/example-org/ccpm-official.git"
community = "https://github.com/example-org/ccpm-community.git"
private = "git@github.com:mycompany/private-agents.git"

# Project agents
[agents]
code-reviewer = { source = "official", path = "agents/code-reviewer.md", version = "v1.0.0" }
test-writer = { source = "community", path = "agents/test-writer.md", version = "v2.1.0" }
local-helper = { path = "../local-agents/helper.md" }
test-helper = { source = "official", path = "agents/test-helper.md", version = "v1.0.0" }

# Project snippets
[snippets]
utils = { source = "official", path = "snippets/utils.md", version = "v1.2.0" }
mock-utils = { source = "community", path = "snippets/mock-utils.md", version = "v1.0.0" }
```

## Troubleshooting

### Common Issues

**No manifest found:**
```bash
# Error: No ccpm.toml found in current directory
# Solution: Create a ccpm.toml file with your dependencies
```

**Version conflict:**
```bash
# Check for conflicts
ccpm validate --resolve

# Update version constraints in ccpm.toml
```

**Authentication failure:**
```bash
# For SSH: Ensure SSH keys are configured
ssh-add ~/.ssh/id_rsa

# For HTTPS: Use personal access tokens in URL
# https://token:ghp_xxxx@github.com/user/repo.git
```

**Checksum mismatch:**
```bash
# Force re-download to fix corrupted files
ccpm install --force
```

**Missing lockfile:**
```bash
# Generate lockfile from manifest
ccpm install
```

**Lockfile out of sync:**
```bash
# Regenerate lockfile
ccpm install

# Or validate consistency
ccpm validate --check-lock
```

**Performance issues:**
```bash
# Parallel processing is enabled by default
ccpm install
ccpm update

# If needed, try sequential processing
ccpm install --serial
ccpm update --serial
```

## Best Practices

1. **Always commit ccpm.lock** - Ensures reproducible builds
2. **Use version tags** - Prefer `version = "v1.0.0"` over branches
3. **Validate before commits** - Run `ccpm validate` to catch issues
4. **Use --frozen in CI/CD** - Ensures deterministic builds
5. **Parallel processing is default** - Use `--serial` only if needed
6. **Document custom sources** - Add comments in ccpm.toml for team

## Complete User Workflows

### Workflow 1: Starting a New Project

**Goal:** Set up CCPM in a new project with initial dependencies

```bash
# 1. Initialize CCPM with example entries
ccpm init --with-examples

# 2. Or initialize and add dependencies manually
ccpm init
ccpm add source official https://github.com/example-org/ccpm-official.git
ccpm add source community https://github.com/example-org/ccpm-community.git
ccpm add dep official:agents/code-reviewer.md@v1.0.0 --agent
ccpm add dep community:agents/test-writer.md@v2.1.0 --agent
ccpm add dep official:snippets/utils.md@v1.2.0 --snippet

# 3. Install dependencies
ccpm install

# 4. Verify installation
ccpm list
```

**Files Created:**
- `ccpm.lock` - Lockfile with exact resolved versions (auto-generated)
- `agents/code-reviewer.md` - Code reviewer agent (v1.0.0 from official)
- `agents/test-writer.md` - Test writer agent (v2.1.0 from community)
- `snippets/utils.md` - Utility snippets (v1.2.0 from official)

**Directory Structure After:**
```
project/
├── ccpm.toml        # Manifest (user-created)
├── ccpm.lock        # Lockfile (auto-generated, version 1)
├── agents/
│   ├── code-reviewer.md  # v1.0.0 with SHA256 checksum
│   └── test-writer.md     # v2.1.0 with SHA256 checksum
└── snippets/
    └── utils.md           # v1.2.0 with SHA256 checksum
```

### Workflow 2: Adding New Dependencies

**Goal:** Add new resources to your project

```bash
# 1. Add new dependencies using the add command
ccpm add dep official:agents/new-helper.md@v1.0.0 --agent
ccpm add dep community:agents/api-gen.md@v1.5.0 --agent --name api-generator
ccpm add dep official:snippets/new-utils.md@v1.1.0 --snippet

# 2. Install new dependencies
ccpm install

# 3. Verify all dependencies
ccpm list
```

**Files Created/Updated:**
- `ccpm.lock` - Updated with new dependency resolutions
- `agents/new-helper.md` - New helper agent (v1.0.0)
- `agents/api-generator.md` - API generator (v1.5.0)
- `snippets/new-utils.md` - New utilities (v1.1.0)

**Complete Structure:**
```
project/
├── ccpm.toml
├── ccpm.lock              # Updated with new deps
├── agents/                # All agents
│   ├── code-reviewer.md
│   ├── test-writer.md
│   ├── new-helper.md      # v1.0.0
│   └── api-generator.md   # v1.5.0
└── snippets/              # All snippets
    ├── utils.md
    └── new-utils.md       # v1.1.0
```

### Workflow 2: Updating Dependencies

**Goal:** Update to newer versions within constraints

```bash
# 1. Check current versions
ccpm list

# Output:
# code-reviewer  v1.0.0  (official)
# test-writer    v2.1.0  (community)
# utils          v1.2.0  (official)

# 2. Preview available updates
ccpm update --dry-run

# Output:
# Would update:
#   code-reviewer v1.0.0 → v1.2.0
#   test-writer v2.1.0 → v2.2.0
#   utils v1.2.0 (up to date)

# 3. Perform update
ccpm update

# 4. Verify new versions
ccpm list
```

**Files Modified:**
- `ccpm.lock` - Updated with new resolved versions
- `agents/code-reviewer.md` - Updated to v1.2.0 content
- `agents/test-writer.md` - Updated to v2.2.0 content
- `snippets/utils.md` - Unchanged (already latest)

**Version Changes:**
```
Before Update:
- code-reviewer: v1.0.0 (commit: abc123...)
- test-writer: v2.1.0 (commit: def456...)
- utils: v1.2.0 (commit: ghi789...)

After Update:
- code-reviewer: v1.2.0 (commit: jkl012...)
- test-writer: v2.2.0 (commit: mno345...)
- utils: v1.2.0 (commit: ghi789...) [unchanged]
```

### Workflow 3: Reproducible Team Installation

**Goal:** Ensure all team members have exact same versions

```bash
# Developer A: After making changes
# 1. Update dependencies
ccpm update

# 2. Commit lockfile
git add ccpm.lock
git commit -m "Update dependencies to latest versions"
git push

# Developer B: Getting exact versions
# 1. Pull latest changes
git pull

# 2. Install exact versions from lockfile
ccpm install --frozen

# 3. Verify versions match lockfile exactly
ccpm validate --check-lock
```

**Files Synchronized:**
- All `.md` files match exact checksums in lockfile
- Version commits match exactly between team members
- No version drift possible with `--frozen` flag

### Workflow 4: Production Deployment

**Goal:** Deploy with exact dependency versions

```bash
# 1. Clean install for production with exact versions
ccpm install --frozen

# Output:
# 📦 Installing dependencies (frozen)...
# ⚡ Using parallel processing (default)
# ✅ Installed code-reviewer v1.2.0 (sha256:abc...)
# ✅ Installed test-writer v2.2.0 (sha256:def...)
# ✅ Installed utils v1.2.0 (sha256:ghi...)

# 2. Verify all dependencies installed
ccpm list --format simple
```

**Files Created:**
```
production/
├── ccpm.toml
├── ccpm.lock
├── agents/
│   ├── code-reviewer.md
│   └── test-writer.md
└── snippets/
    └── utils.md
```

### Workflow 5: Working with Local Files

**Goal:** Mix local and remote dependencies

```bash
# 1. Create ccpm.toml with mixed sources
cat > ccpm.toml << 'EOF'
[sources]
official = "https://github.com/example-org/ccpm-official.git"

[agents]
# Remote agent with version
remote-agent = { source = "official", path = "agents/helper.md", version = "v1.0.0" }

# Local file (no version needed)
local-agent = { path = "../my-agents/custom.md" }

# Another local file
project-agent = { path = "./custom-agents/project-specific.md" }
EOF

# 2. Install mixed dependencies
ccpm install

# 3. List showing mixed sources
ccpm list
```

**Files Created:**
- `agents/remote-agent.md` - From Git repo (v1.0.0)
- `agents/local-agent.md` - Copied from `../my-agents/custom.md`
- `agents/project-agent.md` - Copied from `./custom-agents/project-specific.md`

**Lockfile Entry for Local:**
```toml
[[agents]]
name = "local-agent"
path = "../my-agents/custom.md"
checksum = "sha256:xyz123..."
installed_at = "agents/local-agent.md"
# Note: No version or source fields for local files
```

### Workflow 6: Partial Updates

**Goal:** Update only specific dependencies

```bash
# 1. Check current state
ccpm list --format simple

# Output:
# code-reviewer v1.0.0
# test-writer v2.1.0
# utils v1.2.0
# helper v1.0.0

# 2. Update only specific dependencies
ccpm update code-reviewer utils

# Output:
# 🔄 Updating specified dependencies...
# ✅ code-reviewer v1.0.0 → v1.3.0
# ✅ utils v1.2.0 → v1.3.0
# ⏭️  test-writer v2.1.0 (not updated)
# ⏭️  helper v1.0.0 (not updated)

# 3. Verify partial update
ccpm list --format simple
```

**Files Modified:**
- `ccpm.lock` - Only entries for code-reviewer and utils updated
- `agents/code-reviewer.md` - Updated to v1.3.0
- `snippets/utils.md` - Updated to v1.3.0
- `agents/test-writer.md` - Unchanged (v2.1.0)
- `agents/helper.md` - Unchanged (v1.0.0)

### Workflow 7: Validating Before Commits

**Goal:** Ensure manifest and dependencies are valid

```bash
# 1. Basic validation
ccpm validate

# Output:
# 🔍 Validating ccpm.toml...
# ✅ Manifest structure is valid
# ✅ All sources are accessible

# 2. Check dependency resolution
ccpm validate --resolve

# Output:
# 🔍 Validating with resolution...
# ✅ Manifest structure is valid
# ✅ All dependencies can be resolved:
#   - code-reviewer: Found v1.0.0 at official
#   - test-writer: Found v2.1.0 at community
#   - utils: Found v1.2.0 at official

# 3. Verify lockfile consistency
ccpm validate --check-lock

# Output:
# 🔍 Validating lockfile consistency...
# ✅ Lockfile is consistent with manifest
# ✅ All checksums verified
```

### Workflow 8: CI/CD Pipeline Setup

**Goal:** Automated testing and deployment

```bash
# .github/workflows/ci.yml or similar
name: CI
on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      
      - name: Install CCPM
        run: |
          curl -L https://github.com/ccpm/releases/latest/ccpm -o ccpm
          chmod +x ccpm
          
      - name: Install dependencies (frozen)
        run: ./ccpm install --frozen
        
      - name: Validate manifest
        run: ./ccpm validate --resolve --check-lock
        
      - name: List installed versions
        run: ./ccpm list --format json > installed-versions.json
        
      - name: Run tests
        run: npm test # or your test command
```

**Files Used in CI:**
- `ccpm.toml` - Defines dependencies (from repo)
- `ccpm.lock` - Ensures exact versions (from repo)
- Generated `.md` files - Installed by CI process

### Workflow 9: Troubleshooting Installation Issues

**Goal:** Debug and fix installation problems

```bash
# 1. Verbose installation for debugging
RUST_LOG=debug ccpm install

# 2. Force re-download if cache corrupted
ccpm install --force

# 3. Check what would be installed
ccpm install --dry-run

# 4. Verify file integrity
ccpm validate --check-lock

# 5. Clear cache and reinstall
rm -rf ~/.ccpm/cache
ccpm install --force

# 6. Install with detailed progress
ccpm install --verbose
```

**Diagnostic Information:**
- Check `~/.ccpm/cache/sources/` for cloned repositories
- Verify checksums in `ccpm.lock` match installed files
- Look for Git authentication issues in verbose output
- Check file permissions on target directories

## Environment Variables

CCPM respects several environment variables:

| Variable | Description | Example |
|----------|-------------|---------|  
| `CCPM_CACHE_DIR` | Override cache directory location | `/tmp/ccpm-cache` |
| `CCPM_NO_PROGRESS` | Disable progress bars and spinners | `1` |
| `RUST_LOG` | Set logging level | `debug`, `info`, `warn` |
| `NO_COLOR` | Disable colored output | `1` |

Example usage:
```bash
# Debug mode with custom cache
CCPM_CACHE_DIR=/tmp/test RUST_LOG=debug ccpm install

# Quiet mode for CI/CD
CCPM_NO_PROGRESS=1 ccpm install --quiet
```

## Troubleshooting

### Common Issues and Solutions

#### 1. Git Authentication Failures
```bash
# For private repositories, configure git credentials
git config --global credential.helper store

# Or use SSH authentication
[sources]
private = "git@github.com:company/private-repo.git"
```

#### 2. Lockfile Conflicts in Version Control
```bash
# After merge conflicts, regenerate lockfile
rm ccpm.lock
ccpm install
```

#### 3. Cache Corruption
```bash
# Clear cache and reinstall
ccpm cache clean --all
ccpm install --force
```

#### 4. Version Resolution Failures
```bash
# Check available versions
ccpm validate --resolve --verbose

# Use specific version to avoid conflicts
ccpm add dep agents my-agent --version "=1.2.3"
```

## Getting Help

For more details, see the full documentation in README.md or run:

```bash
ccpm --help
ccpm <command> --help
```

For specific command help:
```bash
ccpm init --help
ccpm add --help
ccpm install --help
ccpm update --help
ccpm list --help
ccpm validate --help
ccpm cache --help
ccpm config --help
```

## Additional Resources

- **GitHub Repository**: [https://github.com/aig787/ccpm](https://github.com/aig787/ccpm)
- **Issue Tracker**: [https://github.com/aig787/ccpm/issues](https://github.com/aig787/ccpm/issues)
- **Contributing Guide**: See [CONTRIBUTING.md](CONTRIBUTING.md)
- **License**: MIT License - see [LICENSE.md](LICENSE.md)