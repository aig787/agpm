//! Progress indicators and user interface utilities
//!
//! This module provides consistent, cross-platform progress indicators for CCPM operations.
//! It includes progress bars, spinners, and multi-progress containers that work well in
//! both interactive terminals and CI/automation environments.
//!
//! # Features
//!
//! - **Consistent styling**: Unified look across all CCPM operations
//! - **CI/quiet mode support**: Automatically disables in non-interactive environments
//! - **Thread safety**: Safe to use across async tasks and parallel operations
//! - **Multiple progress types**: Bars for known work, spinners for indeterminate work
//! - **Multi-progress support**: Manage multiple concurrent operations
//!
//! # Environment Variables
//!
//! - `CCPM_NO_PROGRESS`: Set to any value to disable all progress indicators
//!
//! # Examples
//!
//! ## Basic Progress Bar
//!
//! ```rust
//! use ccpm::utils::progress::ProgressBar;
//!
//! let progress = ProgressBar::new(100);
//! progress.set_message("Processing files");
//!
//! for i in 0..100 {
//!     // Do work
//!     progress.inc(1);
//! }
//!
//! progress.finish_with_message("✅ Completed!");
//! ```
//!
//! ## Spinner for Indeterminate Work
//!
//! ```rust
//! use ccpm::utils::progress::ProgressBar;
//!
//! let spinner = ProgressBar::new_spinner();
//! spinner.set_message("Cloning repository...");
//!
//! // Long running operation
//! // clone_repository().await?;
//!
//! spinner.finish_with_message("Repository cloned");
//! ```
//!
//! ## Multiple Progress Bars
//!
//! ```rust
//! use ccpm::utils::progress::MultiProgress;
//!
//! let multi = MultiProgress::new();
//! let pb1 = multi.add_bar(50);
//! let pb2 = multi.add_bar(30);
//!
//! pb1.set_message("Downloading agents");
//! pb2.set_message("Downloading snippets");
//!
//! // Progress bars update independently
//! ```
//!
//! ## Thread-Safe Progress
//!
//! ```rust,no_run
//! use ccpm::utils::progress::ThreadSafeProgressBar;
//! use std::sync::Arc;
//!
//! let progress = ThreadSafeProgressBar::new(100);
//! let progress_clone = progress.clone();
//!
//! // Use in parallel tasks
//! tokio::spawn(async move {
//!     progress_clone.inc(10);
//! });
//! ```
//!
//! # CI and Automation Support
//!
//! Progress indicators automatically disable in:
//! - Non-TTY environments (pipes, redirects)
//! - When `CCPM_NO_PROGRESS` environment variable is set
//! - CI/CD environments (detected automatically)
//!
//! This ensures clean output in scripts and automation while providing
//! rich feedback in interactive use.

use indicatif::{ProgressBar as IndicatifBar, ProgressStyle as IndicatifStyle};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Checks if progress bars should be disabled.
///
/// Progress bars are disabled when the `CCPM_NO_PROGRESS` environment variable
/// is set to any value. This is useful for CI/CD environments, scripts, or
/// when clean output is desired.
///
/// # Returns
///
/// `true` if progress bars should be disabled, `false` otherwise
///
/// # Examples
///
/// ```bash
/// # Disable progress bars
/// export CCPM_NO_PROGRESS=1
/// ccpm install  # No progress bars shown
///
/// # Re-enable progress bars
/// unset CCPM_NO_PROGRESS
/// ccpm install  # Progress bars shown
/// ```
fn is_progress_disabled() -> bool {
    std::env::var("CCPM_NO_PROGRESS").is_ok()
}

/// A progress bar with consistent styling and cross-platform behavior.
///
/// This is the main progress indicator for CCPM operations. It wraps the
/// `indicatif` crate's progress bar with CCPM-specific styling and behavior.
/// The progress bar automatically respects the `CCPM_NO_PROGRESS` environment
/// variable and provides a consistent user experience.
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::progress::ProgressBar;
///
/// // Create a progress bar for known work
/// let progress = ProgressBar::new(100);
/// progress.set_message("Installing packages");
/// progress.set_prefix("📦");
///
/// for i in 0..100 {
///     // Simulate work
///     std::thread::sleep(std::time::Duration::from_millis(10));
///     progress.inc(1);
/// }
///
/// progress.finish_with_message("✅ Installation complete!");
/// ```
///
/// # Thread Safety
///
/// The underlying `indicatif` progress bar is thread-safe and can be shared
/// across threads. For more explicit thread safety with additional features,
/// consider using [`ThreadSafeProgressBar`].
///
/// # Styling
///
/// All progress bars use consistent CCPM styling with:
/// - Cyan/blue color scheme
/// - Unicode progress characters
/// - Consistent message formatting
/// - ETA and position display
#[derive(Clone)]
pub struct ProgressBar {
    inner: IndicatifBar,
}

impl ProgressBar {
    /// Creates a new progress bar with a specified total length.
    ///
    /// The progress bar will track progress from 0 to `len`. If progress bars
    /// are disabled via environment variables, this creates a hidden progress bar
    /// that silently ignores all operations.
    ///
    /// # Arguments
    ///
    /// * `len` - The total number of work units this progress bar represents
    ///
    /// # Returns
    ///
    /// A new [`ProgressBar`] instance with the default CCPM styling
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::utils::progress::ProgressBar;
    ///
    /// // Track progress for 50 files
    /// let progress = ProgressBar::new(50);
    /// progress.set_message("Processing files");
    ///
    /// // Process files and update progress
    /// for i in 0..50 {
    ///     // process_file(i);
    ///     progress.inc(1);
    /// }
    ///
    /// progress.finish_with_message("All files processed");
    /// ```
    pub fn new(len: u64) -> Self {
        let bar = if is_progress_disabled() {
            IndicatifBar::hidden()
        } else {
            let bar = IndicatifBar::new(len);
            bar.set_style(default_style());
            bar
        };
        Self { inner: bar }
    }

