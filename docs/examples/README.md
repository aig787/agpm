# AGPM Configuration Examples

This directory contains example AGPM configuration files demonstrating various features and use cases.

## Available Examples

### [basic-project.toml](basic-project.toml)
A minimal configuration showing the most common use cases:
- Basic source definition
- Simple dependencies with version constraints
- Local file dependencies
- All six resource types

### [multi-tool.toml](multi-tool.toml)
Managing resources for multiple AI assistants:
- Claude Code and OpenCode resources
- Default tool configuration
- Shared snippets across tools
- Tool-specific resource routing

### [pattern-matching.toml](pattern-matching.toml)
Bulk installation using glob patterns:
- Wildcard patterns (`*`, `**`)
- Directory structure control (flatten option)
- Recursive patterns
- Complex pattern examples

### [patches.toml](patches.toml)
Customizing resources without forking:
- Project-level patches (team-shared)
- Field overrides for agents, commands, etc.
- Model and behavior customization
- Configuration examples

### [patches-private.toml](patches-private.toml)
Private configuration example (not committed to git):
- Personal API keys and endpoints
- Development-specific overrides
- Sensitive configuration
- Extending project patches

### [advanced.toml](advanced.toml)
Complex features and best practices:
- Multiple sources
- Transitive dependencies
- Mixed versioning strategies
- Environment-specific configuration
- Advanced patch examples
- Comprehensive patterns

## Quick Start

1. Copy the example that best matches your use case
2. Rename it to `agpm.toml` in your project root
3. Modify the sources and dependencies for your needs
4. Run `agpm install` to install the dependencies

## Key Concepts

### Version Constraints
- `^1.2.3` - Compatible updates (1.x.x)
- `~1.2.3` - Patch updates only (1.2.x)
- `v1.2.3` - Exact version
- `latest` - Latest stable tag
- `branch = "main"` - Track a branch
- `rev = "abc123"` - Pin to commit

### Pattern Matching
- `*` - Matches any characters except `/`
- `**` - Matches any number of directories
- `?` - Single character wildcard
- `[abc]` - Character sets
- `{a,b}` - Alternatives

### Directory Structure
- `flatten = true` - Use only filename (default for agents/commands)
- `flatten = false` - Preserve source directory structure (default for snippets)

### Tool Specification
- No `tool` field - Uses default for resource type
- `tool = "claude-code"` - Claude Code resources
- `tool = "opencode"` - OpenCode resources (alpha)
- `tool = "agpm"` - Shared infrastructure

## Best Practices

1. **Start simple** - Use basic-project.toml as a template
2. **Version carefully** - Use `^` for flexibility, exact versions for stability
3. **Document patches** - Add comments explaining why patches are needed
4. **Keep secrets separate** - Use agpm.private.toml for sensitive data
5. **Use patterns wisely** - Great for related resources, but be specific
6. **Test incrementally** - Add a few dependencies at a time

## See Also

- [User Guide](../user-guide.md) - Getting started with AGPM
- [Dependencies Guide](../dependencies.md) - Detailed dependency management
- [Multi-Tool Support](../multi-tool-support.md) - Working with multiple AI assistants
- [Configuration Guide](../configuration.md) - Global and project configuration
- [Manifest Reference](../manifest-reference.md) - Complete field reference