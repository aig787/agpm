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
3. ✅ Run `cargo nextest run` (or `cargo test`) to verify tests pass
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

## Documentation

- Document all public APIs with `///` doc comments
- Include examples in doc comments
- Use '//!' for module-level documentation
- Keep documentation up-to-date with code changes
- Use `#[doc(hidden)]` for internal implementation details

## Error Handling

- Use `anyhow::Result<T>` for application errors
- Use `thiserror` for library error types
- Provide context with `.context()` and `.with_context()`
- Include actionable error messages
- Return `Result<T, E>` instead of panicking
- Use `.expect()` only when panic is truly acceptable
- Handle all error cases explicitly
- Use `?` operator for error propagation

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

- **`Box<T>`**: Heap allocation, trait objects, recursive types
- **`Rc<T>`**: Single-threaded reference counting (avoid in async code)
- **`Arc<T>`**: Thread-safe reference counting for shared ownership
- **`Cow<T>`**: Clone-on-write for conditional ownership
- **Interior Mutability**: `Cell<T>`, `RefCell<T>`, `Mutex<T>`, `RwLock<T>`
  - `Cell<T>`: Copy types, single-threaded
  - `RefCell<T>`: Runtime borrow checking, single-threaded
  - `Mutex<T>`: Exclusive access, multi-threaded
  - `RwLock<T>`: Multiple readers or single writer, multi-threaded
- Use `Arc<Mutex<T>>` or `Arc<RwLock<T>>` for shared mutable state across threads
- Prefer `RwLock` when reads vastly outnumber writes

## Type Safety

- Use newtypes for domain-specific types
- Prefer enums over booleans for state
- Use type aliases to clarify intent
- Leverage the type system to prevent invalid states
- Use `#[must_use]` on functions that should not be ignored

## Trait Design

- **Prefer composition over inheritance**: Use trait objects and trait bounds
- **Associated types vs generics**: Use associated types when there's one natural implementation
- **Implement standard traits thoughtfully**: `Debug`, `Display`, `Clone`, `Copy`, `Default`
- **Conversion traits**: Implement `From`/`Into` for type conversions
- **Sealed trait pattern**: Prevent external implementations with private supertrait
  ```rust
  mod sealed { pub trait Sealed {} }
  pub trait MyTrait: sealed::Sealed {}
  ```
- **Marker traits**: Use zero-sized traits to encode compile-time properties
- **Trait bounds**: Prefer `where` clauses for complex bounds
- **Blanket implementations**: Use carefully to avoid conflicts

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

- Prefer iterators over manual loops
- Use iterator methods (`map`, `filter`, `fold`) for transformations
- Pre-allocate collections with `with_capacity` when size is known
- Use appropriate collection types (Vec, HashMap, BTreeMap, etc.)
- Avoid unnecessary allocations with iterator chains
- Use `&str` instead of `String` when possible
- Prefer iterators over collecting

## Builder Pattern

- Use for structs with many optional fields
- Enable method chaining for fluent APIs
- Consider using `derive_builder` crate for automatic generation
- Example pattern:
  ```rust
  pub struct Config {
      field1: String,
      field2: Option<u32>,
  }

  pub struct ConfigBuilder {
      field1: Option<String>,
      field2: Option<u32>,
  }

  impl ConfigBuilder {
      pub fn field1(mut self, value: String) -> Self {
          self.field1 = Some(value);
          self
      }

      pub fn build(self) -> Result<Config, BuildError> {
          Ok(Config {
              field1: self.field1.ok_or(BuildError::MissingField)?,
              field2: self.field2,
          })
      }
  }
  ```
- Validate at build time, not construction time
- Use `build()` or `try_build()` for final construction

## Testing Strategy

- Write unit tests in the same file as the code
- Put integration tests in `tests/` directory
- Aim for >70% test coverage
- Use property-based testing where appropriate
- Mock external dependencies
- Use `#[cfg(test)]` for test modules
- Name test functions descriptively
- Use `assert!`, `assert_eq!`, and `assert_ne!` appropriately
- **Use cargo nextest**: Run tests with `cargo nextest run` for parallel execution
- **All tests must be parallel-safe**:
  - Avoid `serial_test` crate when possible
  - Never use `std::env::set_var` (causes data races between parallel tests)
  - Each test should use its own isolated temp directory
- **Use `tokio::fs` in async tests**: Not `std::fs` for proper async I/O
- **Doctest configuration**: Use `no_run` attribute for examples that demonstrate usage but shouldn't execute
  - Use `ignore` for examples that won't compile

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

- Profile before optimizing
- Use `&str` instead of `String` when possible
- Prefer iterators over collecting
- Use `Arc` and `Rc` judiciously
- Consider zero-copy patterns
- Leverage const generics and const functions
- Use `#[inline]` judiciously
- Avoid unnecessary heap allocations
- Consider using `SmallVec` for small collections

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