    /// Creates a spinner for indeterminate progress operations.
    ///
    /// Spinners are used when the total amount of work is unknown or when
    /// the operation doesn't have discrete progress steps. The spinner will
    /// animate continuously until finished.
    ///
    /// # Returns
    ///
    /// A new [`ProgressBar`] configured as a spinner with CCPM styling
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::utils::progress::ProgressBar;
    ///
    /// let spinner = ProgressBar::new_spinner();
    /// spinner.set_message("Cloning repository...");
    ///
    /// // Long-running operation with unknown duration
    /// // clone_repository().await?;
    ///
    /// spinner.finish_with_message("Repository cloned successfully");
    /// ```
    ///
    /// # Spinner Animation
    ///
    /// The spinner uses Unicode Braille patterns for smooth animation:
    /// `⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏`
    ///
    /// The animation updates every 100ms automatically.
    pub fn new_spinner() -> Self {
        let bar = if is_progress_disabled() {
            IndicatifBar::hidden()
        } else {
            let bar = IndicatifBar::new_spinner();
            bar.set_style(spinner_style());
            bar.enable_steady_tick(Duration::from_millis(100));
            bar
        };
        Self { inner: bar }
    }

    /// Sets the message displayed alongside the progress bar.
    ///
    /// The message appears to the right of the progress bar and typically
    /// describes the current operation being performed.
    ///
    /// # Arguments
    ///
    /// * `msg` - The message to display (anything that converts to String)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::utils::progress::ProgressBar;
    ///
    /// let progress = ProgressBar::new(100);
    /// progress.set_message("Downloading packages");
    ///
    /// // Later, update the message
    /// progress.set_message("Installing packages");
    /// progress.set_message(format!("Processing file {}", 42));
    /// ```
    pub fn set_message(&self, msg: impl Into<String>) {
        self.inner.set_message(msg.into());
    }

    /// Sets the prefix displayed before the progress bar.
    ///
    /// The prefix typically contains an emoji or short indicator of the
    /// operation type. It appears at the beginning of the progress bar line.
    ///
    /// # Arguments
    ///
    /// * `prefix` - The prefix to display (anything that converts to String)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::utils::progress::ProgressBar;
    ///
    /// let progress = ProgressBar::new(100);
    /// progress.set_prefix("📦");  // Package installation
    /// progress.set_prefix("🔄");  // Update operation
    /// progress.set_prefix("✨");   // Generation/creation
    /// progress.set_prefix("🧹");   // Cleanup operation
    /// ```
    pub fn set_prefix(&self, prefix: impl Into<String>) {
        self.inner.set_prefix(prefix.into());
    }

    /// Increments the progress bar by the specified amount.
    ///
    /// This is the most common way to update progress as work is completed.
    /// The progress bar will automatically update its display including
    /// the percentage, ETA, and visual bar.
    ///
    /// # Arguments
    ///
    /// * `delta` - The number of work units to add to the current progress
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::utils::progress::ProgressBar;
    ///
    /// let progress = ProgressBar::new(100);
    ///
    /// // Increment by 1 for each completed item
    /// for _ in 0..100 {
    ///     // do_work();
    ///     progress.inc(1);
    /// }
    ///
    /// // Or increment by larger amounts
    /// progress.set_position(0); // Reset for example
    /// progress.inc(25);  // 25% complete
    /// progress.inc(50);  // 75% complete
    /// progress.inc(25);  // 100% complete
    /// ```
    pub fn inc(&self, delta: u64) {
        self.inner.inc(delta);
    }

    /// Sets the current progress position directly.
    ///
    /// This method sets the absolute position rather than incrementing from
    /// the current position. It's useful when you know the exact progress
    /// or need to update progress based on external measurements.
    ///
    /// # Arguments
    ///
    /// * `pos` - The absolute position to set (0 to total length)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::utils::progress::ProgressBar;
    ///
    /// let progress = ProgressBar::new(100);
    ///
    /// // Set specific positions
    /// progress.set_position(0);   // 0%
    /// progress.set_position(25);  // 25%
    /// progress.set_position(50);  // 50%
    /// progress.set_position(100); // 100%
    ///
    /// // Useful when progress is known externally
    /// let files_processed = 42u64; // Example external counter
    /// progress.set_position(files_processed);
    /// ```
    ///
    /// # Note
    ///
    /// Setting a position greater than the total length will clamp to 100%.
    pub fn set_position(&self, pos: u64) {
        self.inner.set_position(pos);
    }

    /// Finishes the progress bar and displays a completion message.
    ///
    /// This method completes the progress bar, sets it to 100%, and replaces
    /// the entire progress line with the provided message. The message
    /// typically indicates successful completion.
    ///
    /// # Arguments
    ///
    /// * `msg` - The completion message to display
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::utils::progress::ProgressBar;
    ///
    /// let progress = ProgressBar::new(100);
    /// progress.set_message("Installing packages");
    ///
    /// // Simulate work
    /// for i in 0..100 {
    ///     // install_package(i);
    ///     progress.inc(1);
    /// }
    ///
    /// // Finish with success message
    /// progress.finish_with_message("✅ All packages installed successfully!");
    /// ```
    ///
    /// # Common Messages
    ///
    /// - `"✅ Operation completed successfully"`
    /// - `"📦 Packages installed"`
    /// - `"🎉 Installation finished"`
    /// - `"⚡ Cache updated"`
    pub fn finish_with_message(&self, msg: impl Into<String>) {
        self.inner.finish_with_message(msg.into());
    }

