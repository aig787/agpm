# Documentation Improvement Instructions for CCPM

## Overview
You are tasked with adding comprehensive documentation to the CCPM (Claude Code Package Manager) project. Every public API should have proper docstrings, all modules should have high-level documentation, and the codebase should be fully documented following Rust's documentation standards.

**IMPORTANT**: This is an iterative process. You will:
1. Check current documentation coverage
2. Identify undocumented or poorly documented areas
3. Document one module/component thoroughly
4. Verify documentation builds and tests pass
5. Re-check documentation quality
6. Repeat until all modules are comprehensively documented

Work incrementally - focus on one module at a time, ensuring quality and accuracy at each step.

## Pre-Flight Checklist

### Step 1: Ensure Clean Starting State
**CRITICAL**: All code must compile and tests must pass before adding documentation:

```bash
# Format and lint check
cargo fmt --check
cargo clippy -- -D warnings

# Run all tests to ensure they pass
cargo test --all

# Build documentation to check current state
cargo doc --no-deps --open
```

### Step 2: Generate Documentation Coverage Report
```bash
# Build docs with private items to see everything
cargo doc --no-deps --document-private-items

# Check for missing documentation warnings
cargo rustdoc -- -D missing-docs

# Test documentation examples
cargo test --doc
```

## Documentation Improvement Strategy

### Step 3: Identify Documentation Gaps

Use the rust-doc-expert agent to analyze and identify:
1. **Undocumented modules** - Modules lacking `//!` documentation
2. **Public APIs without docs** - Functions, structs, enums missing `///` comments
3. **Missing examples** - APIs without usage examples
4. **Incomplete documentation** - Partial or unclear descriptions
5. **Missing error documentation** - Error types and conditions not explained
6. **Unsafe code** - Unsafe blocks without safety documentation

Priority order for CCPM modules:
1. **Public CLI interface** (`src/cli/`) - User-facing commands
2. **Core types** (`src/core/`) - Fundamental structures and errors
3. **Manifest/Lockfile** (`src/manifest/`, `src/lockfile/`) - File formats
4. **Dependency resolver** (`src/resolver/`) - Resolution algorithm
5. **Git operations** (`src/git/`) - Version control integration
6. **Source management** (`src/source/`) - Repository handling
7. **Configuration** (`src/config/`) - Settings and options
8. **Utilities** (`src/utils/`) - Helper functions

### Step 4: Using the Documentation Expert Agent

#### Invoke the rust-doc-expert agent for each module:

```
/agent rust-doc-expert

Task: Add comprehensive documentation to the [module_name] module in CCPM.

Requirements:
1. Add module-level documentation with `//!` comments explaining the module's purpose
2. Document all public functions, structs, enums, and traits with `///` comments
3. Include at least one runnable example for each public function
4. Document error conditions and when they occur
5. Add safety documentation for any unsafe code
6. Ensure all documentation tests compile and pass
7. Follow Rust API documentation guidelines
8. Include cross-references to related modules/types using `[Type]` links

Module to document: src/[module_name]/mod.rs

