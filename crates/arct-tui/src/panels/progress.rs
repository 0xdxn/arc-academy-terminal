//! Progress dashboard panel for displaying user statistics
//!
//! Visual-first: gauges for per-difficulty completion and XP-to-next-level,
//! plus a 14-day streak calendar strip.

use crate::icons;
use crate::level;
use crate::theme::Theme;
use arct_core::{Difficulty, LessonLibrary, UserStats};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, Paragraph},
    Frame,
};

/// Progress panel for displaying user stats and progress
pub struct ProgressPanel;

impl ProgressPanel {
    pub fn new() -> Self {
        Self
    }

    /// Render the progress dashboard overlay (centered popup).
    ///
    /// `activity` is one entry per day for the last 14 days (oldest first),
    /// `true` when the user ran at least one command that day.
    pub fn render(
        &self,
        frame: &mut Frame,
        theme: &Theme,
        stats: &UserStats,
        library: &LessonLibrary,
        activity: Option<&[bool]>,
    ) {
        let area = Self::centered_rect(70, 70, frame.size());

        // Clear the background
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(format!(" {}Your Progress Dashboard ", icons::target().content))
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(theme.style_border_focused())
            .style(theme.style_block()); // Set background for light themes

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Split into sections
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5), // Overview stats
                Constraint::Length(3), // XP / level gauge
                Constraint::Length(8), // Per-difficulty gauges
                Constraint::Length(4), // Streak calendar strip
                Constraint::Min(1),    // Spacer
                Constraint::Length(1), // Controls help
            ])
            .split(inner);

        self.render_overview(frame, chunks[0], theme, stats);
        self.render_level_gauge(frame, chunks[1], theme, stats);
        self.render_difficulty_gauges(frame, chunks[2], theme, stats, library);
        self.render_streak_strip(frame, chunks[3], theme, stats, activity);

        // Controls help
        let controls = Paragraph::new(vec![Line::from(vec![
            Span::styled("Esc", theme.style_accent()),
            Span::raw(" or "),
            Span::styled("Alt+P", theme.style_accent()),
            Span::raw(" to close"),
        ])])
        .alignment(Alignment::Center);
        frame.render_widget(controls, chunks[5]);
    }

    /// Headline numbers: streak, lessons, commands, achievements, time
    fn render_overview(&self, frame: &mut Frame, area: Rect, theme: &Theme, stats: &UserStats) {
        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("streak ", theme.style_warning()),
                Span::styled(
                    format!("{} day streak", stats.current_streak),
                    theme.style_accent().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  (best: {} days)", stats.longest_streak),
                    theme.style_dim(),
                ),
                Span::raw("    "),
                Span::styled(icons::timer_glyph(), theme.style_info()),
                Span::styled(
                    format!("  {} invested", stats.formatted_time_spent()),
                    theme.style_secondary(),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                icons::lesson(),
                Span::styled(
                    format!("{} lessons", stats.lessons_completed.len()),
                    theme.style_normal(),
                ),
                Span::raw("    "),
                icons::shell(),
                Span::styled(
                    format!("{} commands mastered", stats.commands_used.len()),
                    theme.style_normal(),
                ),
                Span::raw("    "),
                icons::celebration(),
                Span::styled(
                    format!(
                        "{} achievements ({} pts)",
                        stats.achievements.total_unlocked(),
                        stats.achievements.total_points()
                    ),
                    theme.style_normal(),
                ),
            ]),
        ];
        frame.render_widget(Paragraph::new(lines), area);
    }

    /// XP-to-next-level gauge
    fn render_level_gauge(&self, frame: &mut Frame, area: Rect, theme: &Theme, stats: &UserStats) {
        let info = level::level_info(stats);
        let gauge_area = Rect {
            x: area.x + 2,
            y: area.y,
            width: area.width.saturating_sub(4),
            height: area.height.min(3),
        };
        let gauge = Gauge::default()
            .block(
                Block::default()
                    .title(format!(" Level {} ", info.level))
                    .title_style(theme.style_accent().add_modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .border_style(theme.style_border()),
            )
            .gauge_style(Style::default().fg(theme.accent).bg(theme.bg_tertiary))
            .ratio(info.progress())
            .label(format!(
                "{}/{} XP to level {}",
                info.xp_into_level,
                info.xp_for_next,
                info.level + 1
            ));
        frame.render_widget(gauge, gauge_area);
    }

    /// Per-difficulty completion gauges using real lesson counts
    fn render_difficulty_gauges(
        &self,
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        stats: &UserStats,
        library: &LessonLibrary,
    ) {
        let title = Paragraph::new(Line::from(vec![Span::styled(
            "  Lesson Progress",
            theme.style_header(),
        )]));
        frame.render_widget(
            title,
            Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: 1,
            },
        );

        let counts = library.difficulty_counts();
        let difficulties = [
            (Difficulty::Beginner, "Beginner", theme.success),
            (Difficulty::Intermediate, "Intermediate", theme.info),
            (Difficulty::Advanced, "Advanced", theme.warning),
            (Difficulty::Expert, "Expert", theme.error),
        ];

        let mut y = area.y + 1;
        for (difficulty, name, color) in difficulties {
            let total = counts.get(&difficulty).copied().unwrap_or(0);
            if total == 0 {
                continue; // Nothing to show for empty tiers
            }
            if y >= area.y + area.height {
                break;
            }
            let completed = stats
                .lessons_by_difficulty
                .get(&difficulty)
                .copied()
                .unwrap_or(0)
                .min(total);

            let label_area = Rect {
                x: area.x + 2,
                y,
                width: 14.min(area.width.saturating_sub(2)),
                height: 1,
            };
            frame.render_widget(
                Paragraph::new(Span::styled(name, Style::default().fg(color))),
                label_area,
            );

            let gauge_area = Rect {
                x: area.x + 17,
                y,
                width: area.width.saturating_sub(19),
                height: 1,
            };
            if gauge_area.width > 4 {
                let gauge = Gauge::default()
                    .gauge_style(Style::default().fg(color).bg(theme.bg_tertiary))
                    .ratio(completed as f64 / total as f64)
                    .label(format!("{}/{}", completed, total));
                frame.render_widget(gauge, gauge_area);
            }
            y += 1;
        }
    }

    /// 14-day activity strip: ■ for active days, □ for quiet ones
    fn render_streak_strip(
        &self,
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        stats: &UserStats,
        activity: Option<&[bool]>,
    ) {
        let mut lines = vec![
            Line::from(""),
            Line::from(vec![Span::styled("  Last 14 Days", theme.style_header())]),
        ];

        match activity {
            Some(days) if !days.is_empty() => {
                let mut spans = vec![Span::raw("  ")];
                for active in days {
                    if *active {
                        spans.push(Span::styled("■ ", theme.style_success()));
                    } else {
                        spans.push(Span::styled("□ ", theme.style_dim()));
                    }
                }
                spans.push(Span::styled("← today", theme.style_dim()));
                lines.push(Line::from(spans));
            }
            _ => {
                lines.push(Line::from(vec![Span::styled(
                    format!(
                        "  No activity data yet — current streak: {} days",
                        stats.current_streak
                    ),
                    theme.style_dim(),
                )]));
            }
        }

        frame.render_widget(Paragraph::new(lines), area);
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

impl Default for ProgressPanel {
    fn default() -> Self {
        Self::new()
    }
}