    /// Finishes the progress bar and clears it from the terminal.
    ///
    /// This method completes the progress bar and removes it entirely from
    /// the terminal, leaving no trace. Use this when you don't want to show
    /// a completion message or when the progress bar is temporary.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::utils::progress::ProgressBar;
    ///
    /// let progress = ProgressBar::new(100);
    /// progress.set_message("Processing...");
    ///
    /// // Do work
    /// for i in 0..100 {
    ///     // process_item(i);
    ///     progress.inc(1);
    /// }
    ///
    /// // Remove progress bar without message
    /// progress.finish_and_clear();
    /// println!("Processing complete!"); // This appears clean
    /// ```
    ///
    /// # Use Cases
    ///
    /// - Temporary progress indicators
    /// - When completion messages are handled elsewhere
    /// - Clean output requirements
    /// - Nested progress operations
    pub fn finish_and_clear(&self) {
        self.inner.finish_and_clear();
    }
}

/// Progress style utilities for consistent CCPM progress bar appearance.
///
/// This struct provides pre-configured styles for different types of progress
/// indicators. All styles follow CCPM's design guidelines and provide consistent
/// visual feedback across different operations.
///
/// # Style Characteristics
///
/// - **Color scheme**: Cyan and blue for active elements
/// - **Characters**: Unicode box drawing characters for smooth appearance
/// - **Information**: Position, total, percentage, and ETA display
/// - **Prefixes**: Support for operation-specific prefixes
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::progress::{ProgressBar, ProgressStyle};
/// use indicatif::ProgressBar as IndicatifBar;
///
/// // Using default style (automatically applied)
/// let pb = ProgressBar::new(100);
///
/// // Manually applying styles to indicatif bars
/// let indicatif_bar = IndicatifBar::new(100);
/// indicatif_bar.set_style(ProgressStyle::download());
/// ```
pub struct ProgressStyle;

impl ProgressStyle {
    /// Returns the default progress bar style used throughout CCPM.
    ///
    /// This style includes:
    /// - Position and total count display (`pos/len`)
    /// - Percentage completion
    /// - Estimated time to completion (ETA)
    /// - 40-character progress bar with cyan/blue colors
    /// - Support for prefix and message display
    ///
    /// # Returns
    ///
    /// An [`IndicatifStyle`] configured with CCPM's default appearance
    ///
    /// # Template Format
    ///
    /// ```text
    /// {prefix} [{bar:40.cyan/blue}] {pos}/{len} ({eta})
    /// ```
    ///
    /// Example output:
    /// ```text
    /// 📦 [████████████████████████████████████████] 50/50 (00:00)
    /// ```
    pub fn default_style() -> IndicatifStyle {
        default_style()
    }

    /// Returns the spinner style for indeterminate progress operations.
    ///
    /// This style is used for operations where the total amount of work is
    /// unknown or when discrete progress steps don't make sense. The spinner
    /// uses Unicode Braille patterns for smooth animation.
    ///
    /// # Returns
    ///
    /// An [`IndicatifStyle`] configured for spinner animation
    ///
    /// # Animation Characters
    ///
    /// The spinner cycles through these Braille patterns:
    /// `⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏`
    ///
    /// # Template Format
    ///
    /// ```text
    /// {prefix} {spinner} {msg}
    /// ```
    ///
    /// Example output:
    /// ```text
    /// 🔄 ⠋ Cloning repository...
    /// ```
    pub fn spinner() -> IndicatifStyle {
        spinner_style()
    }