Please ensure documentation is accurate, comprehensive, and helpful for both users and contributors.
```

### Step 5: Documentation Standards for CCPM

#### Module Documentation Template
```rust
//! # Module Name
//!
//! Brief one-line description of the module's purpose.
//!
//! ## Overview
//!
//! Detailed explanation of what this module provides, its role in CCPM,
//! and how it interacts with other modules.
//!
//! ## Examples
//!
//! ```rust
//! use ccpm::module_name;
//!
//! // Example showing primary use case
//! let result = module_name::main_function()?;
//! ```
//!
//! ## Implementation Details
//!
//! Technical details about algorithms, data structures, or design decisions
//! that are important for contributors to understand.
```

#### Function Documentation Template
```rust
/// Brief one-line summary of what the function does.
///
/// More detailed explanation providing context about when and why
/// to use this function. Explain any important behavior or constraints.
///
/// # Arguments
///
/// * `param1` - Description of the first parameter and valid values
/// * `param2` - Description of the second parameter and constraints
///
/// # Returns
///
/// Description of what the function returns and what it represents.
///
/// # Errors
///
/// Returns [`ErrorType`] when:
/// - Specific condition that causes this error
/// - Another condition that causes an error
///
/// # Examples
///
/// ```rust
/// use ccpm::module::function;
///
/// let result = function("input", 42)?;
/// assert_eq!(result, "expected output");
/// ```
///
/// # Panics
///
/// Panics if [condition that causes panic].
pub fn function(param1: &str, param2: usize) -> Result<String, Error> {
    // Implementation
}
```

### Step 6: CCPM-Specific Documentation Requirements

#### Document Key Concepts

**Manifest Format (ccpm.toml)**
```rust
/// Represents the project manifest file (ccpm.toml).
///
/// The manifest defines project dependencies, sources, and metadata.
/// It follows a similar model to Cargo.toml but for Claude Code resources.
///
/// # Format
///
/// ```toml
/// [sources]
/// community = "https://github.com/org/repo.git"
///
/// [agents]
/// agent-name = { source = "community", path = "agents/file.md", version = "v1.0.0" }
///
/// [snippets]
/// snippet-name = { source = "community", path = "snippets/file.md" }
/// ```
///
/// # Examples
///
/// ```rust
/// use ccpm::manifest::Manifest;
///
/// let manifest = Manifest::from_file("ccpm.toml")?;
/// for (name, agent) in &manifest.agents {
///     println!("Agent: {}", name);
/// }
/// ```
pub struct Manifest {
    // ...
}
```

**Lockfile Format (ccpm.lock)**
```rust
/// Represents the lockfile that ensures reproducible installations.
///
/// The lockfile records exact versions and commits for all dependencies,
/// similar to Cargo.lock. It should be committed to version control
/// to ensure all team members get identical dependencies.
///
/// # Generation
///
/// The lockfile is automatically generated/updated when running:
/// - `ccpm install` - Creates or updates based on manifest
/// - `ccpm update` - Updates within version constraints
///
/// # Format
///
/// The lockfile uses TOML format with resolved dependency information.
```

#### Document CLI Commands
```rust
/// Installs dependencies defined in the project manifest.
///
/// This command reads `ccpm.toml`, resolves all dependencies,
/// and installs them to the project directory. If a `ccpm.lock`
/// exists, it uses the locked versions for reproducible builds.
///
/// # Behavior
///
/// 1. Reads and validates `ccpm.toml`
/// 2. Resolves dependency versions
/// 3. Clones/updates sources in cache
/// 4. Copies resources to project
/// 5. Generates/updates `ccpm.lock`
///
/// # Options
///
/// * `--no-cache` - Skip cache and clone fresh
/// * `--offline` - Use only cached sources
/// * `--verbose` - Show detailed progress
///
/// # Examples
///
/// ```bash
/// # Install all dependencies
/// ccpm install
///
/// # Install without using cache
/// ccpm install --no-cache
/// ```
pub async fn execute(args: InstallArgs) -> Result<()> {
    // ...
}
```

### Step 7: Iterative Documentation Workflow

**Follow this cycle for each module:**

1. **Assess Current Documentation**
   ```bash
   # Check for missing docs in specific module
   cargo rustdoc -- -D missing-docs 2>&1 | grep "src/module_name"
   
   # View current documentation
   cargo doc --no-deps --open
   ```

2. **Pick ONE Module to Document**
   - Start with most important public APIs
   - Or pick module with zero documentation
   - Complete one module before moving to next

3. **Use rust-doc-expert Agent**
   - Invoke agent with specific module
   - Review generated documentation
   - Ensure accuracy and completeness

4. **Verify Documentation Quality**
   ```bash
   # Build documentation
   cargo doc --no-deps
   
   # Test documentation examples
   cargo test --doc
   
   # Check for broken links
   cargo doc --no-deps 2>&1 | grep "warning"
   ```

5. **Test Documentation Examples**
   ```bash
   # Run doc tests for specific module
   cargo test --doc module_name
   
   # Ensure all examples compile
   cargo test --doc --no-run
   ```

6. **Review and Refine**
   - Read generated HTML docs
   - Check examples are helpful
   - Ensure cross-references work
   - Verify error documentation is complete

7. **Commit Progress**
   ```bash
   # Commit documentation for this module
   git add -A
   git commit -m "Add comprehensive documentation for [module_name]"
   ```

8. **Repeat for Next Module**
   - Return to step 1
   - Pick next module
   - Continue until all modules documented

### Step 8: Documentation Testing Patterns

#### Testing Documentation Examples
```rust
/// Parses a manifest from a string.
///
/// # Examples
///
/// ```rust
/// # use ccpm::manifest::Manifest;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let toml = r#"
///     [sources]
///     local = "https://github.com/user/repo.git"
/// "#;
/// 
/// let manifest = Manifest::from_str(toml)?;
/// assert_eq!(manifest.sources.len(), 1);
/// # Ok(())
/// # }
/// ```
```

#### Documenting Error Conditions
```rust
/// Resolves dependencies for the project.
///
/// # Errors
///
/// Returns [`ResolverError::Conflict`] when:
/// - Two dependencies require incompatible versions of the same package
/// - Example: package A requires foo v1.0, package B requires foo v2.0
///
/// Returns [`ResolverError::NotFound`] when:
/// - A dependency references a non-existent source
/// - A specified version tag doesn't exist in the repository
///
/// Returns [`ResolverError::Cycle`] when:
/// - Circular dependencies are detected
/// - Example: A depends on B, B depends on A
```

#### Documenting Async Functions
```rust
/// Fetches the latest changes from a remote repository.
///
/// This is an async function that requires a tokio runtime.
///
/// # Examples
///
/// ```rust
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use ccpm::git;
///
/// let updates = git::fetch_updates("https://github.com/user/repo.git").await?;
/// println!("Fetched {} new commits", updates.len());
/// # Ok(())
/// # }
/// # tokio::runtime::Runtime::new().unwrap().block_on(example());
/// ```
```

### Step 9: Special Documentation Areas for CCPM

#### Security Considerations
```rust
/// Validates source URLs for security.
///
/// # Security
///
/// This function prevents:
/// - Command injection through malformed URLs
/// - Path traversal attacks via `../` sequences
/// - Unauthorized access to local file system
///
/// Only HTTPS URLs and SSH URLs to known hosts are accepted.
/// Authentication tokens must be in ~/.ccpm/config.toml, never in URLs.
```

#### Cross-Platform Behavior
```rust
/// Creates the cache directory for storing cloned repositories.
///
/// # Platform Behavior
///
/// * **Windows**: Uses `%LOCALAPPDATA%\ccpm\cache` or `%USERPROFILE%\.ccpm\cache`
/// * **macOS**: Uses `~/Library/Caches/ccpm` or `~/.ccpm/cache`
/// * **Linux**: Uses `$XDG_CACHE_HOME/ccpm` or `~/.ccpm/cache`
///
/// The directory is created with appropriate permissions for the platform.
```

#### Configuration Documentation
```rust
/// Global configuration file (~/.ccpm/config.toml).
///
/// # Location
///
/// The configuration file is stored in the user's home directory:
/// - Windows: `%USERPROFILE%\.ccpm\config.toml`
/// - Unix: `~/.ccpm/config.toml`
///
/// # Format
///
/// ```toml
/// # Private sources with authentication
/// [sources]
/// private = "https://oauth2:token@github.com/org/private-repo.git"
///
/// # Cache settings
/// [cache]
/// directory = "~/.ccpm/cache"
/// ttl_days = 7
/// ```
///
/// # Security
///
/// This file contains authentication tokens and should have restricted
/// permissions (0600 on Unix). Never commit this file to version control.
```

## Common Documentation Improvements

1. **Add Missing Examples**: Every public function needs at least one example
2. **Document "Why" Not Just "What"**: Explain design decisions and use cases
3. **Cross-Reference Related Items**: Use `[Type]` links to connect related docs
4. **Document Error Recovery**: Show how to handle each error type
5. **Include Performance Notes**: Document O(n) complexity where relevant
6. **Add Migration Guides**: Help users upgrade between versions
7. **Document Invariants**: State assumptions and preconditions clearly

## Quick Commands Reference

```bash
# Build and view documentation
cargo doc --no-deps --open

