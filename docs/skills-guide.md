# Claude Skills Guide

Claude Skills are directory-based resources that enable packaging of expertise, procedures, and reusable components that Claude can automatically invoke based on context. Unlike other resource types that are single files, Skills are complete directories containing a `SKILL.md` file and optional supporting files.

## Overview

### What are Claude Skills?

- **Directory-based resources**: Skills contain multiple files organized in a directory structure
- **Required SKILL.md**: Each skill must have a `SKILL.md` file with YAML frontmatter
- **Model-invoked**: Claude decides when to use them based on the description and context
- **Multi-file support**: Can include documentation, code examples, reference materials, and scripts
- **Install to `.claude/skills/`**: Skills are installed in a dedicated skills directory

### Skill Directory Structure

```
.claude/skills/
├── my-skill/
│   ├── SKILL.md       # Required: Main skill definition
│   ├── REFERENCE.md   # Optional: Additional documentation
│   ├── examples/      # Optional: Code examples
│   │   ├── basic.py
│   │   └── advanced.js
│   └── scripts/       # Optional: Helper scripts
│       └── helper.sh
└── another-skill/
    └── SKILL.md
```

## SKILL.md Format

Every skill must have a `SKILL.md` file with YAML frontmatter:

```yaml
---
name: Rust Development Helper
description: Comprehensive assistance for Rust development including debugging, optimization, and best practices
version: 1.0.0
allowed-tools: Read, Grep, Write, Bash
dependencies:
  agents:
    - path: agents/rust-expert.md
      version: v1.0.0
  snippets:
    - path: snippets/rust-patterns.md
---

# Rust Development Helper

This skill provides comprehensive assistance for Rust development projects.

## Capabilities

- Debugging Rust code and identifying common issues
- Performance optimization suggestions
- Code review and best practices
- Refactoring recommendations
- Testing strategies and implementation

## Usage

Claude will automatically invoke this skill when working on Rust projects or when you explicitly ask for Rust-related help.

## Examples

### Debugging
```rust
// Problematic code
fn main() {
    let mut vec = Vec::new();
    vec.push(42);
    println!("{}", vec[0]); // This might cause issues
}
```

### Optimization
```rust
// Optimized version
fn main() {
    let vec = vec![42]; // More efficient
    println!("{}", vec[0]);
}
```
```

### Required Frontmatter Fields

- `name` (string): Human-readable name of the skill
- `description` (string): What this skill does and when it should be used

### Optional Frontmatter Fields

- `version` (string): Version of the skill (semver recommended)
- `allowed-tools` (array): List of tools this skill can use (Read, Grep, Write, Bash, etc.)
- `dependencies` (object): Dependencies on other resources

## Managing Skills

### Adding Skills

Skills are added by editing your `agpm.toml` manifest file directly:

```toml
[sources]
community = "https://github.com/aig787/agpm-community.git"

[skills]
# Add a single skill from a source repository
rust-helper = { source = "community", path = "skills/rust-helper", version = "v1.0.0" }

# Add a skill with a custom target directory
code-reviewer = { source = "community", path = "skills/ai-reviewer", version = "v2.0.0" }

# Add a local skill directory
my-skill = { path = "./my-local-skill" }

# Add multiple skills using a pattern
all-community-skills = { source = "community", path = "skills/*", version = "v1.0.0" }
```

After editing, run `agpm install` to install the skills.

### Listing Skills

```bash
# List all installed resources
agpm list

# List only skills
agpm list --type skill

# JSON output for automation
agpm list --format json
```

### Removing Skills

To remove a skill, delete its entry from `agpm.toml` and run:

```bash
# Re-install to clean up removed dependencies
agpm install
```

### Updating Skills

```bash
# Update all dependencies including skills
agpm update

# Update specific skills by name
agpm update rust-helper ai-reviewer
```

## Manifest Configuration

### Single Skill Dependencies

