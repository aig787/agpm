---
name: api-design-principles
description: Best practices for designing clean, maintainable APIs
tags: [api, design, best-practices]
---

# API Design Principles

## General Principles

### 1. Consistency is King
- Use consistent naming conventions across the API
- Follow established patterns from the language/framework
- Be predictable in structure and behavior

### 2. Make the Simple Easy, the Complex Possible
- Common use cases should require minimal code
- Advanced features should be available but not in the way
- Provide sensible defaults

### 3. Design for Evolution
- Plan for future additions without breaking changes
- Use extensible data structures
- Consider versioning strategy from the start

## Naming Conventions

### Be Descriptive and Unambiguous
```rust
// Good
fn parse_config_file(path: &Path) -> Result<Config>

// Bad
fn parse(path: &Path) -> Result<Config>
fn get_config(path: &Path) -> Result<Config>  // "get" is vague
```

### Use Domain Language
```rust
// Good - uses domain terminology
struct ShoppingCart {
    fn add_item(&mut self, item: Item)
    fn checkout(&self) -> Order
}

// Bad - generic terminology
struct Container {
    fn add(&mut self, thing: Thing)
    fn process(&self) -> Result
}
```

## Function Design

### Single Responsibility
```rust
// Good - each function does one thing
fn load_config(path: &Path) -> Result<String>
fn parse_config(content: &str) -> Result<Config>

// Bad - does too much
fn load_and_parse_and_validate_config(path: &Path) -> Result<Config>
```

### Avoid Boolean Parameters
```rust
// Good - explicit and self-documenting
enum CacheStrategy {
    UseCache,
    BypassCache,
}
fn fetch_data(url: &str, cache: CacheStrategy) -> Result<Data>

// Bad - unclear what true/false means
fn fetch_data(url: &str, use_cache: bool) -> Result<Data>
```

### Return Meaningful Types
```rust
// Good - uses type system for safety
fn get_user(id: UserId) -> Result<User, UserError>

// Bad - stringly-typed errors
fn get_user(id: u64) -> Result<User, String>
```

## Error Handling

### Provide Context
```rust
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to read config file at {path}: {source}")]
    ReadError {
        path: PathBuf,
        source: io::Error,
    },

    #[error("Invalid configuration: {0}")]
    ValidationError(String),
}
```

### Distinguish Recoverable from Unrecoverable
```rust
// Recoverable - returns Result
fn connect_to_server(addr: &str) -> Result<Connection, ConnectError>

// Unrecoverable - panics
fn initialize() {
    let config = load_config()
        .expect("Config must exist for application to run");
}
```

## Type Design

### Use Newtypes for Type Safety
```rust
// Good - impossible to mix up different IDs
pub struct UserId(u64);
pub struct OrderId(u64);

fn process_order(order_id: OrderId) -> Result<()>

// Bad - easy to pass wrong ID
fn process_order(order_id: u64) -> Result<()>
```

### Leverage the Builder Pattern
```rust
// Good - clear and flexible
let client = HttpClient::builder()
    .timeout(Duration::from_secs(30))
    .retry_count(3)
    .build()?;

// Bad - too many parameters
let client = HttpClient::new(
    "https://api.example.com",
    Duration::from_secs(30),
    3,
    true,
    false,
    None,
)?;
```

## Async API Design

### Be Clear About Async Boundaries
```rust
// Good - async is visible in signature
pub async fn fetch_data(url: &str) -> Result<Data>

// Also good - returns Future explicitly
pub fn fetch_data(url: &str) -> impl Future<Output = Result<Data>>
```

### Avoid Blocking in Async
```rust
// Good - uses async I/O
pub async fn read_file(path: &Path) -> Result<String> {
    tokio::fs::read_to_string(path).await
}

// Bad - blocks the executor
pub async fn read_file(path: &Path) -> Result<String> {
    std::fs::read_to_string(path)  // Blocking!
}
```

## Documentation

### Document Invariants and Contracts
```rust
/// Parses a configuration file.
///
/// # Arguments
/// * `path` - Path to the config file (must exist)
///
/// # Returns
/// The parsed configuration, or an error if:
/// - The file cannot be read
/// - The file contains invalid TOML
/// - Required fields are missing
///
/// # Example
/// ```no_run
/// let config = parse_config(Path::new("config.toml"))?;
/// ```
pub fn parse_config(path: &Path) -> Result<Config>
```

### Document Panics
```rust
/// Gets the user with the given ID.
///
/// # Panics
/// Panics if the database connection pool is exhausted.
pub fn get_user(&self, id: UserId) -> Result<User>
```

## Versioning

### Semantic Versioning
- MAJOR: Breaking changes
- MINOR: New features (backwards compatible)
- PATCH: Bug fixes

### Deprecation Strategy
```rust
/// Gets the user email.
///
/// # Deprecated
/// Use [`User::email_address`] instead. This method will be
/// removed in version 2.0.0.
#[deprecated(since = "1.5.0", note = "Use email_address instead")]
pub fn get_email(&self) -> &str {
    &self.email
}
```
