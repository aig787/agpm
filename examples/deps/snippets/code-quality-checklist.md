---
name: code-quality-checklist
description: Comprehensive checklist for code reviews and quality assurance
tags: [code-review, quality, checklist]
---

# Code Quality Checklist

## Code Structure

- [ ] **Single Responsibility**: Each function/class has one clear purpose
- [ ] **DRY Principle**: No unnecessary code duplication
- [ ] **Appropriate Abstraction**: Not over-engineered, not under-engineered
- [ ] **Clear Naming**: Variables, functions, and types have descriptive names
- [ ] **Consistent Style**: Follows project coding conventions

## Error Handling

- [ ] **Proper Error Types**: Uses appropriate error types (Result, Option, custom errors)
- [ ] **Error Propagation**: Errors are properly propagated with context
- [ ] **No Silent Failures**: All errors are logged or handled appropriately
- [ ] **Recovery Logic**: Appropriate fallback behavior for failures
- [ ] **Resource Cleanup**: Resources are cleaned up even on error paths

## Testing

- [ ] **Unit Tests**: Core logic is covered by unit tests
- [ ] **Integration Tests**: Component interactions are tested
- [ ] **Edge Cases**: Boundary conditions and edge cases are tested
- [ ] **Error Cases**: Failure modes are tested
- [ ] **Test Clarity**: Tests are readable and well-documented
- [ ] **No Flaky Tests**: Tests are deterministic and reliable

## Performance

- [ ] **Efficient Algorithms**: No unnecessary O(n²) operations
- [ ] **Memory Usage**: No memory leaks or excessive allocations
- [ ] **I/O Efficiency**: File and network I/O is optimized
- [ ] **Lazy Evaluation**: Expensive operations are deferred when possible
- [ ] **Caching**: Repeated computations are cached appropriately

## Security

- [ ] **Input Validation**: All inputs are validated and sanitized
- [ ] **No Hardcoded Secrets**: Credentials are not in source code
- [ ] **SQL Injection**: Queries use parameterization
- [ ] **Path Traversal**: File paths are validated
- [ ] **Authentication**: Proper authentication and authorization checks

## Documentation

- [ ] **Public API Docs**: All public functions/types are documented
- [ ] **Examples**: Complex functionality includes usage examples
- [ ] **Comments**: Complex logic is explained with comments
- [ ] **README Updated**: User-facing changes are documented
- [ ] **Changelog**: Breaking changes are noted

## Maintainability

- [ ] **No Magic Numbers**: Constants are named and documented
- [ ] **No God Classes**: Large classes are broken down
- [ ] **Limited Dependencies**: External dependencies are justified
- [ ] **Backwards Compatibility**: Changes maintain API compatibility
- [ ] **Deprecation Warnings**: Old APIs have deprecation notices

## Git Hygiene

- [ ] **Atomic Commits**: Each commit is a logical unit
- [ ] **Clear Messages**: Commit messages explain the "why"
- [ ] **No Debug Code**: No commented-out code or debug prints
- [ ] **Clean History**: No merge commits or fixup commits in PR
- [ ] **PR Description**: PR clearly explains the changes

## Language-Specific (Rust)

- [ ] **Ownership**: Proper use of ownership, borrowing, and lifetimes
- [ ] **No Unwrap**: Avoid .unwrap() except in tests or when justified
- [ ] **Clippy Clean**: No clippy warnings (or explicitly allowed)
- [ ] **No Unsafe**: Unsafe code is avoided or well-justified
- [ ] **Trait Bounds**: Generic bounds are minimal and appropriate
