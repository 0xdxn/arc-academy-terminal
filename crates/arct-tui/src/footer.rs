//! Persistent footer hint bar
//!
//! A one-line, always-visible strip at the bottom of the screen showing the
//! keys that matter *right now*. This is the primary discoverability surface
//! for novice users: whatever mode or overlay is active, the footer tells
//! them how to drive it (and how to get out).

use crate::theme::Theme;
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

/// Which set of hints the footer should show, derived from app state.
/// Ordered roughly by "what captures input first".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FooterMode {
    /// A notification popup is showing
    Notification,
    /// The command palette (Ctrl+K) is open
    Palette,
    /// The settings panel is open
    Settings,
    /// The lesson selection menu is open
    LessonMenu,
    /// A scrollable overlay is open (help / achievements / progress / challenges)
    Overlay,
    /// Lesson mode is active (working through lesson steps)
    Lesson,
    /// AI assistant input mode
    Ai,
    /// The welcome dashboard is showing
    Dashboard,
    /// Regular shell mode
    Normal,
}

/// The (key, label) hint chips for a given footer mode.
/// Pure function so the per-mode content is unit-testable.
pub fn hints(mode: FooterMode) -> Vec<(&'static str, &'static str)> {
    match mode {
        FooterMode::Normal => vec![
            ("Ctrl+L", "lessons"),
            ("Ctrl+U/D", "scroll"),
            ("Ctrl+F", "side panel"),
            ("Ctrl+K", "all actions"),
            ("?", "help"),
        ],
        FooterMode::Dashboard => vec![
            ("Ctrl+L", "start learning"),
            ("Ctrl+K", "palette"),
            ("any key", "dismiss"),
        ],
        FooterMode::Lesson => vec![
            ("Enter", "run"),
            ("Ctrl+B", "back a step"),
            ("Ctrl+L", "lesson map"),
            ("Ctrl+U/D", "scroll"),
            ("Ctrl+K", "more"),
            ("?", "help"),
        ],
        FooterMode::LessonMenu => vec![
            ("↑↓", "select"),
            ("Enter", "start"),
            ("1-9", "jump"),
            ("Esc", "close"),
            ("Ctrl+L", "leave lessons"),
        ],
        FooterMode::Overlay => vec![("↑↓", "scroll"), ("Esc", "close")],
        FooterMode::Palette => vec![
            ("type", "filter"),
            ("↑↓", "select"),
            ("Enter", "run"),
            ("Esc", "close"),
        ],
        FooterMode::Settings => vec![
            ("↑↓", "field"),
            ("Enter", "edit"),
            ("Esc", "close"),
        ],
        FooterMode::Ai => vec![
            ("Enter", "ask"),
            ("Ctrl+A", "shell"),
            ("Esc", "exit ai"),
        ],
        FooterMode::Notification => vec![("Enter/Esc", "dismiss")],
    }
}

/// Render the footer hint bar into `area` (expected to be 1 row tall)
pub fn render(frame: &mut Frame, area: Rect, theme: &Theme, mode: FooterMode) {
    if area.height == 0 {
        return;
    }

    let mut spans: Vec<Span> = vec![Span::styled(" ", theme.style_footer())];
    let chips = hints(mode);
    for (i, (key, label)) in chips.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", theme.style_footer()));
        }
        spans.push(Span::styled(*key, theme.style_footer_key()));
        spans.push(Span::styled(" ", theme.style_footer()));
        spans.push(Span::styled(*label, theme.style_footer()));
    }

    let bar = Paragraph::new(Line::from(spans)).style(theme.style_footer());
    frame.render_widget(bar, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(mode: FooterMode) -> Vec<&'static str> {
        hints(mode).into_iter().map(|(k, _)| k).collect()
    }

    #[test]
    fn test_normal_mode_shows_discovery_keys() {
        let keys = keys(FooterMode::Normal);
        assert!(keys.contains(&"Ctrl+K"));
        assert!(keys.contains(&"Ctrl+L"));
        assert!(keys.contains(&"Ctrl+U/D"));
        assert!(keys.contains(&"?"));
    }

    #[test]
    fn test_lesson_mode_shows_lesson_keys() {
        let keys = keys(FooterMode::Lesson);
        assert!(keys.contains(&"Enter"));
        assert!(keys.contains(&"Ctrl+B"));
        assert!(keys.contains(&"Ctrl+L"));
    }

    #[test]
    fn test_footer_keys_dont_rely_on_alt() {
        // Option/Alt types a character in most Mac terminals
        for mode in [
            FooterMode::Normal,
            FooterMode::Dashboard,
            FooterMode::Lesson,
            FooterMode::LessonMenu,
            FooterMode::Overlay,
            FooterMode::Palette,
            FooterMode::Settings,
            FooterMode::Ai,
            FooterMode::Notification,
        ] {
            assert!(!keys(mode).iter().any(|k| k.contains("Alt")), "{mode:?}");
        }
    }

    #[test]
    fn test_overlay_mode_shows_scroll_and_close() {
        let hints = hints(FooterMode::Overlay);
        assert_eq!(hints, vec![("↑↓", "scroll"), ("Esc", "close")]);
    }

    #[test]
    fn test_every_transient_mode_offers_escape() {
        // Any mode that captures input must show the user a way out
        for mode in [
            FooterMode::Palette,
            FooterMode::Settings,
            FooterMode::LessonMenu,
            FooterMode::Overlay,
        ] {
            assert!(
                hints(mode).iter().any(|(k, _)| k.contains("Esc")),
                "{:?} footer must include an Esc hint",
                mode
            );
        }
        assert!(hints(FooterMode::Notification)
            .iter()
            .any(|(k, _)| k.contains("Esc")));
    }

    #[test]
    fn test_no_mode_has_empty_hints() {
        for mode in [
            FooterMode::Normal,
            FooterMode::Dashboard,
            FooterMode::Lesson,
            FooterMode::LessonMenu,
            FooterMode::Overlay,
            FooterMode::Palette,
            FooterMode::Settings,
            FooterMode::Ai,
            FooterMode::Notification,
        ] {
            assert!(!hints(mode).is_empty(), "{:?} has no hints", mode);
        }
    }
}
