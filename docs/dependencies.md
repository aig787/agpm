# Dependencies Guide

This guide covers dependency management in AGPM, including version constraints, transitive dependencies, conflict resolution, validation, and patches.

## Table of Contents

- [Adding Dependencies](#adding-dependencies)
- [Version Constraints](#version-constraints)
- [Transitive Dependencies](#transitive-dependencies)
- [Dependency Validation](#dependency-validation)
- [Conflict Resolution](#conflict-resolution)
- [Patches and Overrides](#patches-and-overrides)
- [Lockfile Management](#lockfile-management)
- [Best Practices](#best-practices)

## Adding Dependencies

### Method 1: Edit agpm.toml directly

Add dependencies to your manifest file:

```toml
[agents]
my-agent = { source = "community", path = "agents/helper.md", version = "v1.0.0" }

[snippets]
utils = { source = "community", path = "snippets/utils.md", version = "^2.0.0" }
```

Then install:
```bash
agpm install
```

### Method 2: Use CLI commands

```bash
# Add a Git source repository
agpm add source community https://github.com/aig787/agpm-community.git

# Add dependencies from Git sources
agpm add dep agent community:agents/rust-expert.md@v1.0.0
agpm add dep snippet community:snippets/react.md --name react-utils

# Add local file dependencies
agpm add dep agent ./local-agents/helper.md --name my-helper
agpm add dep script ../shared/scripts/build.sh

# Add pattern dependencies (bulk installation)
agpm add dep agent "community:agents/ai/*.md@v1.0.0" --name ai-agents

# Batch mode: Add multiple dependencies without installing
agpm add dep agent --no-install community:agents/rust-expert.md@v1.0.0
agpm add dep snippet --no-install community:snippets/utils.md@v1.0.0
agpm install  # Install all dependencies at once
```

See the [Command Reference](command-reference.md#add-dependency) for all supported dependency formats.

## Version Constraints

AGPM uses semantic versioning constraints similar to Cargo:

| Constraint | Description | Example Range |
|------------|-------------|---------------|
| `^1.2.3` | Compatible updates | `>=1.2.3, <2.0.0` |
| `~1.2.3` | Patch updates only | `>=1.2.3, <1.3.0` |
| `>=1.0.0, <2.0.0` | Custom range | As specified |
| `latest` | Latest stable tag | Latest semver tag |
| `*` | Any version | Any tag |
| `v1.0.0` | Exact version | Exactly v1.0.0 |

### Branch and Commit References

```toml
# Track a branch (mutable)
dev-agent = { source = "repo", path = "agent.md", branch = "main" }

# Pin to specific commit (immutable)
stable-agent = { source = "repo", path = "agent.md", rev = "abc123def" }

# Local path (no versioning)
local-agent = { path = "./local-agents/helper.md" }
```

## Transitive Dependencies

Resources can declare their own dependencies within their files, creating a complete dependency graph.

### Declaring Dependencies in Resource Files

**Markdown files (.md)** use YAML frontmatter:

```markdown
---
title: My Agent
description: An example agent with dependencies
dependencies:
  agents:
    - path: ./helper.md
      version: v1.0.0
    - path: ../shared/common.md
  snippets:
    - path: ./utils.md
      version: v2.0.0
      tool: claude-code  # Optional: specify target tool
      name: custom_utils  # Optional: custom template variable name
  commands:
    - path: commands/deploy.md
      flatten: true  # Optional: flatten directory structure
---

# Agent content here
```

**JSON files (.json)** use a top-level `dependencies` field:

```json
{
  "events": ["SessionStart"],
  "type": "command",
  "command": "echo 'Starting session'",
  "dependencies": {
    "commands": [
      {"path": "./setup.md", "version": "v1.0.0"},
      {"path": "../shared/common-setup.md"}
    ]
  }
}
```

### Supported Dependency Fields

- `path` (required): Path to the dependency file within the source repository
- `version` (optional): Version constraint (inherits from parent if not specified)
- `tool` (optional): Target tool (`claude-code`, `opencode`, `agpm`). If not specified:
  - Inherits from parent if parent's tool supports this resource type
  - Falls back to default tool for this resource type
- `name` (optional): Custom name for template variable references (defaults to sanitized filename)
- `flatten` (optional): For pattern dependencies, controls directory structure preservation

### Key Features

- **Path-only transitive support**: Dependencies without Git sources support transitive dependencies
- **File-relative paths**: Paths starting with `./` or `../` are resolved relative to the parent resource file
- **Templated dependency paths**: Use `{{ agpm.project.* }}` variables in dependency paths for dynamic resolution
- **Graph-based resolution**: Topological ordering ensures correct installation order
- **Circular dependency detection**: Prevents infinite loops
- **Version inheritance**: Dependencies inherit source and version from parent when not specified

### Lockfile Format

Dependencies are tracked in `agpm.lock` using the format `resource_type/name@version`:

```toml
[[commands]]
name = "my-command"
path = "commands/my-command.md"
dependencies = [
    "agents/helper@v1.0.0",
    "snippets/utils@v2.0.0"
]
```

## Dependency Validation

AGPM provides comprehensive validation and automatic conflict resolution:

```bash
# Basic manifest validation
agpm validate

# Full validation with all checks
agpm validate --resolve --sources --paths --check-lock

# Validate template rendering and file references
agpm validate --render

# JSON output for CI/CD integration
agpm validate --format json

# Strict mode - fail on any warning
agpm validate --strict
```

### Template and File Reference Validation

The `--render` flag provides additional validation:

- **Template Rendering**: Validates all markdown resources with template syntax (`{{`, `{%`, `{#`)
- **File Reference Auditing**: Checks that all file references within markdown content exist
  - Validates markdown links: `[text](path.md)`
  - Validates direct file paths: `.agpm/snippets/file.md`, `docs/guide.md`
  - Ignores URLs, code blocks, and absolute paths
  - Reports broken references with clear error messages

Use in CI/CD pipelines to catch broken cross-references before deployment:

```bash
# Validate everything before deployment
agpm validate --render --strict
```

## Conflict Resolution

### What is a Conflict?

A conflict occurs when the same resource (same source and path) is required at different versions:

- **Direct conflict**: Your manifest requires `helper.md@v1.0.0` and `helper.md@v2.0.0`
- **Transitive conflict**: Agent A depends on `helper.md@v1.0.0`, Agent B depends on `helper.md@v2.0.0`

### Automatic Resolution Strategy

When conflicts are detected, AGPM automatically resolves them:

1. **Specific over "latest"**: If one version is specific and another is "latest", use the specific version
2. **Higher version**: When both are specific versions, use the higher version
3. **Transparent logging**: All conflict resolutions are logged for visibility

Example conflict resolution:

```text
Direct dependencies:
  - app-agent requires helper.md v1.0.0
  - tool-agent requires helper.md v2.0.0
→ Resolved: Using helper.md v2.0.0 (higher version)

Transitive dependencies:
  - agent-a → depends on → helper.md v1.5.0
  - agent-b → depends on → helper.md v2.0.0
→ Resolved: Using helper.md v2.0.0 (higher version)
```

### When Auto-Resolution Fails

If constraints have no compatible version, installation stops with an error:

```text
Error: Version conflict for agents/helper.md
  requested: v1.0.0 (manifest)
  requested: v2.0.0 (transitive via agents/deploy.md)
  resolution: no compatible tag satisfies both constraints
```

Solutions:
- Pin the manifest entry to a single version and run `agpm install`
- Split competing resources into separate manifests
- Override transitive dependencies by forking the source repo
- Add `filename` or `target` overrides to prevent path conflicts

Use debug logging to see the exact dependency chain:
```bash
RUST_LOG=debug agpm install
```

### Circular Dependencies

AGPM detects and prevents circular dependencies in the dependency graph:

```text
Error: Circular dependency detected: A → B → C → A
```

## Patches and Overrides

Override resource fields without forking upstream repositories. Perfect for customizing model settings, temperature, or any YAML/JSON field.

### Project-Level Patches

Add to `agpm.toml` (committed to git):

```toml
[agents]
rust-expert = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0" }

[patch.agents.rust-expert]
model = "claude-3-haiku"       # Override model selection
temperature = "0.8"            # Adjust temperature
max_tokens = "4096"            # Set token limit
```

### Private Patches

Add to `agpm.private.toml` (in .gitignore):

```toml
[patch.agents.rust-expert]
api_key = "${MY_API_KEY}"      # Personal credentials
custom_endpoint = "https://my-proxy.internal"
debug_mode = "true"            # Personal development settings
```

### Key Features

- Works with both Markdown (YAML frontmatter) and JSON files
- Private patches extend project patches (no conflicts)
- Different fields combine; same field in both - private silently overrides project
- Tracked in lockfile for reproducibility
- See `agpm list` for "(patched)" indicator

### Patch Application Order

1. Original resource content loaded
2. Project patches applied (`agpm.toml`)
3. Private patches applied (`agpm.private.toml`)
4. Final content written to target location

## Lockfile Management

### Lockfile Purpose

The lockfile (`agpm.lock`) records exact resolved versions for reproducible installations:
- Generated automatically by `agpm install`
- Should be committed to version control
- Ensures team members get identical versions

### Lifecycle and Guarantees

- `agpm install` always re-runs dependency resolution using the current manifest and lockfile
- Versions do **not** automatically advance just because you reinstalled
- Resolution only diverges when:
  - The manifest changed
  - A tag/branch now points somewhere else
  - A dependency was missing from the previous lockfile

### Installation Modes

```bash
# Standard mode - updates lockfile as needed
agpm install

# Frozen mode - requires exact lockfile match (CI/CD)
agpm install --frozen

# No-lock mode - verify installs without updating lockfile
agpm install --no-lock
```

### Detecting Staleness

AGPM automatically checks for stale lockfiles:
- Duplicate entries or source URL drift (security-critical)
- Manifest entries missing from the lockfile
- Version/path changes that haven't been resolved

Check lockfile status:
```bash
agpm validate --check-lock
```

If stale, regenerate with:
```bash
agpm install  # Without --frozen flag
```

## Best Practices

### Version Constraints

1. **Use semantic constraints** (`^1.0.0`) for flexibility with safety
2. **Pin exact versions** (`v1.0.0`) for critical dependencies
3. **Avoid `latest`** in production - too unpredictable
4. **Use branches** (`branch = "main"`) only for active development

### Dependency Organization

1. **Group related dependencies** in the manifest for clarity
2. **Document why** specific versions or constraints are needed
3. **Use patterns** (`agents/*.md`) to manage related resources together
4. **Keep local and remote sources separate** for easier management

### Team Collaboration

1. **Always commit agpm.lock** to version control
2. **Use `--frozen` in CI/CD** to ensure exact reproducibility
3. **Run `agpm outdated` regularly** to stay current
4. **Document patches** in comments for team visibility

### Security

1. **Never commit agpm.private.toml** - add to .gitignore
2. **Store credentials in global config** (`~/.agpm/config.toml`)
3. **Use environment variables** for sensitive values in patches
4. **Validate sources** before adding them to your manifest

### Performance

1. **Use patterns for bulk operations** instead of individual entries
2. **Leverage parallelism** with `--max-parallel` for large installs
3. **Clean cache periodically** with `agpm cache clean`
4. **Use `--no-cache` sparingly** - only when debugging

## Troubleshooting Dependencies

### Common Issues

**Version not found:**
```bash
# List available versions
git ls-remote --tags <source-url>

# Check version constraints
agpm validate --resolve
```

**Transitive dependency conflicts:**
```bash
# See full dependency graph
RUST_LOG=debug agpm validate --resolve

# Check specific dependency chain
agpm validate --resolve --format json | jq '.dependencies'
```

**Slow resolution:**
```bash
# Increase parallelism
agpm install --max-parallel 16

# Skip cache for fresh fetch
agpm install --no-cache
```

For more troubleshooting help, see the [Troubleshooting Guide](troubleshooting.md).