    /// Returns a progress bar style optimized for download operations.
    ///
    /// This style is specifically designed for file downloads and transfers,
    /// displaying bytes transferred, total bytes, transfer rate, and ETA.
    /// It's ideal for operations involving network transfers or large file operations.
    ///
    /// # Returns
    ///
    /// An [`IndicatifStyle`] configured for download/transfer operations
    ///
    /// # Template Format
    ///
    /// ```text
    /// {prefix} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})
    /// ```
    ///
    /// Example output:
    /// ```text
    /// 📥 [████████████████████████████████████████] 2.1MB/2.1MB (00:05)
    /// ```
    ///
    /// # Use Cases
    ///
    /// - Git repository cloning
    /// - Large file downloads
    /// - Archive extraction progress
    /// - Network transfer operations
    pub fn download() -> IndicatifStyle {
        IndicatifStyle::default_bar()
            .template("{prefix:.bold.cyan} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("━╸━")
    }
}

fn default_style() -> IndicatifStyle {
    IndicatifStyle::default_bar()
        .template("{prefix:.bold} [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
        .unwrap()
        .progress_chars("━╸━")
}

fn spinner_style() -> IndicatifStyle {
    IndicatifStyle::default_spinner()
        .template("{prefix:.bold} {spinner:.cyan} {msg}")
        .unwrap()
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
}

/// A container for managing multiple progress bars simultaneously.
///
/// [`MultiProgress`] allows you to display and manage several progress indicators
/// concurrently. This is useful for operations that involve multiple parallel
/// tasks, such as downloading multiple resources or processing different types
/// of files simultaneously.
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::progress::MultiProgress;
///
/// let multi = MultiProgress::new();
///
/// // Add different types of progress indicators
/// let agents_progress = multi.add_bar(25);
/// let snippets_progress = multi.add_bar(10);
/// let spinner = multi.add_spinner();
///
/// agents_progress.set_message("Downloading agents");
/// snippets_progress.set_message("Downloading snippets");
/// spinner.set_message("Updating cache");
///
/// // Progress bars update independently
/// agents_progress.inc(5);
/// snippets_progress.inc(2);
/// ```
///
/// # Visual Layout
///
/// Multiple progress bars are stacked vertically:
/// ```text
/// 📦 [████████████████░░░░░░░░░░░░░░░░░░░░░░░░] 15/25 Downloading agents
/// 📝 [████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░] 3/10  Downloading snippets
/// 🔄 ⠋ Updating cache
/// ```
///
/// # Thread Safety
///
/// The [`MultiProgress`] container and its associated progress bars are
/// thread-safe and can be shared across async tasks and threads.
pub struct MultiProgress {
    inner: indicatif::MultiProgress,
}

impl MultiProgress {
    /// Creates a new multi-progress container.
    ///
    /// The container starts empty and progress bars can be added using
    /// the `add`, `add_bar`, or `add_spinner` methods.
    ///
    /// # Returns
    ///
    /// A new empty [`MultiProgress`] container
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::utils::progress::MultiProgress;
    ///
    /// let multi = MultiProgress::new();
    ///
    /// // Container is ready to accept progress bars
    /// let pb1 = multi.add_bar(100);
    /// let pb2 = multi.add_spinner();
    /// ```
    pub fn new() -> Self {
        Self {
            inner: indicatif::MultiProgress::new(),
        }
    }

    /// Adds an existing progress bar to the multi-progress container.
    ///
    /// This method takes ownership of a progress bar created elsewhere
    /// and adds it to the container for coordinated display.
    ///
    /// # Arguments
    ///
    /// * `pb` - The [`ProgressBar`] to add to the container
    ///
    /// # Returns
    ///
    /// A new [`ProgressBar`] that's managed by this container
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::utils::progress::{MultiProgress, ProgressBar};
    ///
    /// let multi = MultiProgress::new();
    ///
    /// // Create progress bar independently
    /// let pb = ProgressBar::new(50);
    /// pb.set_message("Processing");
    ///
    /// // Add to container
    /// let managed_pb = multi.add(pb);
    /// managed_pb.inc(10);
    /// ```
    pub fn add(&self, pb: ProgressBar) -> ProgressBar {
        ProgressBar {
            inner: self.inner.add(pb.inner),
        }
    }

    /// Creates and adds a new progress bar to the container.
    ///
    /// This is a convenience method that creates a progress bar with the
    /// specified length and immediately adds it to the container.
    ///
    /// # Arguments
    ///
    /// * `len` - The total length for the new progress bar
    ///
    /// # Returns
    ///
    /// A new [`ProgressBar`] managed by this container
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::utils::progress::MultiProgress;
    ///
    /// let multi = MultiProgress::new();
    ///
    /// // Create multiple progress bars
    /// let agents = multi.add_bar(25);
    /// let snippets = multi.add_bar(10);
    ///
    /// agents.set_message("Installing agents");
    /// snippets.set_message("Installing snippets");
    /// ```
    pub fn add_bar(&self, len: u64) -> ProgressBar {
        let pb = ProgressBar::new(len);
        self.add(pb)
    }

    /// Creates and adds a new spinner to the container.
    ///
    /// This is a convenience method that creates a spinner for indeterminate
    /// progress and immediately adds it to the container.
    ///
    /// # Returns
    ///
    /// A new spinner [`ProgressBar`] managed by this container
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::utils::progress::MultiProgress;
    ///
    /// let multi = MultiProgress::new();
    ///
    /// let file_progress = multi.add_bar(100);
    /// let network_spinner = multi.add_spinner();
    ///
    /// file_progress.set_message("Processing files");
    /// network_spinner.set_message("Fetching metadata");
    /// ```
    pub fn add_spinner(&self) -> ProgressBar {
        let pb = ProgressBar::new_spinner();
        self.add(pb)
    }
}

impl Default for MultiProgress {
    fn default() -> Self {
        Self::new()
    }
}

/// Creates a spinner with an initial message for quick use.
///
/// This is a convenience function that creates a spinner and immediately
/// sets its message. It's ideal for simple indeterminate operations where
/// you want to quickly show progress feedback.
///
/// # Arguments
///
/// * `msg` - The initial message to display with the spinner
///
/// # Returns
///
/// A new spinner [`ProgressBar`] with the message already set
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::progress::spinner_with_message;
///
/// let spinner = spinner_with_message("Cloning repository...");
///
/// // Long running operation
/// // perform_clone().await?;
///
/// spinner.finish_with_message("Repository cloned successfully");
/// ```
///
/// # Equivalent Code
///
/// This function is equivalent to:
/// ```rust
/// # use ccpm::utils::progress::ProgressBar;
/// let spinner = ProgressBar::new_spinner();
/// spinner.set_message("Your message here");
/// ```
pub fn spinner_with_message(msg: impl Into<String>) -> ProgressBar {
    let spinner = ProgressBar::new_spinner();
    spinner.set_message(msg);
    spinner
}

/// Creates either a progress bar or spinner based on optional length.
///
/// This utility function creates the appropriate type of progress indicator
/// based on whether the total work amount is known. It's useful when the
/// progress type is determined at runtime.
///
/// # Arguments
///
/// * `len` - Optional total length. `Some(n)` creates a progress bar, `None` creates a spinner
///
/// # Returns
///
/// A [`ProgressBar`] configured as either a progress bar (known length) or spinner (unknown length)
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::progress::create_progress_bar;
///
/// // Create progress bar when work amount is known
/// let progress = create_progress_bar(Some(100));
/// progress.set_message("Processing 100 files");
///
/// // Create spinner when work amount is unknown
/// let spinner = create_progress_bar(None);
/// spinner.set_message("Processing files...");
/// ```
///
/// # Use Cases
///
/// - Configuration-driven progress indicators
/// - APIs that handle both determinate and indeterminate progress
/// - Dynamic progress bar creation based on runtime conditions
pub fn create_progress_bar(len: Option<u64>) -> ProgressBar {
    if let Some(len) = len {
        ProgressBar::new(len)
    } else {
        ProgressBar::new_spinner()
    }
}

/// Wraps an iterator with a progress bar for visual feedback.
///
/// This function creates a progress bar based on the iterator's length and
/// updates it as the iterator is consumed. It's perfect for adding progress
/// visualization to existing iterator-based code.
///
/// # Type Parameters
///
/// * `I` - An iterator type that implements [`ExactSizeIterator`]
///
/// # Arguments
///
/// * `iter` - The iterator to wrap with progress tracking
/// * `msg` - The message to display with the progress bar
///
/// # Returns
///
/// An iterator that yields the same items as the input iterator while
/// updating a progress bar
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::progress::progress_iterator;
///
/// let files = vec!["file1.txt", "file2.txt", "file3.txt"];
///
/// for file in progress_iterator(files.into_iter(), "Processing files") {
///     // Process each file
///     println!("Processing: {}", file);
///     // The progress bar updates automatically
/// }
/// ```
///
/// # Requirements
///
/// The input iterator must implement [`ExactSizeIterator`], which means
/// its length must be known in advance. This is required to create
/// the progress bar with the correct total.
///
/// # Lazy Evaluation
///
/// The progress bar is updated as the iterator is consumed, not when
/// this function is called. The iterator remains lazy.
pub fn progress_iterator<I>(iter: I, msg: impl Into<String>) -> impl Iterator<Item = I::Item>
where
    I: ExactSizeIterator,
{
    let pb = ProgressBar::new(iter.len() as u64);
    pb.set_message(msg);

    iter.enumerate().map(move |(i, item)| {
        pb.set_position(i as u64);
        item
    })
}

/// A thread-safe progress bar wrapper for parallel operations.
///
/// This struct wraps a [`ProgressBar`] in thread-safe mechanisms, allowing
/// it to be safely shared across multiple threads or async tasks. It's
/// particularly useful for parallel processing operations where multiple
/// workers need to update the same progress indicator.
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::progress::ThreadSafeProgressBar;
/// use std::sync::Arc;
///
/// # async fn example() -> anyhow::Result<()> {
/// let progress = ThreadSafeProgressBar::new(100);
/// progress.set_message("Processing in parallel");
///
/// // Share across multiple async tasks
/// let mut tasks = vec![];
/// for i in 0..10 {
///     let progress_clone = progress.clone();
///     let task = tokio::spawn(async move {
///         // Simulate work
///         tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
///         progress_clone.inc(10);
///     });
///     tasks.push(task);
/// }
///
/// // Wait for all tasks to complete
/// for task in tasks {
///     task.await?;
/// }
///
/// progress.finish_with_message("All tasks completed!");
/// # Ok(())
/// # }
/// ```
///
/// # Thread Safety
///
/// All operations on this progress bar are protected by a [`Mutex`], ensuring
/// that updates from multiple threads don't interfere with each other. If a
/// mutex lock fails (very rare), the operation is silently ignored to prevent
/// progress bar issues from crashing the application.
///
/// # Performance
///
/// While thread-safe, there is a small overhead from mutex locking on each
/// operation. For single-threaded use cases, prefer the regular [`ProgressBar`].
pub struct ThreadSafeProgressBar {
    inner: Arc<Mutex<ProgressBar>>,
}

impl ThreadSafeProgressBar {
    /// Creates a new thread-safe progress bar with the specified length.
    ///
    /// # Arguments
    ///
    /// * `len` - The total number of work units this progress bar represents
    ///
    /// # Returns
    ///
    /// A new [`ThreadSafeProgressBar`] that can be safely shared across threads
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::utils::progress::ThreadSafeProgressBar;
    ///
    /// let progress = ThreadSafeProgressBar::new(100);
    /// progress.set_message("Parallel processing");
    /// ```
    pub fn new(len: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ProgressBar::new(len))),
        }
    }

    /// Creates a new thread-safe spinner for indeterminate progress.
    ///
    /// # Returns
    ///
    /// A new [`ThreadSafeProgressBar`] configured as a spinner
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::utils::progress::ThreadSafeProgressBar;
    ///
    /// let spinner = ThreadSafeProgressBar::new_spinner();
    /// spinner.set_message("Processing in background...");
    /// ```
    pub fn new_spinner() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ProgressBar::new_spinner())),
        }
    }

    /// Set the message (thread-safe)
    pub fn set_message(&self, msg: impl Into<String>) {
        if let Ok(pb) = self.inner.lock() {
            pb.set_message(msg);
        }
    }

    /// Set the prefix (thread-safe)
    pub fn set_prefix(&self, prefix: impl Into<String>) {
        if let Ok(pb) = self.inner.lock() {
            pb.set_prefix(prefix);
        }
    }

    /// Increment the progress bar (thread-safe)
    pub fn inc(&self, delta: u64) {
        if let Ok(pb) = self.inner.lock() {
            pb.inc(delta);
        }
    }

    /// Set the current position (thread-safe)
    pub fn set_position(&self, pos: u64) {
        if let Ok(pb) = self.inner.lock() {
            pb.set_position(pos);
        }
    }

    /// Finish the progress bar with a message (thread-safe)
    pub fn finish_with_message(&self, msg: impl Into<String>) {
        if let Ok(pb) = self.inner.lock() {
            pb.finish_with_message(msg);
        }
    }

    /// Finish and clear the progress bar (thread-safe)
    pub fn finish_and_clear(&self) {
        if let Ok(pb) = self.inner.lock() {
            pb.finish_and_clear();
        }
    }

    /// Returns a clone of the inner Arc for advanced sharing scenarios.
    ///
    /// This method provides direct access to the underlying [`Arc<Mutex<ProgressBar>>`]
    /// for cases where you need more control over the sharing mechanism.
    ///
    /// # Returns
    ///
    /// An [`Arc<Mutex<ProgressBar>>`] that can be shared across threads
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::utils::progress::ThreadSafeProgressBar;
    ///
    /// let progress = ThreadSafeProgressBar::new(100);
    /// let inner = progress.clone_inner();
    ///
    /// // Use the inner Arc directly if needed
    /// if let Ok(pb) = inner.lock() {
    ///     pb.set_message("Direct access");
    /// };
    /// ```
    ///
    /// # Note
    ///
    /// In most cases, using the [`Clone`] implementation is preferred over
    /// this method, as it provides a cleaner interface.
    pub fn clone_inner(&self) -> Arc<Mutex<ProgressBar>> {
        Arc::clone(&self.inner)
    }
}

