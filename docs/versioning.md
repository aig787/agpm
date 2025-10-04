# Versioning Guide

AGPM uses Git-based versioning where version constraints apply at the repository level, not individual files. This guide explains how versioning works and best practices for managing dependencies.

## Core Concepts

### Repository-Level Versioning

When you specify a version (e.g., `version = "v1.0.0"`), you're referencing a Git tag on the entire repository, not individual files. All resources from that repository at that tag share the same version.

### Supported Version References

AGPM supports multiple ways to reference specific points in a repository's history:

- **Git Tags** (recommended): Semantic versions like `v1.0.0`, `v2.1.3`
- **Git Branches**: Branch names like `main`, `develop`, `feature/xyz`
- **Git Commits**: Specific commit hashes like `abc123def`
- **Special Keywords**: `latest` (newest tag), `*` (any version)

## Version Syntax

### Exact Versions

```toml
# With or without 'v' prefix
agent1 = { source = "community", path = "agents/agent1.md", version = "1.0.0" }
agent2 = { source = "community", path = "agents/agent2.md", version = "v1.0.0" }
```

### Semantic Version Ranges

AGPM follows standard semver conventions used by Cargo and npm:

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

### Examples

```toml
# Semver ranges (enhanced constraint support)
agent3 = { source = "community", path = "agents/agent3.md", version = "^1.2.0" }  # 1.2.0, 1.3.0, etc. (not 2.0.0)
agent4 = { source = "community", path = "agents/agent4.md", version = "~1.2.0" }  # 1.2.0, 1.2.1, etc. (not 1.3.0)
agent5 = { source = "community", path = "agents/agent5.md", version = ">=1.0.0" } # At least 1.0.0
agent6 = { source = "community", path = "agents/agent6.md", version = ">=1.0.0, <2.0.0" } # Complex range

# Multiple constraints with AND logic
agent7 = { source = "community", path = "agents/agent7.md", version = ">=1.2.0, <2.0.0, !=1.5.0" } # Exclude specific version

# Special keywords
latest-agent = { source = "community", path = "agents/latest.md", version = "latest" }
beta-agent = { source = "community", path = "agents/beta.md", version = "latest-prerelease" }
any-agent = { source = "community", path = "agents/any.md", version = "*" }
```

### Enhanced Constraint Support

AGPM v0.3.2+ includes improved constraint parsing and resolution:

- **Complex ranges**: Support for multiple constraints with AND logic
- **Intelligent tag matching**: Better handling of tag prefixes (v1.0.0 vs 1.0.0)
- **Validation**: Robust constraint validation before resolution
- **Performance optimization**: Batch resolution minimizes repository operations

## Version Reference Types

### Git Tags (Recommended)

Git tags provide stable, semantic version numbers:

```toml
[agents]
# Exact version using a git tag
stable-agent = { source = "community", path = "agents/example.md", version = "v1.0.0" }

# Version ranges using semantic versioning
compatible-agent = { source = "community", path = "agents/helper.md", version = "^1.2.0" }  # 1.2.0 to <2.0.0
patch-only-agent = { source = "community", path = "agents/util.md", version = "~1.2.3" }    # 1.2.3 to <1.3.0
```

**How it works**: When AGPM resolves `version = "v1.0.0"`, it:
1. Looks for a Git tag named `v1.0.0` in the repository
2. Checks out the repository at that tag
3. Retrieves the specified file from that tagged state

### Git Branches

Branches reference the latest commit on a specific branch:

```toml
[agents]
# Track the main branch (updates with each install/update)
dev-agent = { source = "community", path = "agents/dev.md", branch = "main" }

# Track a feature branch
feature-agent = { source = "community", path = "agents/new.md", branch = "feature/new-capability" }
```

⚠️ **Important**: Branch references are mutable - they update to the latest commit each time you run `agpm update`. Use tags for stable, reproducible builds.

### Git Commit Hashes

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

