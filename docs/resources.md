# Resources Guide

AGPM manages six types of resources for Claude Code, divided into two categories based on how they're integrated.

## Resource Categories

### Direct Installation Resources

These resources are copied directly to their target directories and used as standalone files:

- **Agents** - AI assistant configurations
- **Snippets** - Reusable code templates
- **Commands** - Claude Code slash commands
- **Scripts** - Executable automation files

### Configuration-Merged Resources

These resources are installed to `.claude/agpm/` and then their configurations are merged into Claude Code's settings:

- **Hooks** - Event-based automation
- **MCP Servers** - Model Context Protocol servers

## Resource Types

### Agents

AI assistant configurations with prompts and behavioral definitions.

**Default Location**: `.claude/agents/`

**Path Preservation**: AGPM preserves the source directory structure during installation.

**Examples**:
```toml
[agents]
# Simple path - installed as .claude/agents/rust-expert.md
rust-expert = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0" }

# Nested path - installed as .claude/agents/ai/code-reviewer.md (preserves ai/ subdirectory)
code-reviewer = { source = "community", path = "agents/ai/code-reviewer.md", version = "v1.0.0" }

# Deeply nested - installed as .claude/agents/specialized/rust/expert.md
rust-specialist = { source = "community", path = "agents/specialized/rust/expert.md", version = "v1.0.0" }

# Local path with structure - preserves relative path from source
local-agent = { path = "../local-agents/ai/helper.md" }  # → .claude/agents/ai/helper.md
```

### Snippets

Reusable code templates and documentation fragments.

**Default Location**: `.claude/agpm/snippets/`

**Example**:
```toml
[snippets]
react-component = { source = "community", path = "snippets/react-component.md", version = "v1.2.0" }
utils = { source = "local-deps", path = "snippets/utils.md" }
```

### Commands

Claude Code slash commands that extend functionality.

**Default Location**: `.claude/commands/`

**Example**:
```toml
[commands]
deploy = { source = "community", path = "commands/deploy.md", version = "v2.0.0" }
lint = { source = "tools", path = "commands/lint.md", branch = "main" }
```

### Scripts

Executable files (.sh, .js, .py, etc.) that can be run by hooks or independently.

**Default Location**: `.claude/agpm/scripts/`

**Example**:
```toml
[scripts]
security-check = { source = "security-tools", path = "scripts/security.sh", version = "v1.0.0" }
build = { source = "tools", path = "scripts/build.js", version = "v2.0.0" }
validate = { source = "local", path = "scripts/validate.py" }
```

Scripts must be executable and can be written in any language supported by your system.

### Hooks

Event-based automation configurations for Claude Code. JSON files that define when to run scripts.

**Default Location**: `.claude/agpm/hooks/`
**Configuration**: Automatically merged into `.claude/settings.local.json`

#### Hook Structure

```json
{
  "events": ["PreToolUse"],
  "matcher": "Bash|Write|Edit",
  "type": "command",
  "command": ".claude/agpm/scripts/security-check.sh",
  "timeout": 5000,
  "description": "Security validation before file operations"
}
```

#### Available Events

- `PreToolUse` - Before a tool is executed
- `PostToolUse` - After a tool completes
- `UserPromptSubmit` - When user submits a prompt
- `UserPromptReceive` - When prompt is received
- `AssistantResponseReceive` - When assistant responds

#### Example Configuration

```toml
[hooks]
pre-bash = { source = "security-tools", path = "hooks/pre-bash.json", version = "v1.0.0" }
file-guard = { source = "security-tools", path = "hooks/file-guard.json", version = "v1.0.0" }
```

### MCP Servers

Model Context Protocol servers that extend Claude Code's capabilities with external tools and APIs.

**Default Location**: `.claude/agpm/mcp-servers/`
**Configuration**: Automatically merged into `.mcp.json`

#### MCP Server Structure

```json
{
  "command": "npx",
  "args": [
    "-y",
    "@modelcontextprotocol/server-filesystem",
    "--root",
    "./data"
  ],
  "env": {
    "NODE_ENV": "production"
  }
}
```

#### Example Configuration

```toml
[mcp-servers]
filesystem = { source = "community", path = "mcp-servers/filesystem.json", version = "v1.0.0" }
github = { source = "community", path = "mcp-servers/github.json", version = "v1.2.0" }
postgres = { source = "local-deps", path = "mcp-servers/postgres.json" }
```

## Configuration Merging

### How It Works

Configuration-merged resources (Hooks and MCP Servers) follow a two-step process:

1. **File Installation**: JSON configuration files are installed to `.claude/agpm/`
2. **Configuration Merging**: Settings are automatically merged into Claude Code's configuration files
3. **Non-destructive Updates**: AGPM preserves user-configured entries while managing its own
4. **Tracking**: AGPM adds metadata to track which entries it manages

### Example: Merged .mcp.json

After installation, `.mcp.json` contains both user and AGPM-managed servers:

```json
{
  "mcpServers": {
    "my-manual-server": {
      "command": "node",
      "args": ["./custom.js"]
    },
    "filesystem": {
      "command": "npx",
      "args": [
        "-y",
        "@modelcontextprotocol/server-filesystem",
        "--root",
        "./data"
      ],
      "_agpm": {
        "managed": true,
        "config_file": ".claude/agpm/mcp-servers/filesystem.json",
        "installed_at": "2024-01-15T10:30:00Z"
      }
    }
  }
}
```

## File Naming and Path Preservation

### Path Preservation