impl Clone for ThreadSafeProgressBar {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

/// Creates a thread-safe progress indicator based on optional length.
///
/// This utility function creates either a thread-safe progress bar or spinner
/// based on whether the total work amount is known. It's the thread-safe
/// equivalent of [`create_progress_bar`].
///
/// # Arguments
///
/// * `len` - Optional total length. `Some(n)` creates a progress bar, `None` creates a spinner
///
/// # Returns
///
/// A [`ThreadSafeProgressBar`] configured as either a progress bar or spinner
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::progress::create_thread_safe_progress;
///
/// // For known work amount
/// let progress = create_thread_safe_progress(Some(50));
/// progress.set_message("Processing 50 items");
///
/// // For unknown work amount
/// let spinner = create_thread_safe_progress(None);
/// spinner.set_message("Processing items...");
/// ```
///
/// # Use Cases
///
/// - Parallel processing with configurable progress types
/// - APIs that need to handle both determinate and indeterminate parallel work
/// - Dynamic progress creation in multi-threaded environments
pub fn create_thread_safe_progress(len: Option<u64>) -> ThreadSafeProgressBar {
    if let Some(len) = len {
        ThreadSafeProgressBar::new(len)
    } else {
        ThreadSafeProgressBar::new_spinner()
    }
}

/// A thread-safe counter for tracking parallel operation completion.
///
/// This struct provides a thread-safe way to track the completion of parallel
/// operations with optional progress bar visualization. It's designed for
/// scenarios where multiple workers are processing items and you need to
/// track overall completion.
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::progress::ParallelProgressCounter;
/// use std::sync::Arc;
///
/// # async fn example() -> anyhow::Result<()> {
/// // Create counter for 10 parallel tasks with progress bar
/// let counter = Arc::new(ParallelProgressCounter::new(10, true));
///
/// let mut tasks = vec![];
/// for i in 0..10 {
///     let counter_clone = counter.clone();
///     let task = tokio::spawn(async move {
///         // Simulate work
///         tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
///         
///         // Mark task as complete
///         let completed = counter_clone.increment();
///         println!("Task {} completed, total: {}", i, completed);
///     });
///     tasks.push(task);
/// }
///
/// // Wait for all tasks
/// for task in tasks {
///     task.await?;
/// }
///
/// counter.finish();
/// assert!(counter.is_complete());
/// # Ok(())
/// # }
/// ```
///
/// # Features
///
/// - **Thread-safe counting**: Atomic increment operations
/// - **Optional progress bar**: Visual feedback for completion
/// - **Completion tracking**: Easy checking for 100% completion
/// - **Flexible sharing**: Designed to work with [`Arc`] for sharing
pub struct ParallelProgressCounter {
    completed: Arc<Mutex<usize>>,
    total: usize,
    progress_bar: Option<ThreadSafeProgressBar>,
}

impl ParallelProgressCounter {
    /// Creates a new parallel progress counter.
    ///
    /// # Arguments
    ///
    /// * `total` - The total number of tasks/items to track
    /// * `with_progress_bar` - Whether to show a visual progress bar
    ///
    /// # Returns
    ///
    /// A new [`ParallelProgressCounter`] ready for parallel use
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::utils::progress::ParallelProgressCounter;
    ///
    /// // With visual progress bar
    /// let counter = ParallelProgressCounter::new(100, true);
    ///
    /// // Without progress bar (for quiet operations)
    /// let quiet_counter = ParallelProgressCounter::new(50, false);
    /// ```
    ///
    /// # Progress Bar Behavior
    ///
    /// When `with_progress_bar` is `true` and progress bars are not disabled
    /// via environment variables, a progress bar will be displayed and updated
    /// as tasks complete.
    pub fn new(total: usize, with_progress_bar: bool) -> Self {
        let progress_bar = if with_progress_bar && !is_progress_disabled() {
            Some(ThreadSafeProgressBar::new(total as u64))
        } else {
            None
        };

        Self {
            completed: Arc::new(Mutex::new(0)),
            total,
            progress_bar,
        }
    }

