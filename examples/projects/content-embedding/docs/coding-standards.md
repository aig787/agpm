# Project-Specific Coding Standards

This file demonstrates the content filter feature. It's a project-local file that can be embedded into agents using `{{ 'docs/coding-standards.md' | content }}`.

## Project Overview

This project uses AGPM (Claude Code Package Manager) to manage AI coding assistant resources. When reviewing code, ensure adherence to these project-specific standards.

## Rust Version

- **Minimum**: Rust 2024 edition
- **Toolchain**: Use stable unless specifically needed features require nightly

## Project Structure

```
src/
├── cli/         # Command-line interface implementations
├── core/        # Core types and error handling
├── git/         # Git operations
├── resolver/    # Dependency resolution
└── installer/   # Resource installation
```

## Naming Conventions

### Modules
- Use snake_case for module names
- Organize by feature, not by type
- Keep modules focused and single-purpose

### Functions
- Prefer descriptive names over abbreviations
- Use verb phrases: `resolve_dependencies`, not `dependencies`
- Async functions should be clearly marked in documentation

### Types
- Use PascalCase for type names
- Error types end with `Error`: `ConfigError`, `InstallError`
- Result types should be meaningful: `InstallResult`, not `Result<()>`

## Error Handling

### Custom Error Types
All modules should define their own error types using `thiserror`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ModuleError {
    #[error("Operation failed: {0}")]
    OperationError(String),

    #[error("Invalid configuration: {source}")]
    ConfigError {
        #[from]
        source: ConfigError,
    },
}
```

### Context
Always add context to errors using `.context()` from anyhow:

```rust
fs::read_to_string(&path)
    .context(format!("Failed to read config file at {}", path.display()))?
```

## Testing Standards

### Test Organization
- Unit tests in same file as implementation
- Integration tests in `tests/` directory
- Use `TestProject` and `TestGit` helpers from `tests/common/`

### Test Naming
```rust
#[tokio::test]
async fn test_install_with_valid_manifest() {
    // Arrange
    let project = TestProject::new();

    // Act
    let result = install(&project.path()).await;

    // Assert
    assert!(result.is_ok());
}
```

### Parallel Safety
- ALL tests must be parallel-safe
- No `std::env::set_var()` - causes race conditions
- Each test gets its own temp directory
- Use `tokio::fs` in async tests, not `std::fs`

## Performance Guidelines

### Async I/O
Always use `tokio::fs` for file operations in async contexts:

```rust
// Good
let content = tokio::fs::read_to_string(&path).await?;

// Bad - blocks the executor
let content = std::fs::read_to_string(&path)?;
```

### Parallelism
- Leverage parallel iterators from rayon when appropriate
- Use batch operations instead of loops for Git operations
- Default parallelism: `max(10, 2 × CPU cores)`

## Documentation Requirements

### Public API
All public items must have documentation:

```rust
/// Installs resources from the manifest.
///
/// # Arguments
/// * `project_path` - Root directory of the project
///
/// # Errors
/// Returns `InstallError` if:
/// - The manifest file is missing or invalid
/// - Git operations fail
/// - File system operations fail
///
/// # Example
/// ```no_run
/// use agpm::install;
///
/// let result = install(Path::new(".")).await?;
/// ```
pub async fn install(project_path: &Path) -> Result<(), InstallError>
```

### Doctests
- Use `no_run` for examples that require external resources
- Use `ignore` for examples that won't compile standalone
- Prefer executable examples when possible

## Git Commit Messages

Follow conventional commits:

```
feat: add content filter for template embedding
fix: resolve path traversal vulnerability in content filter
docs: update README with content embedding examples
test: add integration tests for content filter
refactor: simplify dependency resolution logic
```

## CI/CD Requirements

All PRs must:
- Pass `cargo fmt -- --check`
- Pass `cargo clippy -- -D warnings`
- Pass `cargo nextest run` (all tests)
- Pass `cargo test --doc` (doctests)
- Maintain >70% code coverage

## Dependencies

### Adding New Dependencies
1. Justify the need - avoid unnecessary dependencies
2. Check license compatibility
3. Prefer well-maintained crates
4. Update CLAUDE.md with new dependency

### Dependency Versions
- Use specific versions in Cargo.toml
- Test with `cargo update` before releases
- Document any version constraints

## Security

### Path Handling
- Always validate and normalize paths
- Check for directory traversal attempts
- Use `PathBuf` instead of `String` for paths

### Credentials
- Never hardcode credentials
- Store in `~/.agpm/config.toml` only
- Never log credentials or tokens

## Code Review Checklist

Before requesting review:
- [ ] All tests pass locally
- [ ] Added tests for new functionality
- [ ] Updated documentation
- [ ] Ran `cargo fmt`
- [ ] Ran `cargo clippy` and addressed warnings
- [ ] Updated CLAUDE.md if needed
- [ ] Commit messages follow convention

---

*This is a living document. Update as project conventions evolve.*
