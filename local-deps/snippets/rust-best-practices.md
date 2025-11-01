# Rust Best Practices

## Core Principles

1. **Idiomatic Rust**: Write code that follows Rust conventions and patterns
2. **Zero Warnings Policy**: All code must pass `cargo clippy -- -D warnings`
3. **Consistent Formatting**: All code must be formatted with `cargo fmt`
4. **Memory Safety**: Leverage Rust's ownership system effectively
5. **Error Handling**: Use Result<T, E> and proper error propagation
6. **Performance**: Write efficient code without premature optimization
7. **Documentation**: Add doc comments for public APIs

## Mandatory Completion Checklist

Before considering any Rust code complete, you MUST:

1. ✅ Run `cargo fmt` to ensure proper formatting
2. ✅ Run `cargo clippy -- -D warnings` to catch all lints
3. ✅ Run `cargo nextest run` to verify tests pass
4. ✅ Run `cargo test --doc` to verify doctests pass
5. ✅ Run `cargo doc --no-deps` to verify documentation builds

## Code Style & Formatting

- Use `cargo fmt` for consistent formatting
- Follow the official Rust style guide
- Keep line length reasonable (max 100 characters)
- Use meaningful variable and function names
- Group related imports together

## Import Guidelines

- **Prefer imports over full paths**: Import types at the top of the file rather than using full paths
  - ✅ Good: `use crate::core::Struct;` then use `Struct` in code
  - ❌ Avoid: Using `crate::core::Struct` throughout the code
- **Group imports logically**: std → external crates → crate modules
- **Use `self` and `super` sparingly**: Prefer absolute paths for clarity
- **Avoid glob imports**: Use explicit imports instead of `use module::*`
- **One import per line for clarity**: Easier to read and resolve conflicts
- **Exception**: Use full paths for items with common names that might conflict

## Naming Conventions

- Use `snake_case` for variables, functions, and modules
- Use `CamelCase` for types, traits, and enum variants
- Use `SCREAMING_SNAKE_CASE` for constants and statics
- Prefix boolean functions with `is_`, `has_`, or `should_`
- Use descriptive names that convey intent

## Module Organization

- Keep modules focused and cohesive
- Use `mod.rs` for module roots
- Separate concerns clearly
- Export public APIs thoughtfully

## Code Simplicity & Maintainability

- **Prefer simple solutions**: Choose clarity over cleverness
  - If you have a choice between a simple approach and a complex one, pick the simple one
  - Advanced features should solve real problems, not be used for their own sake
  - Example: Use a simple `match` instead of complex combinators if it's more readable

