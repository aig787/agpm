# Architecture Best Practices

## Module Organization

- Keep modules focused and cohesive
- Use `mod.rs` for module roots
- Separate concerns clearly
- Export public APIs thoughtfully

## Error Handling

- Use `anyhow::Result<T>` for application errors
- Use `thiserror` for library error types
- Provide context with `.context()` and `.with_context()`
- Include actionable error messages

## Testing Strategy

- Write unit tests in the same file as the code
- Put integration tests in `tests/` directory
- Aim for >70% test coverage
- Use property-based testing where appropriate
- Mock external dependencies

## Dependency Management

- Prefer well-maintained crates
- Check for security advisories
- Keep dependencies minimal
- Use workspace dependencies for multi-crate projects
- Pin versions for applications, use ranges for libraries

## Performance Considerations

- Profile before optimizing
- Use `&str` instead of `String` when possible
- Prefer iterators over collecting
- Use `Arc` and `Rc` judiciously
- Consider zero-copy patterns
- Leverage const generics and const functions

## Async Rust

- Use `tokio` for async runtime
- Avoid blocking in async contexts
- Use `async-trait` when needed
- Handle cancellation properly
- Consider using `futures` combinators

## Unsafe Code

- Avoid unsafe unless absolutely necessary
- Document safety invariants
- Use `unsafe` blocks minimally
- Consider safe abstractions first
- Run Miri for undefined behavior detection