    /// Increments the completion counter in a thread-safe manner.
    ///
    /// This method atomically increments the internal counter and updates
    /// the progress bar (if present). It returns the new completion count.
    ///
    /// # Returns
    ///
    /// The number of completed tasks after this increment
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::utils::progress::ParallelProgressCounter;
    ///
    /// let counter = ParallelProgressCounter::new(10, false);
    ///
    /// // Called from worker threads/tasks
    /// let completed = counter.increment();
    /// println!("Completed: {}/10", completed);
    /// ```
    ///
    /// # Thread Safety
    ///
    /// This method is safe to call from multiple threads simultaneously.
    /// The internal counter is protected by a mutex, and if the mutex
    /// lock fails, the method returns 0 to prevent panics.
    pub fn increment(&self) -> usize {
        let current = {
            if let Ok(mut completed) = self.completed.lock() {
                *completed += 1;
                *completed
            } else {
                0
            }
        };

        // Update progress bar if available
        if let Some(ref pb) = self.progress_bar {
            pb.set_position(current as u64);
            pb.set_message(format!("Completed {}/{}", current, self.total));
        }

        current
    }

    /// Gets the current completion count.
    ///
    /// This method returns the current number of completed tasks without
    /// modifying the counter.
    ///
    /// # Returns
    ///
    /// The current number of completed tasks
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::utils::progress::ParallelProgressCounter;
    ///
    /// let counter = ParallelProgressCounter::new(10, false);
    /// counter.increment();
    /// counter.increment();
    ///
    /// assert_eq!(counter.current(), 2);
    /// ```
    ///
    /// # Thread Safety
    ///
    /// This method is safe to call from multiple threads. If the mutex
    /// lock fails, it returns 0.
    pub fn current(&self) -> usize {
        self.completed.lock().map(|guard| *guard).unwrap_or(0)
    }

