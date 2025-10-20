# Content Embedding Example Project

This example demonstrates AGPM's content embedding features (v0.4.8+), which allow you to include content from other files in your agents, commands, and other resources.

## What is Content Embedding?

Content embedding allows you to:

1. **Embed versioned content from dependencies** - Include content from Git repositories at specific versions
2. **Embed project-local files** - Include content from your project directory using the `content` filter

## Features Demonstrated

This example shows:

1. **Dependency Content Embedding**: The code reviewer agent embeds three snippet files:
   - `rust-patterns.md` - Common Rust patterns and idioms
   - `code-quality-checklist.md` - Comprehensive review checklist
   - `api-design-principles.md` - API design best practices

2. **Content Filter**: The agent can also reference project-local documentation in `docs/coding-standards.md`

3. **Install-Only vs. Content-Only**: Dependencies can be configured to either:
   - Create files AND make content available (default: `install: true`)
   - Only make content available without creating files (`install: false`)

## Setup

1. Install the dependencies:
   ```bash
   agpm install
   ```

2. This will:
   - Install the `code-reviewer-with-standards` agent to `.claude/agents/`
   - The agent will have all the snippet content embedded in it
   - The snippet files themselves won't be installed (because they're used as dependencies)

## How It Works

### 1. Dependency Content Embedding

In the agent's frontmatter, we declare dependencies:

```yaml
---
agpm.templating: true
dependencies:
  snippets:
    - path: snippets/rust-patterns.md
      name: rust_patterns
    - path: snippets/code-quality-checklist.md
      name: quality_checklist
    - path: snippets/api-design-principles.md
      name: api_principles
---
```

Then in the agent body, we reference the content:

```markdown
## Rust-Specific Patterns

{{ agpm.deps.snippets.rust_patterns.content }}
```

### 2. Content Filter (Project-Local)

The agent can also reference project-local files:

```markdown
## Project-Specific Standards

{{ 'docs/coding-standards.md' | content }}
```

This reads the file from your project directory at install time.

## Inspecting the Result

After running `agpm install`, check the installed agent:

```bash
cat .claude/agents/code-reviewer-with-standards.md
```

You'll see that all the snippet content has been embedded directly into the agent file. The agent is now self-contained with all its required knowledge.

## Benefits

### 1. Version Control
- Dependencies are pinned to specific versions
- You get consistent content across installations
- Updates are controlled and explicit

### 2. Reusability
- Share common knowledge across multiple agents
- Avoid duplication
- Maintain consistency

### 3. Modularity
- Break down large agents into smaller, focused snippets
- Easier to maintain and update
- Better organization

### 4. Project-Specific Customization
- Embed project-local documentation
- Adapt generic agents to specific projects
- No need to fork or modify upstream agents

## Customization

### Add Your Own Standards

Edit `docs/coding-standards.md` to add your project-specific guidelines. The agent will pick them up on the next `agpm install`.

### Use Different Snippets

Modify `agpm.toml` to use different snippets or add more dependencies:

```toml
[agents]
code-reviewer = {
    source = "examples",
    path = "agents/code-reviewer-with-standards.md",
    dependencies.snippets = [
        { path = "snippets/security-best-practices.md", name = "security" }
    ]
}
```

### Create Your Own Content-Embedding Agent

1. Set `agpm.templating: true` in the frontmatter
2. Add dependencies with the content you want to embed
3. Use `{{ agpm.deps.<type>.<name>.content }}` to reference the content
4. Use `{{ 'path/to/file.md' | content }}` for project-local files

## Advanced: Content-Only Dependencies

You can also use dependencies without installing them as files:

```yaml
---
dependencies:
  snippets:
    - path: snippets/helper-functions.md
      name: helpers
      install: false  # Don't create file, only make content available
---

## Helper Functions

{{ agpm.deps.snippets.helpers.content }}
```

This is useful when you want to include library code or documentation in your agent without cluttering your project with the individual files.

## Next Steps

- Explore the installed agent in `.claude/agents/code-reviewer-with-standards.md`
- Modify `docs/coding-standards.md` and run `agpm install` again to see the changes
- Create your own agents with content embedding
- Check out the other examples in `examples/deps/`

## See Also

- [AGPM Documentation](https://github.com/yourusername/agpm)
- [Templating Guide](../../../docs/templating.md)
- [Dependency System](../../../docs/dependencies.md)
