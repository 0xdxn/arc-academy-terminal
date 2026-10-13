//! Command palette (Ctrl+K) - fuzzy-searchable list of every named action
//!
//! This is the discoverability layer: every feature is reachable from here,
//! with its keybinding shown as a hint. Selecting an entry routes through the
//! normal `Action` dispatch so behavior stays in one place.

use crate::events::Action;
use crate::theme::Theme;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

/// What a palette entry does when executed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCommand {
    /// Dispatch a regular application action
    Action(Action),
    /// Set a specific theme by name
    SetTheme(&'static str),
}

/// A single palette entry: display name, keybinding hint, and command
pub struct PaletteEntry {
    pub name: &'static str,
    pub hint: &'static str,
    pub command: PaletteCommand,
}

/// All named actions reachable from the palette
const ENTRIES: &[PaletteEntry] = &[
    PaletteEntry { name: "Lessons / Lesson Map", hint: "Ctrl+L", command: PaletteCommand::Action(Action::ToggleLesson) },
    PaletteEntry { name: "Leave Lessons", hint: "Ctrl+L in the map", command: PaletteCommand::Action(Action::ExitLessons) },
    PaletteEntry { name: "Previous Lesson Step", hint: "Ctrl+B", command: PaletteCommand::Action(Action::LessonPreviousStep) },
    PaletteEntry { name: "Restart Lesson", hint: "", command: PaletteCommand::Action(Action::LessonRestart) },
    PaletteEntry { name: "Toggle Real-Filesystem Practice", hint: "", command: PaletteCommand::Action(Action::TogglePracticeMode) },
    PaletteEntry { name: "Reset Lesson Playground", hint: "", command: PaletteCommand::Action(Action::ResetPlayground) },
    PaletteEntry { name: "Show Achievements", hint: "", command: PaletteCommand::Action(Action::ShowAchievements) },
    PaletteEntry { name: "Show Progress", hint: "Ctrl+P", command: PaletteCommand::Action(Action::ShowProgress) },
    PaletteEntry { name: "Show Challenges", hint: "", command: PaletteCommand::Action(Action::ShowChallenges) },
    PaletteEntry { name: "Show/Hide Side Panel", hint: "Ctrl+F", command: PaletteCommand::Action(Action::ToggleSidebar) },
    PaletteEntry { name: "Open Settings", hint: "Ctrl+S", command: PaletteCommand::Action(Action::ToggleSettings) },
    PaletteEntry { name: "Cycle Theme", hint: "Ctrl+T", command: PaletteCommand::Action(Action::ToggleTheme) },
    PaletteEntry { name: "Theme: Arc Academy Orange", hint: "", command: PaletteCommand::SetTheme("Arc Academy Orange") },
    PaletteEntry { name: "Theme: Arc Academy Green", hint: "", command: PaletteCommand::SetTheme("Arc Academy Green") },
    PaletteEntry { name: "Theme: Arc Dark", hint: "", command: PaletteCommand::SetTheme("Arc Dark") },
    PaletteEntry { name: "Theme: Arc Light", hint: "", command: PaletteCommand::SetTheme("Arc Light") },
    PaletteEntry { name: "Theme: Night", hint: "", command: PaletteCommand::SetTheme("Night") },
    PaletteEntry { name: "Theme: Mocha", hint: "", command: PaletteCommand::SetTheme("Mocha") },
    PaletteEntry { name: "Toggle AI Assistant", hint: "Ctrl+A", command: PaletteCommand::Action(Action::ToggleAI) },
    PaletteEntry { name: "Help", hint: "?", command: PaletteCommand::Action(Action::Help) },
    PaletteEntry { name: "Quit", hint: "q / Ctrl+C", command: PaletteCommand::Action(Action::Quit) },
];

/// Result of feeding a key event into the palette
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteOutcome {
    /// Palette consumed the key; stay open
    Pending,
    /// Close the palette without executing anything
    Close,
    /// Execute the given command and close the palette
    Execute(PaletteCommand),
}

/// Command palette state (text filter + selection)
pub struct CommandPalette {
    input: String,
    selected: usize,
    /// Last rendered list area + scroll (for mouse hit-testing)
    last_list_area: Option<Rect>,
    last_scroll: usize,
}