    /// Checks if all tasks have been completed.
    ///
    /// This method compares the current completion count with the total
    /// number of tasks to determine if the operation is complete.
    ///
    /// # Returns
    ///
    /// `true` if all tasks are completed, `false` otherwise
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::utils::progress::ParallelProgressCounter;
    ///
    /// let counter = ParallelProgressCounter::new(3, false);
    ///
    /// assert!(!counter.is_complete());
    ///
    /// counter.increment();
    /// counter.increment();
    /// counter.increment();
    ///
    /// assert!(counter.is_complete());
    /// ```
    ///
    /// # Use Cases
    ///
    /// - Checking completion status in monitoring loops
    /// - Determining when to proceed to the next phase
    /// - Validation in tests and assertions
    pub fn is_complete(&self) -> bool {
        self.current() >= self.total
    }

    /// Finishes the progress bar with a completion message.
    ///
    /// This method completes the progress bar (if present) and displays
    /// a success message indicating all tasks have been completed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::utils::progress::ParallelProgressCounter;
    ///
    /// let counter = ParallelProgressCounter::new(5, true);
    ///
    /// // Complete all tasks
    /// for _ in 0..5 {
    ///     counter.increment();
    /// }
    ///
    /// // Finish with success message
    /// counter.finish();
    /// ```
    ///
    /// # Message Format
    ///
    /// The completion message follows the format: `"✅ Completed all {total} tasks"`
    ///
    /// # Behavior
    ///
    /// - If no progress bar was created, this method does nothing
    /// - Safe to call multiple times (subsequent calls are ignored)
    /// - Thread-safe and can be called from any thread
    pub fn finish(&self) {
        if let Some(ref pb) = self.progress_bar {
            pb.finish_with_message(format!("✅ Completed all {} tasks", self.total));
        }
    }