- **Avoid premature abstraction**: Don't create abstractions until you have 2-3 concrete uses
  - YAGNI (You Aren't Gonna Need It) principle
  - Interfaces should emerge from concrete implementations, not precede them
  - Example: Don't create a trait for a single implementation

- **Write readable code**: Code is read more often than it's written
  - Favor explicit over implicit (within reason)
  - Use meaningful variable and function names
  - Keep functions small and focused
  - Add comments for "why", not "what"

- **Choose familiar patterns**: Use patterns other Rust developers know
  - Standard library types and patterns
  - Common crate idioms
  - Reserve custom patterns for when they provide clear value

- **Keep complexity localized**: If complexity is necessary, contain it
  - Complex logic in well-named functions
  - Unsafe code in safe wrappers
  - Advanced features behind simple interfaces

## Documentation

- Document all public APIs with `///` doc comments
- Include examples in doc comments
- Use '//!' for module-level documentation
- Keep documentation up-to-date with code changes
- Use `#[doc(hidden)]` for internal implementation details

## Error Handling

- **Keep it simple first**: Use basic `Result` for simple cases
  - For internal apps or simple functions: `Result<T, Box<dyn std::error::Error>>`
  - For prototypes: `anyhow::Result<T>` is fine
  - Don't create custom error types until you need them
  - Example: `fn read_file(path: &Path) -> Result<String, std::io::Error>`

- **When to use different error types**:
  - **Applications**: Use `anyhow::Result<T>` for quick development
  - **Libraries**: Use `thiserror` to create structured error types
  - **Simple cases**: Use standard library error types like `std::io::Error`
  - **Custom errors**: Create error types only when you need specific error handling

- **Simple error patterns**:
  ```rust
  // Simple function with straightforward error
  fn parse_number(s: &str) -> Result<i32, std::num::ParseIntError> {
      s.parse()
  }

  // Using anyhow for application code
  use anyhow::{Context, Result};

  fn load_config() -> Result<Config> {
      let content = std::fs::read_to_string("config.toml")
          .context("Failed to read config file")?;
      toml::from_str(&content)
          .context("Failed to parse config TOML")
  }

  // Library error with thiserror
  use thiserror::Error;

  #[derive(Error, Debug)]
  pub enum MyError {
      #[error("File not found: {0}")]
      NotFound(PathBuf),
      #[error("Invalid format: {0}")]
      InvalidFormat(String),
  }
  ```

- **Error context and clarity**:
  - Provide context with `.context()` and `.with_context()` for chainable errors
  - Include actionable error messages (tell user what to do)
  - Return `Result<T, E>` instead of panicking
  - **NEVER use `.unwrap()` or `.expect()` in production code or tests**
  - **Exception**: Each unwrap() MUST have a comment justifying WHY it's acceptable:
    ```rust
    // System invariant: cannot fail due to validation above
    let timeout = global_config.timeout.unwrap();

    // TODO: Remove by v1.2.0 - temporary during refactoring
    let legacy = old_api.get_value().unwrap();
    ```
  - Acceptable combinators: `unwrap_or`, `unwrap_or_else`, `unwrap_or_default`
  - Handle all error cases explicitly
  - Use `?` operator for error propagation

- **Error handling best practices**:
  - Don't silence errors with `let _ = ...` unless intentional
  - Log errors at appropriate levels
  - Consider custom error types for public APIs
  - Make errors recoverable when possible

## Result/Option Combinators

- Use combinators to chain operations: `ok_or`, `ok_or_else`, `map_err`, `and_then`, `or_else`
- Prefer `?` for simple error propagation
- Use combinators for transformation: `map`, `and_then`, `unwrap_or`, `unwrap_or_else`
- Use `transpose()` for `Result<Option<T>>` ↔ `Option<Result<T>>`
- Chain methods instead of nested matches when possible
- Example: `value.ok_or_else(|| Error::NotFound)?.parse()?`

## Ownership & Borrowing

- Prefer borrowing (`&T`) over ownership when possible
- Use `&mut` sparingly and only when needed
- Avoid unnecessary clones
- Use `Cow<T>` for conditional ownership
- Leverage lifetime elision where possible

## Smart Pointer Usage

- **Start with ownership**: Don't reach for smart pointers immediately
  - Use plain ownership (`T`) and borrowing (`&T`, `&mut T`) first
  - Smart pointers add runtime cost and complexity
  - Only use them when they solve a specific problem

- **Common patterns**:
  - **`Box<T>`**: Heap allocation for large data, trait objects, recursive types
  - **`Arc<T>`**: Thread-safe shared ownership when you need it
  - **`Rc<T>`**: Single-threaded reference counting (avoid in async code)
  - **`Cow<T>`**: Clone-on-write for conditional ownership

- **Interior mutability** (use carefully):
  - **`Cell<T>`**: For copy types, single-threaded
  - **`RefCell<T>`**: Runtime borrow checking, single-threaded
  - **`Mutex<T>`**: Exclusive access, multi-threaded
  - **`RwLock<T>`**: Multiple readers or single writer, multi-threaded

- **When to use what**:
  - Use `Arc<Mutex<T>>` or `Arc<RwLock<T>>` for shared mutable state across threads
  - Prefer `RwLock` when reads vastly outnumber writes
  - Consider channels for message passing instead of shared state
  - Remember: `Mutex` can be a bottleneck - consider alternatives

- **Simple alternative**: Often, restructured code can avoid smart pointers entirely

## Type Safety

- Use newtypes for domain-specific types
- Prefer enums over booleans for state
- Use type aliases to clarify intent
- Leverage the type system to prevent invalid states
- Use `#[must_use]` on functions that should not be ignored

## Trait Design

- **Start without traits**: Use concrete types first
  - Don't create traits for a single implementation
  - Traits add complexity and cognitive overhead
  - Add traits when you need polymorphism or to constrain generics
  - Example: Start with `fn process(item: &Data)`, add trait only if multiple types need processing

- **Simple trait patterns**:
  - **Prefer composition over inheritance**: Use trait objects and trait bounds
  - **Associated types vs generics**: Use associated types when there's one natural implementation
  - **Implement standard traits thoughtfully**: `Debug`, `Display`, `Clone`, `Copy`, `Default`
  - **Conversion traits**: Implement `From`/`Into` for type conversions
  - **Trait bounds**: Prefer `where` clauses for complex bounds

- **Advanced patterns (use sparingly)**:
  - **Sealed trait pattern**: Prevent external implementations with private supertrait
    ```rust
    // Only use this if you really need to prevent external implementations
    mod sealed { pub trait Sealed {} }
    pub trait MyTrait: sealed::Sealed {}
    ```
  - **Marker traits**: Use zero-sized traits to encode compile-time properties
  - **Blanket implementations**: Use carefully to avoid conflicts

- **When to use traits**:
  - Multiple types need to implement the same behavior
  - You need to constrain generic types
  - You want to enable dynamic dispatch with trait objects
  - You're creating a public API that benefits from abstraction

## Practical API Design

- **Start simple**: Begin with the simplest API that solves the problem
  - You can always add complexity later if needed
  - Complex APIs are hard to simplify later
  - Example: Start with a simple function, add trait methods if multiple types need it

- **Prefer functions over traits**: Until you have multiple implementations
  - Functions are simpler to understand and use
  - Traits add cognitive overhead
  - Add traits when you need polymorphism or to constrain generics

- **Design for the common case**: Make frequent operations simple
  - Don't optimize for rare use cases at the expense of common ones
  - Provide convenience methods for frequent patterns
  - Example: `Vec::new()` is simple, `Vec::with_capacity()` for optimization

- **Consistent naming**: Follow Rust conventions and your own patterns
  - Use the same naming patterns across your API
  - Check similar crates for naming conventions
  - `from_`, `to_`, `as_`, `try_` prefixes have specific meanings

- **Error handling in APIs**: Be explicit and helpful
  - Return `Result` for operations that can fail
  - Use specific error types for libraries
  - Include context about what failed and why

- **Breaking changes**: Minimize and version carefully
  - Prefer additive changes over breaking ones
  - Use semantic versioning
  - Document breaking changes in changelog

- **Documentation**: Every public API needs examples
  - Show the most common usage first
  - Include edge cases and error handling
  - Examples should be copy-pasteable

## Pattern Matching

- **Exhaustive matching**: Use `match` to handle all cases
- **`if let` and `while let`**: For single-pattern matching
- **Destructuring**: In function parameters, let bindings, and matches
- **Match guards**: Use `if` conditions in match arms when needed
- **Ignore with `_`**: Be explicit about unused values
- **`@` bindings**: Bind and match simultaneously: `Some(x @ 1..=10)`
- **Or patterns**: `Some(1 | 2 | 3)` instead of multiple arms
- **Avoid wildcard `_` too early**: Place specific patterns before catch-all

## Collections & Iterators

- **Choose clarity over cleverness**: Iterators should be readable
  - A simple `for` loop is often clearer than complex iterator chains
  - Don't chain 5+ iterator methods - break it into steps
  - Example: `map(...).filter(...).collect()` is good, but not `map(...).filter(...).map(...).filter(...).fold(...).map(...)`

- **Iterator patterns**:
  - Prefer iterators over manual loops for simple transformations
  - Use iterator methods (`map`, `filter`, `fold`) for transformations
  - Pre-allocate collections with `with_capacity` when size is known
  - Avoid unnecessary allocations with iterator chains
  - Use `&str` instead of `String` when possible

- **When to use loops**:
  - Complex logic that's hard to express with iterators
  - When you need early exit or complex control flow
  - Performance-critical code where loops are measurably faster
  - When the loop is more readable than the iterator equivalent

- **Collection choices**:
  - Use appropriate collection types (Vec, HashMap, BTreeMap, etc.)
  - Prefer `Vec` for sequential data
  - Use `HashMap` for key-value lookups
  - Consider `BTreeMap` if you need ordered keys
  - Remember: different collections have different performance characteristics

## Builder Pattern

- Use for structs with many optional fields or complex validation
- Consider if you really need it - simple constructors are often better
- Enable method chaining for fluent APIs
- Consider using `derive_builder` crate for automatic generation

- **Simple example**:
  ```rust
  pub struct Config {
      name: String,
      port: u16,
      retries: Option<u32>,
  }

  impl Config {
      pub fn new(name: String, port: u16) -> Self {
          Self { name, port, retries: None }
      }

      pub fn with_retries(mut self, retries: u32) -> Self {
          self.retries = Some(retries);
          self
      }
  }

  // Usage:
  let config = Config::new("app".to_string(), 8080)
      .with_retries(3);
  ```

- **Full builder** (use only when necessary):
  ```rust
  pub struct ConfigBuilder {
      name: Option<String>,
      port: Option<u16>,
      retries: Option<u32>,
  }

  impl ConfigBuilder {
      pub fn new() -> Self {
          Self { name: None, port: None, retries: Some(3) }
      }

      pub fn name(mut self, name: String) -> Self {
          self.name = Some(name);
          self
      }

      pub fn build(self) -> Result<Config, String> {
          Ok(Config {
              name: self.name.ok_or("missing name")?,
              port: self.port.unwrap_or(8080),
              retries: self.retries,
          })
      }
  }
  ```
- Validate at build time, not construction time
- Remember: a simple `new()` function is often sufficient

## Testing Strategy

- **Test Structure**: Organize tests by type and scope
  - Unit tests: Test single functions/modules in `#[cfg(test)]` blocks
  - Integration tests: Test multiple modules together in `tests/` directory
  - Doctests: Example code in doc comments that serve as usage examples and tests
  - Performance tests: In `tests/stress/` directory for critical paths

- **Test Naming**: Be descriptive and consistent
  - Use `test_` prefix for test functions
  - Describe what is being tested and the expected outcome
  - Examples: `test_add_returns_sum_when_both_positive`, `test_error_when_file_not_found`

- **Test Organization**: Keep tests maintainable
  - Group related tests in nested modules
  - Use test utilities for common setup code
  - One assertion per test when possible
  - Use parameterized tests for similar cases

- **Test Data Management**: Ensure reproducibility
  - Create test data programmatically when possible
  - Check test files into version control if they're small and meaningful
  - Use deterministic data (avoid random values or fixed timestamps)
  - Clean up temporary files/directories in test teardown

- **Mocking and External Dependencies**: Isolate tests
  - Use dependency injection to enable testing
  - Mock external services and filesystem operations
  - Avoid network calls in tests unless specifically testing network code
  - Use in-memory implementations for databases and storage

- **Async Testing**: Handle async correctly
  - Use `#[tokio::test]` for async test functions
  - Prefer `tokio::fs` over `std::fs` in async tests
  - Avoid blocking operations in async tests
  - Use timeout futures to prevent hanging tests

- **Test Quality**: Write meaningful tests
  - Test behavior, not implementation details
  - Include happy path and error cases
  - Test edge cases and boundaries
  - Aim for >70% coverage, but focus on critical paths

- **Parallel Test Execution**: Ensure tests are thread-safe
  - **Use cargo nextest**: Run tests with `cargo nextest run` for parallel execution
  - **All tests must be parallel-safe**:
    - Avoid `serial_test` crate when possible
    - Never use `std::env::set_var` (causes data races between parallel tests)
    - Each test should use its own isolated temp directory
  - **Test isolation**:
    - Don't share state between tests
    - Use unique temp directories for each test
    - Reset global state in test setup/teardown

- **Doctest Best Practices**:
  - **Default to `no_run`**: Use `no_run` attribute for doctests unless there's a good reason to execute them
    - Doctests are primarily for documentation, not testing
    - Executable doctests can slow down documentation builds
    - Use `no_run` to prevent potential side effects or dependencies
  - Use `ignore` for examples that won't compile or need special setup
  - Only make doctests executable when they demonstrate actual runtime behavior
  - Keep doctests simple and focused on the API being documented
  - Test doctests with `cargo test --doc`

## Clippy Best Practices

- **Run in CI**: Use `cargo clippy -- -D warnings` to fail on warnings
- **Fix mode with uncommitted changes**: Use `cargo clippy --fix --allow-dirty` when there are uncommitted changes
- **Target all code**: Use `--all-targets` to include tests, benches, examples
- **Lint attributes**: Use `#[allow(clippy::lint_name)]` sparingly with justification
- **Regular runs**: Run clippy frequently during development, not just before commits

### Recommended Clippy Configuration

**Enforce these lints** (add to lib.rs/main.rs):
```rust
#![warn(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    missing_docs,
    missing_debug_implementations,
    missing_copy_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unstable_features,
    unused_import_braces,
    unused_qualifications,
)]
```

**Allow these when appropriate**:
```rust
#![allow(
    clippy::module_name_repetitions,  // Common in Rust APIs
    clippy::must_use_candidate,       // Can be too noisy
    clippy::missing_errors_doc,       // Use selectively
)]
```

## Code Review Guidelines

- **Review for clarity and simplicity**
  - Is the code easy to understand at a glance?
  - Are variable and function names descriptive?
  - Could complex parts be simplified?
  - Are there clever tricks that should be more explicit?

- **Check for common issues**:
  - Unused imports, variables, or dead code
  - Missing error handling or inappropriate panics
  - Performance anti-patterns (unnecessary clones, blocking in async)
  - Security issues (unsafe blocks, path traversal, credentials)
  - Cross-platform compatibility issues

- **Verify correctness**:
  - Does the code handle edge cases?
  - Are there potential race conditions or data races?
  - Is resource cleanup correct (Drop implementations)?
  - Are all error cases handled appropriately?

- **API design review**:
  - Is the API surface minimal and focused?
  - Are naming conventions followed?
  - Is error handling consistent with the rest of the codebase?
  - Could the API be simplified without losing functionality?

- **Test coverage**:
  - Are new features tested?
  - Do tests cover edge cases?
  - Are tests independent and repeatable?
  - Do tests actually test the intended behavior?

- **Documentation**:
  - Are public APIs documented?
  - Are doc examples correct and runnable?
  - Is complex logic explained with comments?
  - Does documentation match the implementation?

- **Review process best practices**:
  - Be constructive and specific in feedback
  - Explain why something is problematic, not just that it is
  - Suggest concrete improvements
  - Ask questions if you don't understand something
  - Consider the author's experience level when giving feedback

## Logging & Tracing

- **Use structured logging**: Leverage `tracing` crate for structured logs
- **Log levels**:
  - `trace!`: Very detailed debugging information
  - `debug!`: Debugging information for developers
  - `info!`: General informational messages
  - `warn!`: Warning messages for potentially problematic situations
  - `error!`: Error messages for failures
- **Structured fields**: Include context with key-value pairs
  ```rust
  tracing::info!(path = %file_path, size = file_size, "Processing file");
  ```
- **Tracing spans**: Use spans for operation context
  ```rust
  let span = tracing::info_span!("install", dependency = %name);
  let _enter = span.enter();
  ```
- **Progress indication**: Use `indicatif` crate for progress bars and spinners
- **Avoid println!**: Use logging macros instead of direct console output
- **Configure subscribers**: Set up `tracing_subscriber` in main/tests

## Dependency Management

- Prefer well-maintained crates
- Check for security advisories
- Keep dependencies minimal
- Use workspace dependencies for multi-crate projects
- Pin versions for applications, use ranges for libraries
- Audit dependencies regularly with `cargo audit`
- Check license compatibility

## Performance Considerations

- **Don't prematurely optimize**: Profile first, optimize later
  - Measure actual bottlenecks before making changes
  - Use tools like `cargo flamegraph`, `criterion`, `perf`
  - Optimize based on data, not assumptions
  - Remember: readability is often more important than micro-optimizations

- **Common performance patterns**:
  - Use `&str` instead of `String` when possible
  - Prefer iterators over collecting into intermediate collections
  - Use `Arc` and `Rc` judiciously (they have overhead)
  - Consider zero-copy patterns for large data
  - Use `#[inline]` for small, frequently called functions

- **When to optimize**:
  - After profiling shows a bottleneck
  - For hot paths in performance-critical code
  - When dealing with large datasets or high-frequency operations
  - When memory usage is problematic

- **Advanced optimizations (use with justification)**:
  - Leverage const generics and const functions
  - Avoid unnecessary heap allocations
  - Consider using `SmallVec` for small collections
  - Use unsafe code only when measurements show significant benefit

- **Performance myths to avoid**:
  - Don't replace clear loops with complex combinators unless it's a bottleneck
  - Don't optimize code that runs rarely
  - Don't make APIs awkward just for performance
  - Don't use unsafe without measurable benefit

## String Allocation Patterns

**Consistent patterns for string operations:**

### Conversion Rules
```rust
// &str → String
let s: String = str_ref.to_string();

// String → String  
let s2: String = s.clone();

// Modern interpolation (Rust 2021+)
let s3: String = format!("value: {x}, count: {y}");

// NOT: format!("value: {}, count: {}", x, y)
```

### Function Parameters
```rust
// ✅ Prefer borrowing
fn process(name: &str) -> Result<()> { ... }

// ❌ Avoid unnecessary ownership
fn process(name: String) -> Result<()> { ... }
```

### Performance Guidelines
- Use `&str` instead of `String` when possible
- Use `Cow<'_, str>` for conditional ownership
- Consider `Arc<String>` for frequently shared strings
- Minimize allocations in hot paths
- Profile before optimizing

### Modern Format Examples
```rust
// ✅ Modern interpolation
format!("File '{file_path}' size: {size}")
format!("Resource: {name}@{type:?}")
format!("Hash: {hash:x}")

// ❌ Legacy positional (avoid)
format!("File '{}' size: {}", file_path, size)
```

## Cross-Platform Development

- **Path separator handling**: CRITICAL for Windows/macOS/Linux compatibility
  - **Storage/serialization**: Always use forward slashes `/` in:
    - Lockfiles and manifest files (TOML, JSON)
    - `.gitignore` entries (Git requirement)
    - Any serialized path representation
  - **Runtime operations**: Use `Path`/`PathBuf` for filesystem operations (automatic platform handling)
  - **`Path::display()` gotcha**: Produces platform-specific separators (backslashes on Windows)
    - Always use helper when storing: `normalize_path_for_storage()`
    - Import: `use crate::utils::normalize_path_for_storage;`
    - Example: `normalize_path_for_storage(format!("{}/{}", path.display(), file))`
  - **Use `join()` not string concatenation**: `path.join("file")` not `format!("{}/file", path)`
- **Windows-specific considerations**:
  - Absolute paths: `C:\path` or `\\server\share` (UNC paths)
  - Reserved filenames: CON, PRN, AUX, NUL, COM1-9, LPT1-9
  - Case-insensitive filesystem (but preserves case)
  - `file://` URLs use forward slashes even on Windows
  - Test on real Windows, not WSL (different behavior)
- **Line endings**: Use `\n` in code, let Git handle CRLF conversion
- **Environment variables**: Use `std::env::var_os` for non-UTF8 safety
- **Path validation**: Consider `dunce` crate to normalize Windows paths
- **Testing**: Run CI on all target platforms (Windows, macOS, Linux)

## Async Rust

- Use `tokio` for async runtime
- Avoid blocking in async contexts
- Use `async-trait` when needed
- Handle cancellation properly
- Consider using `futures` combinators
- **Use `tokio::fs` for file I/O**: Never mix blocking `std::fs` in async code

## Concurrency

- Use channels for communication between threads
- Prefer `std::sync::Arc` for shared ownership
- Use `std::sync::Mutex` or `RwLock` for shared mutable state
- Avoid data races with Rust's ownership rules
- Consider using `crossbeam` for advanced concurrency patterns

## Macro Usage

- **Prefer functions over macros**: Macros should be a last resort
  - Macros are harder to debug and understand
  - Functions provide better type checking and error messages
- **Use macros when necessary**:
  - Reducing boilerplate (e.g., `vec!`, `format!`)
  - Generating repetitive code at compile time
  - Creating DSLs (domain-specific languages)
  - Variadic functions (different number of arguments)
- **Declarative macros** (`macro_rules!`):
  - Pattern-based matching and expansion
  - Good for simple repetitive code
  - Use hygiene to avoid name collisions
- **Procedural macros**:
  - Derive macros: `#[derive(MyTrait)]`
  - Attribute macros: `#[my_attribute]`
  - Function-like macros: `my_macro!(...)`
  - More powerful but more complex
- **Macro hygiene**: Be aware of variable capture and naming conflicts
- **Document macro usage**: Include examples of how to use your macros
- **Test macros thoroughly**: Macro errors can be cryptic

## Unsafe Code

- Avoid unsafe unless absolutely necessary
- Document safety invariants clearly with `# Safety` section
- Use `unsafe` blocks minimally - keep them small
- Consider safe abstractions first
- Run Miri for undefined behavior detection
- Use `#[deny(unsafe_code)]` when possible to prevent accidental usage
- Validate all unsafe assumptions with comments and assertions
- Encapsulate unsafe in safe APIs
- Review unsafe code extra carefully during code review
