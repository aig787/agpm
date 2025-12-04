//! Token counting utilities using BPE tokenization.
//!
//! This module provides approximate token counting for resource content using the
//! cl100k BPE encoding, which is compatible with Claude and GPT-4 models.
//!
//! # Usage
//!
//! ```rust,no_run
//! use agpm_cli::tokens;
//!
//! let content = "Hello, world!";
//! let count = tokens::count_tokens(content);
//! println!("Approximate token count: {}", count);
//! ```
//!
//! # Performance
//!
//! The tokenizer is lazily initialized on first use and cached for subsequent calls.
//! Token counting is O(n) and optimized for high throughput.

/// Get the cached tokenizer instance.
///
/// The bpe-openai crate uses LazyLock internally, so we just return the static reference.
fn get_tokenizer() -> &'static bpe_openai::Tokenizer {
    bpe_openai::cl100k_base()
}

/// Count approximate tokens in content using cl100k encoding.
///
/// This uses the cl100k BPE encoding which is compatible with Claude and GPT-4.
/// The count is approximate since different models may use slightly different
/// tokenization schemes.
///
/// # Arguments
///
/// * `content` - The text content to count tokens for
///
/// # Returns
///
/// The approximate number of tokens in the content.
///
/// # Example
///
/// ```rust,no_run
/// use agpm_cli::tokens::count_tokens;
///
/// let tokens = count_tokens("Hello, world!");
/// assert!(tokens > 0);
/// ```
#[must_use]
pub fn count_tokens(content: &str) -> usize {
    get_tokenizer().count(content)
}

/// Format a token count for human-readable display.
///
/// Formats large numbers with k/M suffixes for readability.
///
/// # Arguments
///
/// * `count` - The token count to format
///
/// # Returns
///
/// A formatted string representation (e.g., "150.2k", "1.5M").
///
/// # Examples
///
/// ```rust
/// use agpm_cli::tokens::format_token_count;
///
/// assert_eq!(format_token_count(500), "500");
/// assert_eq!(format_token_count(1500), "1.5k");
/// assert_eq!(format_token_count(1500000), "1.5M");
/// ```
#[must_use]
pub fn format_token_count(count: usize) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}k", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_tokens_empty() {
        assert_eq!(count_tokens(""), 0);
    }

    #[test]
    fn test_count_tokens_simple() {
        // "Hello, world!" should be a few tokens
        let count = count_tokens("Hello, world!");
        assert!(count > 0);
        assert!(count < 10);
    }

    #[test]
    fn test_count_tokens_longer() {
        let content = "This is a longer piece of text that should result in more tokens. \
            The quick brown fox jumps over the lazy dog.";
        let count = count_tokens(content);
        assert!(count > 10);
    }

    #[test]
    fn test_format_token_count_small() {
        assert_eq!(format_token_count(0), "0");
        assert_eq!(format_token_count(1), "1");
        assert_eq!(format_token_count(999), "999");
    }

    #[test]
    fn test_format_token_count_thousands() {
        assert_eq!(format_token_count(1000), "1.0k");
        assert_eq!(format_token_count(1500), "1.5k");
        assert_eq!(format_token_count(12500), "12.5k");
        assert_eq!(format_token_count(999999), "1000.0k");
    }

    #[test]
    fn test_format_token_count_millions() {
        assert_eq!(format_token_count(1_000_000), "1.0M");
        assert_eq!(format_token_count(1_500_000), "1.5M");
        assert_eq!(format_token_count(10_000_000), "10.0M");
    }
}
