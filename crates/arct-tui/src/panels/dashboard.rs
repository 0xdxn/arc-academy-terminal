//! Welcome dashboard shown on launch
//!
//! Instead of dropping novices into a bare shell, the output area opens with
//! a home view: wordmark, streak, level/XP, lesson progress, today's
//! challenge, and 2-3 big obvious key hints. It disappears on the first
//! keypress — returning users see their streak first.

use crate::level;
use crate::theme::Theme;
use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// Compact ASCII wordmark (fits terminals ≥ 44 columns)
const WORDMARK: [&str; 4] = [
    r"  ▄▀█ █▀█ █▀▀   ▄▀█ █▀▀ ▄▀█ █▀▄ █▀▀ █▀▄▀█ █▄█",
    r"  █▀█ █▀▄ █▄▄   █▀█ █▄▄ █▀█ █▄▀ ██▄ █ ▀ █  █ ",
    r"  ─────────────────────────────────────────────",
    r"        learn the terminal, one step at a time",
];

/// Data snapshot the dashboard renders (collected by the caller so this
/// module stays a pure view)
pub struct DashboardData {
    pub user_name: Option<String>,
    pub streak: arct_core::StreakStatus,
    pub level: level::LevelInfo,
    pub lessons_completed: usize,
    pub lessons_total: usize,
    pub daily_challenge: Option<String>,
    pub next_lesson: Option<String>,
}

/// Build a text progress bar like `▰▰▰▱▱▱▱▱▱▱`
fn bar(ratio: f64, width: usize) -> (String, String) {
    let filled = ((ratio.clamp(0.0, 1.0)) * width as f64).round() as usize;
    let filled = filled.min(width);
    ("▰".repeat(filled), "▱".repeat(width - filled))
}

/// Render the dashboard into the output-panel area
pub fn render(frame: &mut Frame, area: Rect, theme: &Theme, data: &DashboardData) {
    let block = Block::default()
        .title(" ⌂ Welcome ")
        .title_style(theme.style_title(true))
        .borders(Borders::ALL)
        .border_style(theme.style_border_focused())
        .style(theme.style_block());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    // Wordmark (skip decorative lines on very short areas)
    if inner.height >= 12 {
        for (i, row) in WORDMARK.iter().enumerate() {
            let style = if i < 2 {
                theme.style_accent()
            } else {
                theme.style_dim()
            };
            lines.push(Line::from(Span::styled(*row, style)));
        }
        lines.push(Line::from(""));
    }

    // Greeting + learning days
    let greeting = match &data.user_name {
        Some(name) => format!("  Welcome back, {}!", name),
        None => "  Welcome back!".to_string(),
    };
    lines.push(Line::from(vec![
        Span::styled(greeting, theme.style_normal().add_modifier(Modifier::BOLD)),
        Span::raw("   "),
        Span::styled(crate::streak::describe(data.streak), theme.style_warning()),
    ]));
    lines.push(Line::from(""));

    // Level / XP bar
    let (filled, empty) = bar(data.level.progress(), 20);
    lines.push(Line::from(vec![
        Span::styled(format!("  Level {:<2} ", data.level.level), theme.style_accent()),
        Span::styled(filled, theme.style_accent()),
        Span::styled(empty, theme.style_dim()),
        Span::styled(
            format!(" {}/{} XP", data.level.xp_into_level, data.level.xp_for_next),
            theme.style_dim(),
        ),
    ]));

    // Lessons completed
    lines.push(Line::from(vec![
        Span::styled("  Lessons  ", theme.style_info()),
        Span::styled(
            format!("{}/{} completed", data.lessons_completed, data.lessons_total),
            theme.style_normal(),
        ),
    ]));

    // Today's challenge
    if let Some(challenge) = &data.daily_challenge {
        lines.push(Line::from(vec![
            Span::styled("  Today    ", theme.style_success()),
            Span::styled(challenge.clone(), theme.style_normal()),
        ]));
    }

    // Recommended next lesson
    if let Some(next) = &data.next_lesson {
        lines.push(Line::from(vec![
            Span::styled("  Next up  ", theme.style_warning()),
            Span::styled(next.clone(), theme.style_normal().add_modifier(Modifier::BOLD)),
        ]));
    }

    lines.push(Line::from(""));

    // Big obvious key hints
    lines.push(Line::from(vec![
        Span::raw("  Press "),
        Span::styled("Ctrl+L", theme.style_accent().add_modifier(Modifier::BOLD)),
        Span::raw(" to start learning"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("  Press "),
        Span::styled("Ctrl+K", theme.style_accent().add_modifier(Modifier::BOLD)),
        Span::raw(" to see everything you can do"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("  Or just "),
        Span::styled("start typing a command", theme.style_success()),
        Span::raw(" — this screen gets out of your way"),
    ]));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bar_is_always_requested_width() {
        for ratio in [-0.5, 0.0, 0.33, 0.5, 1.0, 2.0] {
            let (filled, empty) = bar(ratio, 20);
            assert_eq!(filled.chars().count() + empty.chars().count(), 20);
        }
    }

    #[test]
    fn test_bar_extremes() {
        let (filled, empty) = bar(0.0, 10);
        assert!(filled.is_empty());
        assert_eq!(empty.chars().count(), 10);

        let (filled, empty) = bar(1.0, 10);
        assert_eq!(filled.chars().count(), 10);
        assert!(empty.is_empty());
    }
}