impl CommandPalette {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            selected: 0,
            last_list_area: None,
            last_scroll: 0,
        }
    }

    /// Move the selection one row up/down (used by the mouse wheel)
    pub fn scroll_selection(&mut self, down: bool) {
        let len = self.filtered().len();
        if len == 0 {
            return;
        }
        self.selected = if down {
            (self.selected + 1) % len
        } else if self.selected == 0 {
            len - 1
        } else {
            self.selected - 1
        };
    }

    /// Mouse support: handle a click at absolute screen coordinates.
    /// Clicking a row selects it; clicking the already-selected row runs it.
    pub fn click_at(&mut self, column: u16, row: u16) -> PaletteOutcome {
        let Some(area) = self.last_list_area else {
            return PaletteOutcome::Pending;
        };
        if column < area.x
            || column >= area.x + area.width
            || row < area.y
            || row >= area.y + area.height
        {
            return PaletteOutcome::Pending;
        }
        let idx = self.last_scroll + (row - area.y) as usize;
        let filtered = self.filtered();
        match filtered.get(idx) {
            Some(entry) if idx == self.selected => PaletteOutcome::Execute(entry.command),
            Some(_) => {
                self.selected = idx;
                PaletteOutcome::Pending
            }
            None => PaletteOutcome::Pending,
        }
    }

    /// Case-insensitive fuzzy match: `query` matches `name` if it is a
    /// substring, or if its characters appear in `name` in order
    /// (subsequence match, so "tglai" finds "Toggle AI Assistant").
    fn matches(name: &str, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let name = name.to_lowercase();
        let query = query.to_lowercase();

        if name.contains(&query) {
            return true;
        }

        // Subsequence match
        let mut chars = query.chars().filter(|c| !c.is_whitespace());
        let mut needle = chars.next();
        for c in name.chars() {
            match needle {
                Some(n) if n == c => needle = chars.next(),
                Some(_) => {}
                None => break,
            }
        }
        needle.is_none()
    }

    /// Entries matching the current filter, in declaration order
    pub fn filtered(&self) -> Vec<&'static PaletteEntry> {
        ENTRIES
            .iter()
            .filter(|e| Self::matches(e.name, &self.input))
            .collect()
    }

    /// Handle a key event while the palette is open
    pub fn handle_key(&mut self, key: KeyEvent) -> PaletteOutcome {
        match key.code {
            KeyCode::Esc => PaletteOutcome::Close,
            KeyCode::Enter => {
                let filtered = self.filtered();
                match filtered.get(self.selected) {
                    Some(entry) => PaletteOutcome::Execute(entry.command),
                    None => PaletteOutcome::Close,
                }
            }
            KeyCode::Up => {
                let len = self.filtered().len();
                if len > 0 {
                    self.selected = if self.selected == 0 {
                        len - 1
                    } else {
                        self.selected - 1
                    };
                }
                PaletteOutcome::Pending
            }
            KeyCode::Down => {
                let len = self.filtered().len();
                if len > 0 {
                    self.selected = (self.selected + 1) % len;
                }
                PaletteOutcome::Pending
            }
            KeyCode::Backspace => {
                self.input.pop();
                self.clamp_selection();
                PaletteOutcome::Pending
            }
            KeyCode::Char(c)
                if key.modifiers == KeyModifiers::NONE
                    || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.input.push(c);
                self.clamp_selection();
                PaletteOutcome::Pending
            }
            _ => PaletteOutcome::Pending,
        }
    }

    /// Keep the selection inside the filtered list after filter changes
    fn clamp_selection(&mut self) {
        let len = self.filtered().len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }

    /// Render the palette as a centered overlay popup
    pub fn render(&mut self, frame: &mut Frame, theme: &Theme) {
        let area = Self::centered_rect(55, 60, frame.size());

        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" Command Palette ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(theme.style_border_focused())
            .style(theme.style_block());

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Input line
                Constraint::Min(3),    // Entry list
                Constraint::Length(1), // Controls
            ])
            .split(inner);

        // Input line with cursor
        let input_line = Paragraph::new(vec![
            Line::from(vec![
                Span::styled("> ", theme.style_accent()),
                Span::styled(&self.input, theme.style_normal()),
                Span::styled("█", theme.style_accent()),
            ]),
            Line::from(""),
        ]);
        frame.render_widget(input_line, chunks[0]);

        // Filtered entry list
        let filtered = self.filtered();
        let list_height = chunks[1].height as usize;

        // Keep the selected row visible
        let scroll = if self.selected >= list_height && list_height > 0 {
            self.selected + 1 - list_height
        } else {
            0
        };

        // Remember geometry for mouse hit-testing
        self.last_list_area = Some(chunks[1]);
        self.last_scroll = scroll;

        let hint_col = 34usize;
        let mut items: Vec<ListItem> = Vec::new();
        if filtered.is_empty() {
            items.push(ListItem::new(Line::from(vec![Span::styled(
                "  No matching commands",
                theme.style_dim(),
            )])));
        } else {
            for (idx, entry) in filtered.iter().enumerate().skip(scroll).take(list_height) {
                let selected = idx == self.selected;
                let marker = if selected { "▶ " } else { "  " };
                let name_style = if selected {
                    theme
                        .style_accent()
                        .add_modifier(ratatui::style::Modifier::BOLD)
                } else {
                    theme.style_normal()
                };

                let padding = " ".repeat(hint_col.saturating_sub(entry.name.len()));
                items.push(ListItem::new(Line::from(vec![
                    Span::styled(marker, theme.style_accent()),
                    Span::styled(entry.name, name_style),
                    Span::raw(padding),
                    Span::styled(entry.hint, theme.style_dim()),
                ])));
            }
        }
        frame.render_widget(List::new(items), chunks[1]);

        // Controls
        let controls = Paragraph::new(Line::from(vec![
            Span::styled("↑/↓", theme.style_accent()),
            Span::raw(" select  "),
            Span::styled("Enter", theme.style_accent()),
            Span::raw(" run  "),
            Span::styled("Esc", theme.style_accent()),
            Span::raw(" close"),
        ]))
        .alignment(Alignment::Center);
        frame.render_widget(controls, chunks[2]);
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

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn test_empty_filter_shows_all_entries() {
        let palette = CommandPalette::new();
        assert_eq!(palette.filtered().len(), ENTRIES.len());
    }

    #[test]
    fn test_substring_filter_case_insensitive() {
        let mut palette = CommandPalette::new();
        for c in "THEME".chars() {
            palette.handle_key(key(KeyCode::Char(c)));
        }
        let filtered = palette.filtered();
        assert!(!filtered.is_empty());
        assert!(filtered.iter().all(|e| e.name.to_lowercase().contains("theme")));
    }

    #[test]
    fn test_subsequence_fuzzy_match() {
        assert!(CommandPalette::matches("Toggle AI Assistant", "tglai"));
        assert!(!CommandPalette::matches("Quit", "xyz"));
    }

    #[test]
    fn test_enter_executes_selected_entry() {
        let mut palette = CommandPalette::new();
        for c in "quit".chars() {
            palette.handle_key(key(KeyCode::Char(c)));
        }
        let outcome = palette.handle_key(key(KeyCode::Enter));
        assert_eq!(
            outcome,
            PaletteOutcome::Execute(PaletteCommand::Action(Action::Quit))
        );
    }

    #[test]
    fn test_arrows_wrap_and_esc_closes() {
        let mut palette = CommandPalette::new();
        assert_eq!(palette.handle_key(key(KeyCode::Up)), PaletteOutcome::Pending);
        assert_eq!(palette.selected, ENTRIES.len() - 1);
        assert_eq!(palette.handle_key(key(KeyCode::Down)), PaletteOutcome::Pending);
        assert_eq!(palette.selected, 0);
        assert_eq!(palette.handle_key(key(KeyCode::Esc)), PaletteOutcome::Close);
    }

    #[test]
    fn test_selection_clamped_when_filter_narrows() {
        let mut palette = CommandPalette::new();
        // Move selection to the end, then type a narrow filter
        palette.handle_key(key(KeyCode::Up));
        for c in "quit".chars() {
            palette.handle_key(key(KeyCode::Char(c)));
        }
        assert!(palette.selected < palette.filtered().len());
    }
}