### Local Resources (No Versioning)

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
```

## Version Resolution

AGPM v0.3.2+ uses a centralized, high-performance version resolution system with the VersionResolver module:

### Centralized Resolution Process

1. **Collection Phase**: VersionResolver gathers all unique (source, version) pairs from all dependencies
2. **Deduplication**: Identical version requirements are automatically deduplicated for efficiency
3. **Batch Resolution**: Single operation per repository resolves all required versions to commit SHAs
4. **Constraint Matching**: Enhanced semver engine finds best matching tags for complex constraints
5. **SHA Validation**: All resolved SHAs are validated as 40-character hexadecimal strings
6. **Worktree Optimization**: SHA-based worktree creation maximizes reuse for identical commits
7. **Lock Generation**: Record exact commit SHAs and resolved references in `agpm.lock`

### Enhanced Performance Benefits

- **Minimal Git Operations**: Single fetch per repository per command execution
- **Maximum Deduplication**: Multiple dependencies with same resolved SHA share one worktree
- **Parallel Safety**: Independent SHA-based worktrees enable conflict-free concurrent operations
- **Command-Instance Caching**: Repository fetch operations cached within single command execution

### Version Selection Examples

Given available tags: `v1.0.0`, `v1.1.0`, `v1.2.0`, `v2.0.0`

| Constraint          | Selected Version | Explanation           |
|---------------------|------------------|-----------------------|
| `"^1.0.0"`          | `v1.2.0`         | Highest 1.x.x version |
| `"~1.0.0"`          | `v1.0.0`         | Only 1.0.x allowed    |
| `">=1.1.0, <2.0.0"` | `v1.2.0`         | Highest within range  |
| `"latest"`          | `v2.0.0`         | Newest stable tag     |
| `">1.0.0"`          | `v2.0.0`         | Highest available     |

## Lockfile and Reproducibility

The `agpm.lock` file ensures reproducible installations by recording exact resolution results from the VersionResolver:

```toml
[[agents]]
name = "example-agent"
source = "community"
path = "agents/example.md"
version = "^1.0.0"                    # Original constraint from agpm.toml
resolved_commit = "abc123def456..."   # Exact commit SHA (40 characters)
resolved_version = "v1.2.3"          # Actual tag that satisfied constraint
```

### Enhanced Lockfile Information

With the centralized VersionResolver, the lockfile provides:

- **Original constraint** (`version`): What was requested in `agpm.toml` (e.g., `^1.0.0`)
- **Resolved commit** (`resolved_commit`): Exact Git commit SHA determined by VersionResolver
- **Resolved version** (`resolved_version`): The specific tag/branch that satisfied the constraint
- **SHA-based reproducibility**: Same SHA always produces identical installations
- **Worktree optimization data**: Enables efficient cache reuse on subsequent installs

### Lockfile Staleness Checks

AGPM tracks whether `agpm.lock` still matches the manifest and the resolution rules that produced it. Both `agpm install` and `agpm validate --check-lock` run the same validation logic:

- **Always checked**: duplicate lockfile entries (corruption) and changed source URLs (security risk).
- **Strict-mode checks** (`agpm install`, `agpm validate --check-lock`): missing dependencies that now exist in the manifest, version constraint changes, or path changes compared to what the lockfile previously captured.
- **Lenient mode** (`agpm install --frozen`): only the always-checked issues; anything else causes the command to exit instead of regenerating.

When validation reports a staleness reason, run `agpm install` (without `--frozen`) to regenerate the lockfile. The resolver reuses prior resolutions whenever possible, so versions stay unchanged unless the manifest or upstream reference moved.

## Common Scenarios

### Development vs Production

```toml
# Development - track latest changes
[agents.dev]
cutting-edge = { source = "community", path = "agents/new.md", branch = "main" }

# Production - stable versions only
[agents]
stable = { source = "community", path = "agents/proven.md", version = "^1.0.0" }
```

### Gradual Updates

```toml
# Start conservative
agent = { source = "community", path = "agents/example.md", version = "~1.2.0" }  # Patches only

# After testing, allow minor updates
agent = { source = "community", path = "agents/example.md", version = "^1.2.0" }  # Compatible updates

