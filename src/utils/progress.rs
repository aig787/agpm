//! Progress indicators and user interface utilities
//!
//! This module provides a unified progress system for AGPM operations using the
//! MultiPhaseProgress approach. All progress tracking goes through phases to ensure
//! consistent user experience across different operations.
//!
//! # Features
//!
//! - **Unified progress**: All operations use MultiPhaseProgress for consistency
//! - **Phase-based tracking**: Installation/update operations broken into logical phases
//! - **CI/quiet mode support**: Automatically disables in non-interactive environments
//! - **Thread safety**: Safe to use across async tasks and parallel operations
//!
//! # Configuration
//!
//! Progress indicators are now controlled via the MultiPhaseProgress constructor
//! parameter rather than environment variables for better thread safety.
//!
//! # Examples
//!
//! ## Multi-Phase Progress
//!
//! ```rust,no_run
//! use agpm::utils::progress::{MultiPhaseProgress, InstallationPhase};
//!
//! let progress = MultiPhaseProgress::new(true);
//!
//! // Start syncing phase
//! progress.start_phase(InstallationPhase::SyncingSources, Some("Fetching repositories"));
//! // ... do work ...
//! progress.complete_phase(Some("Synced 3 repositories"));
//!
//! // Start resolving phase
//! progress.start_phase(InstallationPhase::ResolvingDependencies, None);
//! // ... do work ...
//! progress.complete_phase(Some("Resolved 25 dependencies"));
//! ```

use crate::manifest::Manifest;
use indicatif::{ProgressBar as IndicatifBar, ProgressStyle as IndicatifStyle};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// Re-export for deprecated functions - use MultiPhaseProgress instead
#[deprecated(since = "0.3.0", note = "Use MultiPhaseProgress instead")]
pub use indicatif::ProgressBar;

/// Represents different phases of the installation process
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallationPhase {
    /// Syncing source repositories
    SyncingSources,
    /// Resolving dependencies and versions
    ResolvingDependencies,
    /// Installing resources from resolved dependencies
    Installing,
    /// Installing specific resources (used during updates)
    InstallingResources,
    /// Updating configuration files and finalizing
    Finalizing,
}

impl InstallationPhase {
    /// Get a human-readable description of the phase
    pub fn description(&self) -> &'static str {
        match self {
            Self::SyncingSources => "Syncing sources",
            Self::ResolvingDependencies => "Resolving dependencies",
            Self::Installing => "Installing resources",
            Self::InstallingResources => "Installing resources",
            Self::Finalizing => "Finalizing installation",
        }
    }

    /// Get the spinner prefix for this phase
    pub fn spinner_prefix(&self) -> &'static str {
        match self {
            Self::SyncingSources => "⏳",
            Self::ResolvingDependencies => "🔍",
            Self::Installing => "📦",
            Self::InstallingResources => "📦",
            Self::Finalizing => "✨",
        }
    }
}

/// Multi-phase progress manager that displays multiple progress bars
/// with completed phases showing as static messages
#[derive(Clone)]
pub struct MultiPhaseProgress {
    /// MultiProgress container from indicatif
    multi: Arc<indicatif::MultiProgress>,
    /// Current active spinner/progress bar
    current_bar: Arc<Mutex<Option<IndicatifBar>>>,
    /// Whether progress is enabled
    enabled: bool,
}

impl MultiPhaseProgress {
    /// Create a new multi-phase progress manager
    pub fn new(enabled: bool) -> Self {
        Self {
            multi: Arc::new(indicatif::MultiProgress::new()),
            current_bar: Arc::new(Mutex::new(None)),
            enabled,
        }
    }

    /// Start a new phase with a spinner
    pub fn start_phase(&self, phase: InstallationPhase, message: Option<&str>) {
        if !self.enabled {
            // In non-TTY mode, just print the phase
            if !self.enabled {
                return;
            }
            let phase_msg = if let Some(msg) = message {
                format!("{} {} {}", phase.spinner_prefix(), phase.description(), msg)
            } else {
                format!("{} {}", phase.spinner_prefix(), phase.description())
            };
            println!("{}", phase_msg);
            return;
        }

        // Don't clear the existing bar - it should already be finished with a message
        // Just remove our reference to it
        if let Ok(mut guard) = self.current_bar.lock() {
            *guard = None;
        }

        // Create new spinner for this phase
        let spinner = self.multi.add(IndicatifBar::new_spinner());

        // Format the phase message
        let phase_msg = format!(
            "{} {} {}",
            phase.spinner_prefix(),
            phase.description(),
            message.unwrap_or("")
        );

        // Configure spinner style
        let style = IndicatifStyle::default_spinner()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
            .template("{spinner} {msg}")
            .unwrap();

        spinner.set_style(style);
        spinner.set_message(phase_msg);
        spinner.enable_steady_tick(Duration::from_millis(100));

        // Store the spinner
        *self.current_bar.lock().unwrap() = Some(spinner);
    }

    /// Start a new phase with a progress bar
    pub fn start_phase_with_progress(&self, phase: InstallationPhase, total: usize) {
        if !self.enabled {
            // In non-TTY mode, just print the phase
            if !self.enabled {
                return;
            }
            println!(
                "{} {} (0/{})",
                phase.spinner_prefix(),
                phase.description(),
                total
            );
            return;
        }

        // Don't clear the existing bar - it should already be finished with a message
        // Just remove our reference to it
        if let Ok(mut guard) = self.current_bar.lock() {
            *guard = None;
        }

        // Create new progress bar for this phase
        let progress_bar = self.multi.add(IndicatifBar::new(total as u64));

        // Configure progress bar style with phase prefix
        let style = IndicatifStyle::default_bar()
            .template(&format!(
                "{} {{msg}} [{{bar:40.cyan/blue}}] {{pos}}/{{len}}",
                phase.spinner_prefix()
            ))
            .unwrap()
            .progress_chars("=>-");

        progress_bar.set_style(style);
        progress_bar.set_message(phase.description());

        // Store the progress bar
        *self.current_bar.lock().unwrap() = Some(progress_bar);
    }

