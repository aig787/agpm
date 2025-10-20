---
name: rust-patterns
description: Common Rust patterns and idioms for writing idiomatic code
tags: [rust, patterns, best-practices]
---

# Rust Patterns and Idioms

## Error Handling

### Result Type Pattern
```rust
fn parse_config(path: &Path) -> Result<Config, ConfigError> {
    let contents = fs::read_to_string(path)
        .map_err(ConfigError::IoError)?;

    toml::from_str(&contents)
        .map_err(ConfigError::ParseError)
}
```

### Custom Error Types
```rust
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Failed to parse config: {0}")]
    ParseError(#[from] toml::de::Error),
}
```

## Builder Pattern

```rust
pub struct Client {
    url: String,
    timeout: Duration,
    retry_count: u32,
}

impl Client {
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }
}

#[derive(Default)]
pub struct ClientBuilder {
    url: Option<String>,
    timeout: Option<Duration>,
    retry_count: Option<u32>,
}

impl ClientBuilder {
    pub fn url(mut self, url: String) -> Self {
        self.url = Some(url);
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn build(self) -> Result<Client, &'static str> {
        Ok(Client {
            url: self.url.ok_or("URL is required")?,
            timeout: self.timeout.unwrap_or(Duration::from_secs(30)),
            retry_count: self.retry_count.unwrap_or(3),
        })
    }
}
```

## Newtype Pattern

```rust
pub struct UserId(u64);
pub struct OrderId(u64);

impl UserId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

// Prevents accidental mixing of different ID types
fn get_user(id: UserId) -> User {
    // Can't accidentally pass OrderId here
}
```

## Option Combinators

```rust
// Instead of nested if-let
fn process_data(data: Option<String>) -> Option<usize> {
    data.as_ref()
        .filter(|s| !s.is_empty())
        .map(|s| s.len())
}

// Using and_then for chaining
fn get_user_email(user_id: UserId) -> Option<String> {
    get_user(user_id)
        .and_then(|user| user.email)
}
```

## Iterator Patterns

```rust
// Collect into specific types
let numbers: Vec<i32> = (1..=10).collect();
let set: HashSet<_> = numbers.iter().collect();

// Chain iterators
let combined: Vec<_> = vec1.iter()
    .chain(vec2.iter())
    .filter(|&&x| x > 0)
    .collect();

// Partition based on predicate
let (evens, odds): (Vec<_>, Vec<_>) = numbers
    .into_iter()
    .partition(|&n| n % 2 == 0);
```

## RAII Pattern

```rust
pub struct FileGuard {
    path: PathBuf,
}

impl FileGuard {
    pub fn new(path: PathBuf) -> io::Result<Self> {
        File::create(&path)?;
        Ok(Self { path })
    }
}

impl Drop for FileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
```

## Type State Pattern

```rust
pub struct Connection<State> {
    addr: String,
    state: PhantomData<State>,
}

pub struct Disconnected;
pub struct Connected;

impl Connection<Disconnected> {
    pub fn new(addr: String) -> Self {
        Connection {
            addr,
            state: PhantomData,
        }
    }

    pub fn connect(self) -> Result<Connection<Connected>, Error> {
        // Connect logic
        Ok(Connection {
            addr: self.addr,
            state: PhantomData,
        })
    }
}

impl Connection<Connected> {
    pub fn send(&self, data: &[u8]) -> Result<(), Error> {
        // Can only send when connected
        Ok(())
    }
}
```