AGPM preserves the source directory structure during installation. Files are named based on their source path, maintaining the original organization:

```toml
[agents]
# Source file: agents/ai/code-reviewer.md
# Installed as: .claude/agents/ai/code-reviewer.md (preserves ai/ subdirectory)
code-reviewer = { source = "community", path = "agents/ai/code-reviewer.md" }
```

### Benefits of Path Preservation

- **Maintains organization**: Source repository structure is preserved in your project
- **Avoids conflicts**: Files with the same name in different directories won't collide
- **Clear provenance**: Installation path reflects the original source location
- **Pattern matching**: Glob patterns naturally preserve directory hierarchies

### Custom Filenames

You can still use custom filenames with the `filename` option (the dependency name is no longer used):

```toml
[agents]
# Custom filename without changing path structure
reviewer = {
    source = "community",
    path = "agents/ai/code-reviewer.md",
    filename = "my-reviewer.md"
}
# Installed as: .claude/agents/ai/my-reviewer.md (preserves ai/ directory)
```

## Custom Installation Paths

### Global Target Directories

Override default installation directories for all resources of a type:

```toml
[target]
agents = ".claude/agents"           # Default
snippets = ".claude/agpm/snippets"  # Default
commands = ".claude/commands"        # Default
scripts = ".claude/agpm/scripts"    # Default
hooks = ".claude/agpm/hooks"        # Default
mcp-servers = ".claude/agpm/mcp-servers"  # Default

# Or use custom paths
agents = "custom/agents"
snippets = "resources/snippets"
```

**Path preservation applies to custom base directories too:**
```toml
[target]
agents = "my/agents"

[agents]
# Source: agents/ai/helper.md
# Installed as: my/agents/ai/helper.md (preserves ai/ subdirectory)
helper = { source = "community", path = "agents/ai/helper.md" }
```

### Per-Resource Custom Targets

Custom targets are relative to the default resource directory:

```toml
[agents]
example = { source = "community", path = "agents/example.md", target = "custom" }
# Installed as: .claude/agents/custom/example.md (target relative to agents directory)
```

**Examples with path preservation:**
```toml
[agents]
# Nested source with custom target
ai-helper = {
    source = "community",
    path = "agents/ai/helper.md",
    target = "specialized"
}
# Installed as: .claude/agents/specialized/ai/helper.md (preserves ai/ subdirectory)

# Nested target directory
reviewer = {
    source = "community",
    path = "agents/code-reviewer.md",
    target = "custom/reviews"
}
# Installed as: .claude/agents/custom/reviews/code-reviewer.md
```

## Version Control Strategy

By default, AGPM creates `.gitignore` entries to exclude installed files from Git:

- The `agpm.toml` manifest and `agpm.lock` lockfile are committed
- Installed resource files are automatically gitignored
- Team members run `agpm install` to get their own copies

To commit resources to Git instead:

```toml
[target]
gitignore = false  # Don't create .gitignore
```

## Pattern-Based Dependencies

Install multiple resources using glob patterns. Each matched file preserves its source directory structure.

```toml
[agents]
# Install all AI agents - each preserves its source path
# agents/ai/assistant.md → .claude/agents/ai/assistant.md
# agents/ai/analyzer.md → .claude/agents/ai/analyzer.md
ai-agents = { source = "community", path = "agents/ai/*.md", version = "v1.0.0" }

# Install all review tools recursively - maintains nested structure
# agents/code/review-expert.md → .claude/agents/code/review-expert.md
# agents/security/review-scanner.md → .claude/agents/security/review-scanner.md
review-tools = { source = "community", path = "agents/**/review*.md", version = "v1.0.0" }

[snippets]
# All Python snippets - directory structure preserved
# snippets/python/utils.md → .claude/agpm/snippets/python/utils.md
# snippets/python/helpers.md → .claude/agpm/snippets/python/helpers.md
python-snippets = { source = "community", path = "snippets/python/*.md", version = "v1.0.0" }

# Multiple nested directories
# snippets/web/react/hooks.md → .claude/agpm/snippets/web/react/hooks.md
# snippets/web/vue/composables.md → .claude/agpm/snippets/web/vue/composables.md
web-snippets = { source = "community", path = "snippets/web/**/*.md", version = "v1.0.0" }
```

**Benefits**:
- **Organization preserved**: Complex directory hierarchies maintained automatically
- **No conflicts**: Files with identical names in different directories don't collide
- **Clear structure**: Installation mirrors source repository organization

## Best Practices

1. **Organize by Function**: Group related resources together
2. **Use Semantic Names**: Choose descriptive names for your dependencies
3. **Version Scripts with Hooks**: Keep scripts and their hook configurations in sync
4. **Test Locally First**: Use local sources during development
5. **Document Requirements**: Note any runtime requirements for scripts/MCP servers
6. **Preserve User Config**: Never manually edit merged configuration files

## Troubleshooting

### Scripts Not Executing

- Ensure scripts have executable permissions
- Check the script path in hook configuration
- Verify required interpreters are installed (bash, python, node, etc.)

### Hooks Not Triggering

- Check `.claude/settings.local.json` for the hook entry
- Verify the event name and matcher pattern
- Check hook timeout settings

### MCP Servers Not Starting

- Ensure required runtimes are installed (Node.js for npx, Python for uvx)
- Check `.mcp.json` for the server configuration
- Verify environment variables are set correctly

### Configuration Not Merging

- Run `agpm install` again to re-merge configurations
- Check for syntax errors in JSON files
- Ensure AGPM has write permissions to config files