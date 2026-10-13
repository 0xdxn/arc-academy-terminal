//! Help overlay panel

use crate::theme::Theme;
use ratatui::{
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

/// Help panel
pub struct HelpPanel;

impl HelpPanel {
    pub fn new() -> Self {
        Self
    }

    /// The help text: every key here works in any terminal (the Alt keys
    /// are listed as extras, since a Mac's Option key often types a
    /// character instead)
    pub fn lines(theme: &Theme) -> Vec<Line<'static>> {
        let section = |name: &'static str| Line::from(Span::styled(name, theme.style_header()));
        let row = |keys: &'static str, what: &'static str| {
            Line::from(vec![
                Span::styled(format!("  {keys:<18}"), theme.style_accent()),
                Span::styled(what, theme.style_normal()),
            ])
        };
        vec![
            section("Typing"),
            row("Type, then Enter", "Run a command"),
            row("Tab", "Complete a command or file name"),
            row("↑ / ↓", "Earlier commands"),
            Line::from(""),
            section("Scrolling"),
            row("Ctrl+U / Ctrl+D", "Scroll the terminal back / forward"),
            row("Mouse wheel", "Scroll what is under the pointer"),
            row("Enter", "Read on, when a lesson step says there is more"),
            row("Type anything", "Back to the newest output"),
            row("Tab / Shift+Tab", "Move between panels (input line empty)"),
            Line::from(""),
            section("Actions"),
            row("Ctrl+K", "Every action, searchable"),
            row("Ctrl+L", "Lessons; in a lesson the map; in the map, leave"),
            row("Ctrl+B", "Back one lesson step"),
            row("Ctrl+P", "Your progress"),
            row("Ctrl+F", "Show or hide the side panel"),
            row("Ctrl+T / Ctrl+S", "Theme / settings"),
            row("Ctrl+A", "AI assistant (turn it on in settings)"),
            row("? or F1", "This help (? on an empty input line)"),
            row("Ctrl+C", "Quit"),
            Line::from(""),
            Line::from(Span::styled(
                "  Where the terminal sends them: PgUp/PgDn, Shift+↑↓, End, Alt+←",
                theme.style_dim(),
            )),
        ]
    }

    /// Render the help overlay (centered popup, sized to its text)
    pub fn render(&self, frame: &mut Frame, theme: &Theme) {
        let lines = Self::lines(theme);
        let screen = frame.size();
        let width = 74.min(screen.width.saturating_sub(4));
        let room = screen.height.saturating_sub(2);
        // A blank row above the text when there's room for it
        let pad = u16::from(lines.len() as u16 + 3 <= room);
        let height = (lines.len() as u16 + 2 + pad).min(room);
        let area = Rect {
            x: screen.x + (screen.width.saturating_sub(width)) / 2,
            y: screen.y + (screen.height.saturating_sub(height)) / 2,
            width,
            height,
        };

        // Clear the background
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" Help ")
            .title_alignment(Alignment::Center)
            .title(
                ratatui::widgets::block::Title::from(Span::styled(" Esc closes ", theme.style_dim()))
                    .position(ratatui::widgets::block::Position::Bottom)
                    .alignment(Alignment::Center),
            )
            .borders(Borders::ALL)
            .border_style(theme.style_border_focused())
            .style(theme.style_block());  // Set background for light themes

        let inner = block.inner(area);
        frame.render_widget(block, area);
        let inner = Rect {
            x: inner.x + 1,
            y: inner.y + pad,
            width: inner.width.saturating_sub(2),
            height: inner.height.saturating_sub(pad),
        };
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

impl Default for HelpPanel {
    fn default() -> Self {
        Self::new()
    }
}
