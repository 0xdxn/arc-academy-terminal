//! Notification popup for celebrating achievement unlocks, completed
//! challenges and other one-shot announcements

use crate::icons;
use crate::theme::Theme;
use arct_core::{Achievement, Challenge};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

/// How many animation ticks the border animation runs for (ticks arrive
/// roughly every 100ms, so ~20 ticks ≈ 2 seconds)
const ANIMATION_TICKS: u32 = 20;

/// Border style for a given animation tick: cycles through three frame
/// styles for the first ~2 seconds, then settles on a steady double border.
/// Same accent color throughout — motion, not flashing.
pub fn border_type_for_tick(tick: u32) -> BorderType {
    if tick >= ANIMATION_TICKS {
        return BorderType::Double;
    }
    match (tick / 3) % 3 {
        0 => BorderType::Double,
        1 => BorderType::Thick,
        _ => BorderType::Rounded,
    }
}

/// Notification panel for displaying celebration popups
pub struct NotificationPanel {
    /// Popup title bar text (e.g. " ACHIEVEMENT UNLOCKED! ")
    header: String,
    /// Decorative icon shown next to the title
    icon: String,
    /// Main title (achievement/challenge name)
    title: String,
    /// Longer description text
    description: String,
    /// Points awarded, if any
    points: Option<u32>,
}

impl NotificationPanel {
    /// Notification for a newly unlocked achievement
    pub fn achievement(achievement: &Achievement) -> Self {
        Self {
            header: " ACHIEVEMENT UNLOCKED! ".to_string(),
            icon: achievement.icon.to_string(),
            title: achievement.title.clone(),
            description: achievement.description.clone(),
            points: Some(achievement.points),
        }
    }

    /// Backwards-compatible constructor (achievement notification)
    pub fn new(achievement: Achievement) -> Self {
        Self::achievement(&achievement)
    }

    /// Notification for a just-completed daily/weekly challenge
    pub fn challenge(challenge: &Challenge) -> Self {
        Self {
            header: " CHALLENGE COMPLETE! ".to_string(),
            icon: icons::target().content.to_string(),
            title: challenge.title.clone(),
            description: challenge.description.clone(),
            points: Some(challenge.points),
        }
    }

    /// Generic informational notification (no points)
    pub fn info(header: &str, title: &str, description: &str) -> Self {
        Self {
            header: format!(" {} ", header.trim()),
            icon: icons::info().content.to_string(),
            title: title.to_string(),
            description: description.to_string(),
            points: None,
        }
    }

    /// Render the notification as a centered overlay popup.
    ///
    /// `anim_tick` drives a brief celebratory border animation (~2s of
    /// cycling frame styles, then a steady double border).
    pub fn render(&self, frame: &mut Frame, theme: &Theme, anim_tick: u32) {
        let area = Self::centered_rect(50, 30, frame.size());

        // Clear the background
        frame.render_widget(Clear, area);

        // Celebration border: animated frame style, steady accent color
        let block = Block::default()
            .title(self.header.clone())
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(border_type_for_tick(anim_tick))
            .border_style(theme.style_accent().add_modifier(ratatui::style::Modifier::BOLD));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Split into sections
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Celebration header
                Constraint::Length(4),  // Notification info
                Constraint::Min(2),     // Description
                Constraint::Length(3),  // Points and dismiss
            ])
            .split(inner);

        // Celebration header
        let celebration = Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("✦", theme.style_warning()),
                Span::raw("  "),
                Span::styled("✧", theme.style_accent()),
                Span::raw("  "),
                Span::styled("✦", theme.style_warning()),
            ]),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(celebration, chunks[0]);

        // Notification info (icon + title)
        let notification_info = Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    self.icon.clone(),
                    theme.style_accent().add_modifier(ratatui::style::Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(
                    &self.title,
                    theme.style_accent().add_modifier(ratatui::style::Modifier::BOLD),
                ),
            ]),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(notification_info, chunks[1]);

        // Description
        let description = Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                &self.description,
                theme.style_secondary(),
            )]),
        ])
        .alignment(Alignment::Center)
        .wrap(ratatui::widgets::Wrap { trim: true });
        frame.render_widget(description, chunks[2]);

        // Points and dismiss instruction
        let mut footer_lines = Vec::new();
        if let Some(points) = self.points {
            footer_lines.push(Line::from(vec![
                icons::celebration(),
                Span::styled(format!("+{} points!", points), theme.style_accent()),
            ]));
        } else {
            footer_lines.push(Line::from(""));
        }
        footer_lines.push(Line::from(""));
        footer_lines.push(Line::from(vec![
            Span::styled("Press Enter or Esc to continue", theme.style_dim()),
        ]));

        let footer = Paragraph::new(footer_lines).alignment(Alignment::Center);
        frame.render_widget(footer, chunks[3]);

        // Add decorative border effect
        // Draw corner decorations
        if area.width > 4 && area.height > 2 {
            // Top corners
            frame.render_widget(
                Paragraph::new("").block(
                    Block::default()
                        .borders(Borders::NONE)
                        .border_style(theme.style_accent()),
                ),
                Rect::new(area.x, area.y, 2, 1),
            );
            frame.render_widget(
                Paragraph::new("").block(
                    Block::default()
                        .borders(Borders::NONE)
                        .border_style(theme.style_accent()),
                ),
                Rect::new(area.x + area.width - 2, area.y, 2, 1),
            );
        }
    }

    /// Helper function to create a centered rectangle
    fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
        let popup_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ])
            .split(r);

        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ])
            .split(popup_layout[1])[1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_border_animation_settles_after_two_seconds() {
        // During the animation window the style varies...
        let styles: std::collections::HashSet<_> = (0..ANIMATION_TICKS)
            .map(|t| format!("{:?}", border_type_for_tick(t)))
            .collect();
        assert!(styles.len() >= 2, "border animation should cycle styles");

        // ...after it, the border is steady
        for tick in ANIMATION_TICKS..ANIMATION_TICKS + 50 {
            assert_eq!(border_type_for_tick(tick), BorderType::Double);
        }
    }
}