# Eventually, allow any 1.x version
agent = { source = "community", path = "agents/example.md", version = ">=1.2.0, <2.0.0" }
```

### Mixed Sources

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

## Local Dependencies

### Local Directory Sources

Use local directories as sources without requiring Git:

```toml
[sources]
# Local directory as a source - no Git required
local-deps = "./dependencies"
shared-resources = "../shared-resources"
absolute-local = "/home/user/agpm-resources"

[agents]
# Dependencies from local directory sources don't need versions
local-agent = { source = "local-deps", path = "agents/helper.md" }
shared-agent = { source = "shared-resources", path = "agents/common.md" }
```

**Security Note**: For security, local paths are restricted to:
- Within the current project directory
- Within the AGPM cache directory (`~/.agpm/cache`)
- Within `/tmp` for testing

### Direct File Paths

Reference individual files directly without a source:

```toml
# Direct file paths - NO version support
local-agent = { path = "../agents/my-agent.md" }
local-snippet = { path = "./snippets/util.md" }

# ❌ INVALID - versions not allowed for direct paths
# local-versioned = { path = "../agents/agent.md", version = "v1.0.0" }  # ERROR!
```

### Local Git Repositories

Use `file://` URLs to reference local git repositories with full git functionality:

```toml
[sources]
# Local git repository with full version support
local-repo = "file:///home/user/my-git-repo"

[agents]
# Can use versions, branches, tags with local git repos
local-git-agent = { source = "local-repo", path = "agents/agent.md", version = "v1.0.0" }
local-branch-agent = { source = "local-repo", path = "agents/dev.md", branch = "develop" }
```

## Best Practices

1. **Use Semantic Version Tags**: Tag releases with semantic versions (`v1.0.0`, `v2.1.3`)
2. **Prefer Tags Over Branches**: Tags are immutable; branches change over time
3. **Use Caret Ranges**: `^1.0.0` allows compatible updates while preventing breaking changes
4. **Lock for Production**: Commit `agpm.lock` and use `--frozen` flag in CI/CD
5. **Document Breaking Changes**: Use major version bumps (v1.x.x → v2.x.x) for breaking changes
6. **Test Before Updating**: Use `agpm update --dry-run` to preview changes

## Automated Releases

AGPM itself uses [semantic-release](https://semantic-release.gitbook.io/) for automated versioning. Every push to `main` triggers:

1. **Commit analysis** to determine version bump based on [Conventional Commits](https://www.conventionalcommits.org/):
   - `fix:` → Patch release (0.0.X)
   - `feat:` → Minor release (0.X.0) 
   - Breaking changes → Major release (X.0.0)
   - All other types (`docs:`, `style:`, `refactor:`, `test:`, `build:`, `ci:`, `chore:`) → Patch release

2. **Automatic release** with:
   - Version updates in `Cargo.toml` and `Cargo.lock`
   - Changelog generation from conventional commits
   - GitHub release with cross-platform binaries (Linux x86_64/ARM64, macOS x86_64/ARM64, Windows x86_64)
   - Publishing to crates.io
   - Pre-release support on `alpha` and `beta` branches

3. **Binary Assets** available for each release:
   - `agpm-x86_64-linux.tar.gz` - Linux x86_64
   - `agpm-aarch64-linux.tar.gz` - Linux ARM64
   - `agpm-x86_64-macos.tar.gz` - macOS Intel
   - `agpm-aarch64-macos.tar.gz` - macOS Apple Silicon
   - `agpm-x86_64-windows.zip` - Windows x86_64

## Troubleshooting

### Version Conflicts

```bash
# Check for conflicts
agpm validate --resolve

# Update version constraints in agpm.toml to resolve
```

If no shared version exists, synchronize the manifest constraints (for example, bump every dependent to the same tag) or pin the dependency to a specific commit with `rev = "<sha>"`. You can inspect the resolver output with `agpm validate --resolve --format json` to see which dependency introduced the incompatible requirement.

### Lockfile Out of Sync

```bash
# Regenerate lockfile
agpm install

# Use exact lockfile versions (no updates)
agpm install --frozen
```

### Finding Available Versions

Check the Git repository's tags to see available versions:

```bash
git ls-remote --tags https://github.com/org/repo.git
```
