# Contributing to AGPM

Thank you for your interest in contributing to AGPM (AGent Package Manager)! We welcome contributions from everyone and are grateful for even the smallest fixes or features.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [How to Contribute](#how-to-contribute)
- [Pull Request Process](#pull-request-process)
- [Coding Standards](#coding-standards)
- [Testing Guidelines](#testing-guidelines)
- [Documentation](#documentation)
- [Communication](#communication)
- [Recognition](#recognition)

## Code of Conduct

We are committed to providing a welcoming and inclusive environment for all contributors. Please:

- Be respectful and considerate in all interactions
- Welcome newcomers and help them get started
- Focus on constructive criticism and helpful feedback
- Respect differing viewpoints and experiences
- Accept responsibility for mistakes and learn from them

Unacceptable behavior includes harassment, discrimination, or any form of abuse. Such behavior will result in removal from the project.

## Getting Started

### Finding Issues to Work On

Look for issues labeled with:
- `good first issue` - Perfect for newcomers
- `help wanted` - Community help needed
- `bug` - Bug fixes needed
- `enhancement` - New features or improvements
- `documentation` - Documentation improvements

Before starting work on an issue:
1. Comment on the issue to let others know you're working on it
2. Ask any clarifying questions you have
3. Wait for confirmation from a maintainer (usually within 24 hours)

### Creating New Issues

When creating an issue:
- Check if a similar issue already exists
- Use a clear, descriptive title
- Provide detailed information and steps to reproduce (for bugs)
- Include your environment details (OS, Rust version, etc.)
- Add relevant labels

## Development Setup

### Prerequisites

- Rust 1.85.0 or later
- Git 2.5 or later (required for worktree support)
- A GitHub account
- Your favorite code editor (we recommend VS Code with rust-analyzer)
- Understanding of async Rust and tokio (for concurrency-related contributions)

### Setting Up Your Development Environment

1. **Fork and clone the repository:**
   ```bash
   # Fork via GitHub UI, then:
   git clone https://github.com/YOUR_USERNAME/agpm.git
   cd agpm
   ```

2. **Add upstream remote:**
   ```bash
   git remote add upstream https://github.com/aig787/agpm.git
   ```

3. **Install development tools:**
   ```bash
   # Install rustfmt and clippy
   rustup component add rustfmt clippy
   
   # Install cargo-nextest for faster parallel test execution
   cargo install cargo-nextest --locked

   # Install cargo-llvm-cov for coverage (optional)
   cargo install cargo-llvm-cov

   # Install additional development tools
   cargo install cargo-edit  # For managing dependencies
   cargo install cargo-audit # For security auditing
   ```

4. **Build and test the project:**
   ```bash
   # Build in development mode
   cargo build

   # Run the full test suite (uses cargo nextest for parallel execution)
   cargo nextest run
   cargo test --doc

   # Test worktree functionality specifically
   cargo nextest run cache
   cargo nextest run installer
   ```

5. **Set up pre-commit hooks (optional but recommended):**
   ```bash
   # Create a pre-commit hook
   cat > .git/hooks/pre-commit << 'EOF'
   #!/bin/sh
   cargo fmt --check
   cargo clippy -- -D warnings
   cargo nextest run
   cargo test --doc
   EOF
   chmod +x .git/hooks/pre-commit
   ```

## How to Contribute

### Workflow

1. **Create a feature branch:**
   ```bash
   git checkout -b feature/your-feature-name
   # or
   git checkout -b fix/issue-number-description
   ```

2. **Make your changes:**
   - Write clean, idiomatic Rust code
   - Add tests for new functionality
   - Update documentation as needed
   - Follow the coding standards (see below)

3. **Test your changes:**
   ```bash
   # Format code
   cargo fmt
   
   # Run linter
   cargo clippy -- -D warnings

   # Run tests
   cargo nextest run
   cargo test --doc

   # Test parallel functionality specifically
   cargo nextest run --test-threads 1 cache
   cargo nextest run installer

   # Run tests with coverage (optional)
   cargo llvm-cov --html

   # Test on different parallelism levels
   AGPM_MAX_PARALLEL=1 cargo nextest run
   AGPM_MAX_PARALLEL=16 cargo nextest run
   ```

4. **Commit your changes:**
   ```bash
   git add .
   git commit -m "feat: add new feature" # or "fix: resolve issue #123"
   ```
   
   ### Commit Message Convention
   
   This project uses [Conventional Commits](https://www.conventionalcommits.org/) for automated versioning and changelog generation. Please follow this format:
   
   ```
   <type>[optional scope]: <description>
   
   [optional body]
   
   [optional footer(s)]
   ```
   
   **Commit Types:**
   - `feat:` - New feature (triggers minor version bump)
   - `fix:` - Bug fix (triggers patch version bump)
   - `perf:` - Performance improvement (triggers patch version bump)
   - `docs:` - Documentation changes (no version bump)
   - `style:` - Code style changes (no version bump)
   - `refactor:` - Code refactoring (no version bump)
   - `test:` - Test additions or changes (no version bump)
   - `build:` - Build system changes (no version bump)
   - `ci:` - CI configuration changes (no version bump)
   - `chore:` - Maintenance tasks (no version bump)
   - `revert:` - Revert a previous commit (triggers patch version bump)
   
   **Breaking Changes:**
   To indicate a breaking change (triggers major version bump), add `BREAKING CHANGE:` in the commit body/footer, or append `!` after the type:
   
   ```bash
   # Breaking change with !
   git commit -m "feat!: change manifest format to TOML tables"
   
   # Breaking change in body
   git commit -m "feat: update dependency format" -m "BREAKING CHANGE: Dependencies now use [agents] table instead of [dependencies]"
   ```

5. **Push to your fork:**
   ```bash
   git push origin feature/your-feature-name
   ```

6. **Create a Pull Request:**
   - Go to your fork on GitHub
   - Click "New Pull Request"
   - Fill out the PR template
   - Link any related issues

## Pull Request Process

### Before Submitting

Ensure your PR:
- [ ] Passes all tests (`cargo nextest run && cargo test --doc`)
- [ ] Follows code style (`cargo fmt`)
- [ ] Passes linting (`cargo clippy`)
- [ ] Includes tests for new functionality
- [ ] Updates relevant documentation
- [ ] Has a clear, descriptive title
- [ ] References any related issues

### PR Review Process

1. A maintainer will review your PR within 1-3 days
2. Address any feedback or requested changes
3. Once approved, a maintainer will merge your PR
4. Your contribution will be included in the next release!

### What to Expect

- **Feedback Timeline**: Initial review within 72 hours
- **Iteration**: Most PRs require 1-2 rounds of feedback
- **Merge**: Once approved, merged within 24 hours

## Coding Standards

### Rust Style Guide

- Follow standard Rust naming conventions
- Use `rustfmt` for consistent formatting
- Keep functions focused and small (< 50 lines preferred)
- Write descriptive variable and function names
- Avoid `unwrap()` in production code - use proper error handling
- Prefer `Result<T, E>` over `panic!`
- Document public APIs with doc comments

### Documentation Standards

- Add doc comments (`///`) to all public items
- Include examples in doc comments where helpful
- Keep comments up-to-date with code changes
- Write clear commit messages

Example:
```rust
/// Resolves dependencies from the manifest file.
///
/// # Arguments
/// * `manifest` - The parsed manifest file
///
/// # Returns
/// * `Result<Lockfile>` - The resolved lockfile or an error
///
/// # Example
/// ```
/// let manifest = Manifest::load("agpm.toml")?;
/// let lockfile = resolve_dependencies(&manifest)?;
/// ```
pub fn resolve_dependencies(manifest: &Manifest) -> Result<Lockfile> {
    // Implementation
}
```

## Testing Guidelines

### Test Requirements

- All new features must include tests
- Bug fixes should include a test that would have caught the bug
- Maintain or improve test coverage (target: 70%+)
- Test edge cases and error conditions
- Test parallel and concurrent scenarios for cache and installer changes
- Ensure tests are parallel-safe (no shared global state)
- Test worktree functionality across different Git versions
- Test cross-platform compatibility (especially path handling)

### Test Organization

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_success_case() {
        // Test normal operation
    }
    
    #[test]
    fn test_error_case() {
        // Test error handling
    }
    
    #[test]
    fn test_edge_case() {
        // Test boundary conditions
    }
}
```

### Integration Tests

Place integration tests in the `tests/` directory. Focus on testing full workflows:

```rust
// tests/integration_parallel_install.rs
use agpm::cli;
use tempfile::TempDir;
use std::sync::Arc;

#[tokio::test]
async fn test_parallel_install_workflow() {
    let temp_dir = TempDir::new().unwrap();
    // Test parallel installation with worktrees
    // Verify no race conditions or conflicts
}

#[tokio::test]
async fn test_worktree_isolation() {
    // Test that different dependencies from same repo
    // use isolated worktrees without conflicts
}

#[tokio::test]
async fn test_max_parallel_flag() {
    // Test --max-parallel flag functionality
    // Verify parallelism is correctly limited
}
```

### Platform-Specific Testing

Ensure your changes work across all supported platforms:
- **Linux** (x86_64, aarch64) - Test with different Git versions
- **macOS** (Intel, Apple Silicon) - Test with both architectures
- **Windows** (x86_64) - Test path handling and PowerShell compatibility

#### Platform-Specific Considerations

- **Path separators**: Use `std::path` consistently
- **File permissions**: Test executable file handling on Unix
- **Git worktrees**: Verify worktree paths work on all platforms
- **Parallelism**: Test resource contention behavior varies by OS
- **Case sensitivity**: macOS has case-insensitive filesystem by default

Use GitHub Actions CI to verify cross-platform compatibility automatically.

## Documentation

### Types of Documentation

1. **Code Documentation**: Doc comments in source files
2. **User Documentation**: README.md, docs/
3. **Developer Documentation**: CONTRIBUTING.md, CLAUDE.md
4. **API Documentation**: Generated via `cargo doc`

### Documentation Updates

Update documentation when you:
- Add new features
- Change existing behavior
- Fix bugs that affect usage
- Improve examples or clarity

## Communication

### Where to Get Help

- **GitHub Issues**: For bug reports and feature requests
- **GitHub Discussions**: For questions and community discussion
- **Pull Request Comments**: For code-specific discussions

### Response Times

- Issues: Response within 48 hours
- Pull Requests: Initial review within 72 hours
- Questions: Best effort, usually within 24 hours

### Tips for Effective Communication

- Be specific and provide context
- Include code examples when relevant
- Be patient and respectful
- Follow up if you haven't heard back in a week

## Recognition

We value all contributions! Contributors are recognized through:

- Inclusion in release notes
- GitHub contributor badge
- Mentions in project documentation for significant contributions
- Invitation to become a maintainer for consistent contributors

### Types of Contributions We Value

#### Code Contributions
- **Features**: New functionality, especially around parallel processing
- **Bug fixes**: Issues with concurrency, caching, or cross-platform compatibility
- **Performance**: Optimizations for parallel operations and Git worktrees
- **Security**: Vulnerability fixes and security hardening

#### Non-Code Contributions
- **Documentation**: User guides, API docs, architecture explanations
- **Testing**: Platform-specific testing, edge case discovery
- **Bug reports**: Reproducible examples, especially for parallel operation issues
- **Feature requests**: Use cases for improved parallelism or caching
- **Code reviews**: Feedback on concurrency patterns and error handling
- **Community support**: Helping others in discussions and issues

#### Specialized Areas
- **Git worktree expertise**: Improvements to worktree management
- **Async Rust**: Enhancements to parallel processing architecture
- **Cross-platform testing**: Windows, macOS, Linux compatibility
- **Performance profiling**: Identifying bottlenecks in parallel operations
- **Cache optimization**: Improvements to repository caching strategies

## Additional Resources

- [Rust Book](https://doc.rust-lang.org/book/) - Learn Rust
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) - Best practices
- [Conventional Commits](https://www.conventionalcommits.org/) - Commit message format
- [Semantic Versioning](https://semver.org/) - Version numbering

## Release Process

AGPM uses GitHub Actions for automated versioning and release management based on conventional commits.

### Creating a Release

1. **Go to Actions → Release workflow**
2. **Configure release options:**
   - **Version Bump Override** (optional): Leave empty for automatic detection, or force a specific bump:
     - `patch` - Force patch release (0.0.X)
     - `minor` - Force minor release (0.X.0)
     - `major` - Force major release (X.0.0)
   - **Pre-release Type** (optional): Choose release channel:
     - Leave empty for stable release
     - Select "beta" for beta release (e.g., 1.0.0-beta.1)
     - Select "alpha" for alpha release (e.g., 1.0.0-alpha.1)
3. **Click "Run workflow"**

### What Happens During Release

The workflow:

1. **Analyze Commits**: Determines version bump from commit messages since last release:
   - `fix:`, `perf:`, `docs:`, `chore:`, etc. → Patch release (0.0.X)
   - `feat:` → Minor release (0.X.0)
   - Breaking changes (`!` or `BREAKING CHANGE:`) → Major release (X.0.0)
2. **Create Release** (if commits warrant it):
   - Updates version in `Cargo.toml` and `Cargo.lock`
   - Creates git tag (e.g., `v1.0.0`)
   - Generates changelog from conventional commits
   - Creates GitHub release with changelog
   - Builds and attaches binaries for all platforms
   - Publishes package to crates.io
   - Updates Homebrew formula (for stable releases only)

### Semantic Release Configuration

The release process is configured in `.releaserc.json`:
- **Branches**: `main` (stable), `beta`, `alpha`
- **Commit Analysis**: Uses `conventionalcommits` preset
- **Release Rules**: All commit types trigger at least a patch release
- **Plugins**: Handles Cargo.toml updates, GitHub releases, and crates.io publishing

### Important Notes

- Releases are **manual-only** - triggered via GitHub Actions workflow
- Semantic-release may skip release if no releasable commits exist (unless version bump is forced)
- Version is automatically determined from commit messages
- Pre-releases (alpha/beta) use separate branches and increment independently
- All conventional commit types trigger at least a patch release to ensure continuous delivery

## Questions?

If you have questions about contributing, please:
1. Check existing issues and discussions
2. Create a new discussion if your question hasn't been answered
3. Be patient - we're all volunteers!

Thank you for contributing to AGPM! Your efforts help make package management better for the entire Claude Code community.

---

*Last updated: September 2024 - Reflects worktree-based parallel architecture*