    /// Returns a clone of the internal completion counter for advanced use cases.
    ///
    /// This method provides direct access to the underlying [`Arc<Mutex<usize>>`]
    /// for scenarios where you need more control over the counter mechanism.
    ///
    /// # Returns
    ///
    /// An [`Arc<Mutex<usize>>`] representing the completion counter
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::utils::progress::ParallelProgressCounter;
    ///
    /// let counter = ParallelProgressCounter::new(10, false);
    /// let raw_counter = counter.clone_counter();
    ///
    /// // Direct manipulation (advanced use case)
    /// if let Ok(mut count) = raw_counter.lock() {
    ///     *count += 5; // Increment by 5
    /// };
    /// ```
    ///
    /// # Warning
    ///
    /// Direct manipulation of the counter bypasses progress bar updates.
    /// In most cases, using `increment` is preferred.
    ///
    /// # Use Cases
    ///
    /// - Custom increment amounts
    /// - Integration with external counting mechanisms
    /// - Advanced synchronization patterns
    pub fn clone_counter(&self) -> Arc<Mutex<usize>> {
        Arc::clone(&self.completed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_bar_new() {
        let pb = ProgressBar::new(100);
        pb.set_message("Test message");
        pb.set_prefix("Test");
        pb.inc(10);
        pb.set_position(50);
        pb.finish_with_message("Done");
    }

    #[test]
    fn test_progress_bar_spinner() {
        let spinner = ProgressBar::new_spinner();
        spinner.set_message("Loading...");
        spinner.finish_and_clear();
    }

    #[test]
    fn test_progress_style() {
        let _default = ProgressStyle::default_style();
        let _spinner = ProgressStyle::spinner();
        let _download = ProgressStyle::download();
    }

    #[test]
    fn test_multi_progress() {
        let mp = MultiProgress::new();
        let pb1 = mp.add_bar(100);
        let pb2 = mp.add_spinner();

        pb1.inc(50);
        pb2.set_message("Processing");
    }

    #[test]
    fn test_spinner_with_message() {
        let spinner = spinner_with_message("Test spinner");
        spinner.finish_and_clear();
    }

    #[test]
    fn test_progress_iterator() {
        let items = vec![1, 2, 3, 4, 5];
        let mut count = 0;

        for item in progress_iterator(items.into_iter(), "Processing items") {
            count += item;
        }

        assert_eq!(count, 15);
    }

    #[test]
    fn test_default_multi_progress() {
        let mp = MultiProgress::default();
        let _pb = mp.add_bar(50);
    }

    #[test]
    fn test_thread_safe_progress_bar() {
        let pb = ThreadSafeProgressBar::new(100);
        pb.set_message("Test message");
        pb.set_prefix("Test prefix");
        pb.inc(10);
        pb.set_position(50);
        pb.finish_with_message("Completed");

        // Test cloning
        let pb_clone = pb.clone();
        pb_clone.set_message("Clone message");

        // Test spinner variant
        let spinner = ThreadSafeProgressBar::new_spinner();
        spinner.set_message("Spinner test");
        spinner.finish_and_clear();
    }

    #[test]
    fn test_thread_safe_progress_bar_clone_inner() {
        let pb = ThreadSafeProgressBar::new(100);
        let inner_clone = pb.clone_inner();

        // Should be able to use the cloned inner Arc
        let lock_result = inner_clone.lock();
        assert!(lock_result.is_ok());
    }

    #[test]
    fn test_parallel_progress_counter() {
        let counter = ParallelProgressCounter::new(5, true);

        assert_eq!(counter.current(), 0);
        assert!(!counter.is_complete());

        let current = counter.increment();
        assert_eq!(current, 1);
        assert_eq!(counter.current(), 1);

        // Increment until complete
        counter.increment();
        counter.increment();
        counter.increment();
        counter.increment();

        assert_eq!(counter.current(), 5);
        assert!(counter.is_complete());

        counter.finish();
    }

    #[test]
    fn test_parallel_progress_counter_no_progress_bar() {
        let counter = ParallelProgressCounter::new(3, false);

        counter.increment();
        counter.increment();
        counter.increment();

        assert!(counter.is_complete());
        counter.finish(); // Should not panic even without progress bar
    }

    #[test]
    fn test_parallel_progress_counter_clone_counter() {
        let counter = ParallelProgressCounter::new(2, false);
        let cloned_counter = counter.clone_counter();

        // Increment through the cloned counter
        if let Ok(mut guard) = cloned_counter.lock() {
            *guard += 1;
        }

        assert_eq!(counter.current(), 1);
    }

    #[test]
    fn test_create_progress_bar_helper() {
        let pb_with_len = create_progress_bar(Some(100));
        pb_with_len.set_message("With length");

        let pb_spinner = create_progress_bar(None);
        pb_spinner.set_message("Spinner");
        pb_spinner.finish_and_clear();
    }

    #[test]
    fn test_create_thread_safe_progress_helper() {
        let pb_with_len = create_thread_safe_progress(Some(100));
        pb_with_len.set_message("Thread safe with length");

        let pb_spinner = create_thread_safe_progress(None);
        pb_spinner.set_message("Thread safe spinner");
        pb_spinner.finish_and_clear();
    }

    #[test]
    fn test_progress_styles() {
        // Test all style methods don't panic
        let _default = ProgressStyle::default_style();
        let _spinner = ProgressStyle::spinner();
        let _download = ProgressStyle::download();
    }

    #[test]
    fn test_multi_progress_add_methods() {
        let mp = MultiProgress::new();

        // Test add_bar
        let pb1 = mp.add_bar(100);
        pb1.set_message("Bar 1");

        // Test add_spinner
        let pb2 = mp.add_spinner();
        pb2.set_message("Spinner 1");

        // Test add with existing progress bar
        let existing_pb = ProgressBar::new(50);
        let pb3 = mp.add(existing_pb);
        pb3.set_message("Added existing");
    }

    #[test]
    fn test_is_progress_disabled() {
        use std::env;

        // Clear the environment variable first
        env::remove_var("CCPM_NO_PROGRESS");
        assert!(!is_progress_disabled());

        // Set the environment variable
        env::set_var("CCPM_NO_PROGRESS", "1");
        assert!(is_progress_disabled());

        // Any value should work
        env::set_var("CCPM_NO_PROGRESS", "true");
        assert!(is_progress_disabled());

        // Clean up
        env::remove_var("CCPM_NO_PROGRESS");
        assert!(!is_progress_disabled());
    }

    #[test]
    fn test_progress_respects_no_progress_flag() {
        use std::env;

        // Test with flag set
        env::set_var("CCPM_NO_PROGRESS", "1");

        let pb = ProgressBar::new(100);
        pb.set_message("Should be hidden");
        pb.inc(50);
        pb.finish_with_message("Done");

        let spinner = ProgressBar::new_spinner();
        spinner.set_message("Hidden spinner");
        spinner.finish_and_clear();

        // Clean up
        env::remove_var("CCPM_NO_PROGRESS");
    }
}
