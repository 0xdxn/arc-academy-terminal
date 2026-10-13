//! Arc Academy Terminal - TUI Interface
//!
//! Beautiful terminal UI using ratatui

pub mod app;
pub mod ui;
pub mod events;
pub mod panels;
pub mod theme;
pub mod shell;
pub mod ansi;
pub mod persistence;
pub mod autocomplete;
pub mod analytics;
pub mod icons;
pub mod footer;
pub mod level;
pub mod celebrate;
pub mod streak;
pub mod certificate;
pub mod sandbox;

pub use app::App;
pub use theme::Theme;

use anyhow::Result;
use std::path::PathBuf;

/// CLI overrides for launching the TUI
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// Override the config file path (`arct --config <path>`)
    pub config_path: Option<PathBuf>,

    /// Override the theme by name (`arct start --theme <name>`)
    pub theme: Option<String>,
}

/// Resolve a theme by name, case-insensitively (`-` and `_` are treated as
/// spaces, so `arc-dark` matches "Arc Dark"). Returns an error listing the
/// valid themes on bad input.
pub fn resolve_theme(name: &str) -> Result<Theme> {
    let normalized = name.trim().to_lowercase().replace(['-', '_'], " ");
    Theme::all()
        .into_iter()
        .find(|t| t.name.to_lowercase() == normalized)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Unknown theme '{}'. Valid themes: {}",
                name,
                Theme::all_names().join(", ")
            )
        })
}

/// Run the TUI application
pub async fn run() -> Result<()> {
    run_with_options(RunOptions::default()).await
}

/// Run the TUI application with CLI overrides
pub async fn run_with_options(options: RunOptions) -> Result<()> {
    let mut app = App::new_with_options(options)?;
    app.run().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_theme_case_insensitive() {
        assert_eq!(resolve_theme("arc dark").unwrap().name, "Arc Dark");
        assert_eq!(resolve_theme("ARC LIGHT").unwrap().name, "Arc Light");
        assert_eq!(
            resolve_theme("Arc Academy Orange").unwrap().name,
            "Arc Academy Orange"
        );
        assert_eq!(
            resolve_theme("arc-academy-green").unwrap().name,
            "Arc Academy Green"
        );
        assert_eq!(resolve_theme("arc_dark").unwrap().name, "Arc Dark");
        assert_eq!(resolve_theme("night").unwrap().name, "Night");
        assert_eq!(resolve_theme("MOCHA").unwrap().name, "Mocha");
    }

    #[test]
    fn test_resolve_theme_accepts_every_registered_theme() {
        // The `--theme` CLI flag must accept every theme in the registry,
        // including hyphenated lowercase forms
        for name in Theme::all_names() {
            assert_eq!(resolve_theme(&name).unwrap().name, name);
            let hyphenated = name.to_lowercase().replace(' ', "-");
            assert_eq!(resolve_theme(&hyphenated).unwrap().name, name);
        }
    }

    #[test]
    fn test_resolve_theme_bad_input_lists_valid_themes() {
        let err = resolve_theme("solarized").unwrap_err().to_string();
        assert!(err.contains("solarized"));
        // The CLI error message must list every registered theme
        for name in Theme::all_names() {
            assert!(err.contains(&name), "CLI theme list is missing '{}'", name);
        }
    }
}