```toml
[sources]
community = "https://github.com/aig787/agpm-community.git"
local = "../my-skills"

[skills]
# Single skill with version
rust-helper = { source = "community", path = "skills/rust-helper", version = "v1.0.0" }

# Skill with custom target
custom-skill = { 
    source = "local", 
    path = "data-processor", 
    target = ".claude/skills/data-processor"
}

# Skill for different tool (future support)
opencode-skill = { 
    source = "community", 
    path = "skills/helper", 
    tool = "opencode" 
}
```

### Pattern Dependencies

```toml
[skills]
# All skills in a directory
all-skills = { source = "community", path = "skills/*", version = "v1.0.0" }

# Recursive pattern
all-ai-skills = { source = "community", path = "skills/ai/**/*.md", version = "^2.0.0" }
```

## Advanced Features

### Transitive Dependencies

Skills can declare dependencies on other resources:

```yaml
---
name: Advanced Rust Helper
description: Comprehensive Rust development with external tools
dependencies:
  agents:
    - path: agents/rust-expert.md
      version: v1.0.0
  snippets:
    - path: snippets/rust-patterns.md
  skills:
    - path: skills/debugging-helper
      version: v1.5.0
---
```

### Skill Patching

Override skill properties without forking:

```toml
# agpm.toml
[patch.skills.rust-helper]
allowed-tools = ["Read", "Grep", "Write", "Bash", "WebSearch"]
version = "1.1.0"

# agpm.private.toml (not in git)
[patch.skills.rust-helper]
allowed-tools = ["Read", "Grep", "Write", "Bash", "WebSearch", "Database"]
```

### Template Support

Skills support opt-in Tera templating:

```yaml
---
agpm.templating: true
name: {{ agpm.deps.agents.rust-expert.name }} Helper
description: Custom helper for {{ project_name }}
---
```

## Best Practices

### Skill Design

1. **Clear Description**: Write descriptive explanations of when and how the skill should be used
2. **Focused Scope**: Keep skills focused on specific domains or tasks
3. **Good Examples**: Include practical examples in the skill content
4. **Version Management**: Use semantic versioning for skill updates
5. **Documentation**: Include additional files like REFERENCE.md for complex skills

### Directory Organization

```
my-skill/
├── SKILL.md              # Required main file
├── README.md             # Optional overview
├── REFERENCE.md          # Optional detailed reference
├── examples/             # Optional code examples
│   ├── basic/
│   └── advanced/
├── templates/            # Optional template files
└── scripts/              # Optional helper scripts
```

### Dependency Management

1. **Minimal Dependencies**: Only declare necessary dependencies
2. **Version Constraints**: Use appropriate version constraints (^, ~, exact)
3. **Circular Dependencies**: Avoid circular dependencies between skills
4. **Tool Compatibility**: Ensure dependencies are compatible with your target tool

## Lockfile Format

Skills in the lockfile track the directory checksum:

```toml
[[skills]]
name = "rust-helper"
source = "community"
path = "skills/rust-helper"
version = "v1.0.0"
resolved_commit = "abc123def456..."
checksum = "sha256:combined_checksum_of_all_files"
installed_at = ".claude/skills/rust-helper"
```

The `checksum` field contains a SHA-256 hash computed from all files in the skill directory, sorted by path. This allows AGPM to detect when any file in the skill has changed without storing the entire file list in the lockfile.

## Troubleshooting

### Common Issues

1. **Missing SKILL.md**: Ensure the skill directory contains a valid SKILL.md file
2. **Invalid Frontmatter**: Check YAML syntax and required fields
3. **Path Issues**: Use relative paths for local skills
4. **Version Conflicts**: Resolve version constraints in dependencies
5. **Tool Compatibility**: Ensure skills are compatible with your target tool

### Validation

```bash
# Validate skill structure
agpm validate --paths

# Validate with detailed output
agpm validate --verbose

# Check specific skill
agpm validate --manifest-path ./agpm.toml
```

### Debugging