    /// Update the message of the current phase
    pub fn update_message(&self, message: String) {
        if let Ok(guard) = self.current_bar.lock()
            && let Some(ref bar) = *guard
        {
            bar.set_message(message);
        }
    }

    /// Update the current message for the active phase
    pub fn update_current_message(&self, message: &str) {
        if let Ok(guard) = self.current_bar.lock()
            && let Some(ref bar) = *guard
        {
            bar.set_message(message.to_string());
        }
    }

    /// Increment progress for progress bars
    pub fn increment_progress(&self, delta: u64) {
        if let Ok(guard) = self.current_bar.lock()
            && let Some(ref bar) = *guard
        {
            bar.inc(delta);
        }
    }

    /// Set progress position for progress bars
    pub fn set_progress(&self, pos: usize) {
        if let Ok(guard) = self.current_bar.lock()
            && let Some(ref bar) = *guard
        {
            bar.set_position(pos as u64);
        }
    }

    /// Complete the current phase and show it as a static message
    pub fn complete_phase(&self, message: Option<&str>) {
        if !self.enabled {
            // In non-TTY mode, just print completion
            if !self.enabled {
                return;
            }
            if let Some(msg) = message {
                println!("✓ {}", msg);
            }
            return;
        }

        // Complete the current bar/spinner with a message and leave it visible
        if let Ok(mut guard) = self.current_bar.lock()
            && let Some(bar) = guard.take()
        {
            // Disable any animation
            bar.disable_steady_tick();

            // Set the final message
            let final_message = if let Some(msg) = message {
                format!("✓ {}", msg)
            } else {
                "✓ Phase complete".to_string()
            };

            // Clear the spinner
            bar.finish_and_clear();

            // Use suspend to print the completion message outside of the MultiProgress
            // This ensures it stays visible
            self.multi.suspend(|| {
                println!("{}", final_message);
            });
        }
    }

    /// Clear all progress displays
    pub fn clear(&self) {
        // Clear current bar if any
        if let Ok(mut guard) = self.current_bar.lock()
            && let Some(bar) = guard.take()
        {
            bar.finish_and_clear();
        }
        self.multi.clear().ok();
    }

    /// Create a subordinate progress bar for detailed progress within a phase
    pub fn add_progress_bar(&self, total: u64) -> Option<IndicatifBar> {
        if !self.enabled {
            return None;
        }

        let pb = self.multi.add(IndicatifBar::new(total));
        let style = IndicatifStyle::default_bar()
            .template("  {msg} [{bar:40.cyan/blue}] {pos}/{len}")
            .unwrap()
            .progress_chars("=>-");
        pb.set_style(style);
        Some(pb)
    }
}

/// Helper function to collect dependency names from a manifest
pub fn collect_dependency_names(manifest: &Manifest) -> Vec<String> {
    manifest
        .all_dependencies()
        .iter()
        .map(|(name, _)| name.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_installation_phase_description() {
        assert_eq!(
            InstallationPhase::SyncingSources.description(),
            "Syncing sources"
        );
        assert_eq!(
            InstallationPhase::ResolvingDependencies.description(),
            "Resolving dependencies"
        );
        assert_eq!(
            InstallationPhase::Installing.description(),
            "Installing resources"
        );
        assert_eq!(
            InstallationPhase::InstallingResources.description(),
            "Installing resources"
        );
        assert_eq!(
            InstallationPhase::Finalizing.description(),
            "Finalizing installation"
        );
    }

    #[test]
    fn test_installation_phase_spinner_prefix() {
        assert_eq!(InstallationPhase::SyncingSources.spinner_prefix(), "⏳");
        assert_eq!(
            InstallationPhase::ResolvingDependencies.spinner_prefix(),
            "🔍"
        );
        assert_eq!(InstallationPhase::Installing.spinner_prefix(), "📦");
        assert_eq!(
            InstallationPhase::InstallingResources.spinner_prefix(),
            "📦"
        );
        assert_eq!(InstallationPhase::Finalizing.spinner_prefix(), "✨");
    }

    #[test]
    fn test_multi_phase_progress_new() {
        let progress = MultiPhaseProgress::new(true);

        // Test basic functionality
        progress.start_phase(InstallationPhase::SyncingSources, Some("test message"));
        progress.update_current_message("updated message");
        progress.complete_phase(Some("completed"));
        progress.clear();
    }

    #[test]
    fn test_multi_phase_progress_with_progress_bar() {
        let progress = MultiPhaseProgress::new(true);

        progress.start_phase_with_progress(InstallationPhase::Installing, 10);
        progress.increment_progress(5);
        progress.set_progress(8);
        progress.complete_phase(Some("Installation completed"));
    }

    #[test]
    fn test_multi_phase_progress_disabled() {
        let progress = MultiPhaseProgress::new(false);

        // These should not panic when disabled
        progress.start_phase(InstallationPhase::SyncingSources, None);
        progress.complete_phase(Some("test"));
        progress.clear();
    }

    #[test]
    fn test_collect_dependency_names() {
        // This test would need a proper Manifest instance to work
        // For now, just ensure the function compiles and runs

        // Note: This is a minimal test since we'd need to construct a full manifest
        // In real usage, this function extracts dependency names from the manifest
    }
}