# Build docs including private items
cargo doc --no-deps --document-private-items

# Check for missing documentation
cargo rustdoc -- -D missing-docs

# Test documentation examples
cargo test --doc

# Test specific module's doc examples
cargo test --doc module_name

# Check documentation coverage
cargo doc-coverage  # If cargo-doc-coverage is installed

# Serve documentation locally
python3 -m http.server --directory target/doc 8000
```

## When to Use the rust-doc-expert Agent

- **For each module**: Use agent to add comprehensive documentation
- **For complex APIs**: Let agent create detailed examples
- **For error types**: Have agent document all error conditions
- **For public interfaces**: Ensure all user-facing APIs are documented
- **For unsafe code**: Agent adds safety requirements and invariants

## Success Criteria

✅ All public items have documentation
✅ Every module has `//!` level documentation
✅ All functions include at least one example
✅ Documentation examples compile and pass
✅ Error conditions are clearly documented
✅ Cross-references link correctly
✅ No missing_docs warnings
✅ Documentation is helpful for both users and contributors
✅ Security considerations are documented
✅ Platform-specific behavior is explained
✅ Each module documented and committed separately

## Red Flags to Avoid

1. **Outdated Documentation**: Docs that don't match implementation
2. **Broken Examples**: Code examples that don't compile
3. **Missing Error Cases**: Not documenting when errors occur
4. **Unclear Descriptions**: Vague or unhelpful documentation
5. **No Examples**: APIs without usage demonstrations
6. **Broken Links**: Cross-references that don't resolve
7. **Copy-Paste Docs**: Generic documentation not specific to the function

## Module-Specific Documentation Goals

Based on user impact and API complexity:
- `src/cli/`: Complete command documentation with examples
- `src/manifest/`: Full format specification and validation rules
- `src/lockfile/`: Lockfile format and generation algorithm
- `src/resolver/`: Dependency resolution algorithm explanation
- `src/core/`: All error types and resource traits documented
- `src/git/`: Git operations and authentication handling
- `src/source/`: Source types and caching strategy
- `src/config/`: Configuration format and security notes
- `src/utils/`: Helper function documentation with examples

Remember: Good documentation is an investment in the project's future. It helps users adopt CCPM and makes it easier for contributors to improve the codebase.