```bash
# Verbose installation
agpm install --verbose

# Dry run to check what would be installed
agpm install --dry-run

# Check cache status
agpm cache info
```

## Examples

### Simple Skill

```yaml
---
name: Todo List Helper
description: Helps with managing and organizing todo lists
---
# Todo List Helper

This skill helps you create, organize, and manage todo lists effectively.

## Features

- Create structured todo lists
- Prioritize tasks
- Track progress
- Generate reports

## Usage

Simply ask me to help you organize your tasks, and I'll use this skill to provide structured assistance.
```

### Complex Skill with Dependencies

```yaml
---
name: Full-Stack Web Developer
description: Complete assistance for full-stack web development projects
version: 2.1.0
allowed-tools: Read, Grep, Write, Bash, WebSearch
dependencies:
  agents:
    - path: agents/frontend-expert.md
      version: v1.5.0
    - path: agents/backend-expert.md
      version: v1.3.0
  snippets:
    - path: snippets/react-patterns.md
    - path: snippets/node-utilities.md
  skills:
    - path: skills/database-helper
      version: v1.0.0
---
# Full-Stack Web Developer

This skill provides comprehensive assistance for full-stack web development.

## Frontend

- React/Next.js development
- State management
- UI/UX best practices
- Performance optimization

## Backend

- Node.js/Express APIs
- Database design
- Authentication & authorization
- Microservices architecture

## DevOps

- Docker containerization
- CI/CD pipelines
- Cloud deployment
- Monitoring and logging

## Examples

See the `examples/` directory for complete project templates.
```

## Migration Guide

If you're migrating from manual skill management:

1. **Convert to Directory Structure**: Organize your skills into directories with SKILL.md
2. **Add Frontmatter**: Add proper YAML frontmatter to each SKILL.md
3. **Update Dependencies**: Declare dependencies in frontmatter
4. **Add to Manifest**: Add skills to your agpm.toml
5. **Test Installation**: Use `agpm install --dry-run` to verify

```bash
# Migration steps
mkdir -p .claude/skills/my-skill
cp my-skill.md .claude/skills/my-skill/SKILL.md
# Add frontmatter to SKILL.md
# Then add to agpm.toml:
#   [skills]
#   my-skill = { path = "./my-skill" }
agpm install
```

## Security Considerations

When using skills from external sources, be aware of the following security considerations:

### Trust Model

1. **Source Verification**: Only install skills from trusted sources. Skills can contain scripts and executable content that may run on your system.
2. **Review Before Use**: Review the contents of a skill's SKILL.md and any scripts before installing, especially from unfamiliar sources.
3. **Version Pinning**: Use specific versions (`v1.0.0`) instead of floating references to avoid unexpected changes.
4. **Lockfile Integrity**: The `agpm.lock` file contains checksums that verify skill content hasn't changed unexpectedly.

### Skill Capabilities

- **allowed-tools**: Skills declare which tools they can use (Read, Write, Bash, etc.). Be cautious of skills requesting Bash access.
- **Scripts**: Skills may include executable scripts. Review these before installation.
- **Dependencies**: Skills can declare dependencies on other resources, which may also contain scripts.

### Best Practices

1. **Private Sources**: For sensitive environments, host skills in private Git repositories.
2. **Code Review**: Treat skill installations like dependency updates—review changes before accepting.
3. **Patching**: Use `agpm.private.toml` for local customizations that shouldn't be shared.
4. **Regular Updates**: Keep skills updated to receive security fixes.

### Validation

AGPM enforces several limits to prevent abuse:

- Maximum 1000 files per skill
- Maximum 100MB total skill size
- Maximum 64KB frontmatter size
- ASCII-only characters in skill names
- Symlinks are rejected (security risk)

## Resources

- [Skills Examples](../examples/deps/skills/) - Sample skill configurations
- [Community Repository](https://github.com/aig787/agpm-community) - Shared skills
- [Issue Tracker](https://github.com/aig787/agpm/issues) - Report bugs or request features