//! Lesson selection menu — a visual skill tree
//!
//! Lessons are grouped by difficulty into a progression map. Every row shows
//! a state glyph (✓ completed / ▶ available / · locked), a per-lesson resume
//! indicator when a lesson was left mid-way, difficulty color coding, and the
//! estimated minutes. Locked lessons are selectable but show what unlocks
//! them instead of starting.

use crate::icons;
use crate::persistence::LessonResumeState;
use crate::theme::Theme;
use arct_core::{Difficulty, Lesson, LessonLibrary};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};
use std::collections::{HashMap, HashSet};

/// Progression state of a lesson within the skill tree
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LessonState {
    /// Already completed
    Completed,
    /// Prerequisites satisfied — ready to start
    Available,
    /// One or more prerequisites not yet completed
    Locked,
}

/// Compute the skill-tree state of a lesson given the completed set.
/// Completion wins over locking (a completed lesson is never shown locked).
pub fn lesson_state(lesson: &Lesson, completed: &HashSet<String>) -> LessonState {
    if completed.contains(&lesson.id) {
        LessonState::Completed
    } else if lesson.prerequisites.iter().all(|p| completed.contains(p)) {
        LessonState::Available
    } else {
        LessonState::Locked
    }
}

/// Order lessons grouped by difficulty (Beginner → Expert), preserving the
/// library's order within each group. This is the display order of the map.
pub fn grouped_lessons(library: &LessonLibrary) -> Vec<Lesson> {
    let mut lessons: Vec<Lesson> = library.all().into_iter().cloned().collect();
    lessons.sort_by_key(|l| difficulty_rank(l.difficulty));
    lessons
}

fn difficulty_rank(difficulty: Difficulty) -> u8 {
    match difficulty {
        Difficulty::Beginner => 0,
        Difficulty::Intermediate => 1,
        Difficulty::Advanced => 2,
        Difficulty::Expert => 3,
    }
}

fn difficulty_name(difficulty: Difficulty) -> &'static str {
    match difficulty {
        Difficulty::Beginner => "Beginner",
        Difficulty::Intermediate => "Intermediate",
        Difficulty::Advanced => "Advanced",
        Difficulty::Expert => "Expert",
    }
}

/// One display row in the rendered map
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplayRow {
    Header(Difficulty),
    Lesson(usize),
}

/// Lesson menu panel: skill-tree view of the lesson library
pub struct LessonMenuPanel {
    /// Lessons in display order (grouped by difficulty)
    lessons: Vec<Lesson>,
    /// Index into `lessons`
    selected_index: usize,
    /// Scroll offset in display rows
    scroll_offset: usize,
    /// Transient status line (e.g. "locked" feedback after Enter)
    status_message: Option<String>,
    /// Last rendered list area (for mouse hit-testing)
    last_list_area: Option<Rect>,
    /// Per rendered list line: the lesson index it shows, if any
    last_row_map: Vec<Option<usize>>,
}

impl LessonMenuPanel {
    pub fn new() -> Self {
        Self::with_library(LessonLibrary::new())
    }

    /// Create a menu backed by a specific lesson library (built-ins merged
    /// with user lesson packs loaded at startup)
    pub fn with_library(library: LessonLibrary) -> Self {
        Self {
            lessons: grouped_lessons(&library),
            selected_index: 0,
            scroll_offset: 0,
            status_message: None,
            last_list_area: None,
            last_row_map: Vec::new(),
        }
    }

    /// Move selection up
    pub fn select_previous(&mut self) {
        let total = self.lessons.len();
        if total > 0 {
            self.selected_index = if self.selected_index == 0 {
                total - 1
            } else {
                self.selected_index - 1
            };
            self.status_message = None;
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        let total = self.lessons.len();
        if total > 0 {
            self.selected_index = (self.selected_index + 1) % total;
            self.status_message = None;
        }
    }

    /// Select lesson by number (1-indexed, in display order)
    pub fn select_by_number(&mut self, number: usize) {
        if number > 0 && number <= self.lessons.len() {
            self.selected_index = number - 1;
            self.status_message = None;
        }
    }

    /// Get currently selected lesson
    pub fn get_selected_lesson(&self) -> Option<Lesson> {
        self.lessons.get(self.selected_index).cloned()
    }

    /// Skill-tree state of the currently selected lesson
    pub fn selected_state(&self, completed: &HashSet<String>) -> Option<LessonState> {
        self.lessons
            .get(self.selected_index)
            .map(|l| lesson_state(l, completed))
    }

    /// Titles of the not-yet-completed prerequisites of a lesson
    /// (what the user still needs to unlock it)
    pub fn missing_prerequisite_titles(
        &self,
        lesson: &Lesson,
        completed: &HashSet<String>,
    ) -> Vec<String> {
        lesson
            .prerequisites
            .iter()
            .filter(|p| !completed.contains(*p))
            .map(|p| {
                self.lessons
                    .iter()
                    .find(|l| &l.id == p)
                    .map(|l| l.title.clone())
                    .unwrap_or_else(|| p.clone())
            })
            .collect()
    }

    /// Show a transient status line at the bottom of the menu
    pub fn set_status(&mut self, message: String) {
        self.status_message = Some(message);
    }

    /// Mouse support: handle a click at absolute screen coordinates.
    /// Returns `true` when the click lands on the already-selected lesson
    /// (i.e. the caller should activate it, like pressing Enter).
    pub fn click_at(&mut self, column: u16, row: u16) -> bool {
        let Some(area) = self.last_list_area else {
            return false;
        };
        if column < area.x
            || column >= area.x + area.width
            || row < area.y
            || row >= area.y + area.height
        {
            return false;
        }
        let relative = (row - area.y) as usize;
        match self.last_row_map.get(relative).copied().flatten() {
            Some(idx) if idx == self.selected_index => true,
            Some(idx) => {
                self.selected_index = idx;
                self.status_message = None;
                false
            }
            None => false,
        }
    }

    /// Build the display model: difficulty headers interleaved with lessons
    fn display_rows(&self) -> Vec<DisplayRow> {
        let mut rows = Vec::new();
        let mut last_difficulty: Option<Difficulty> = None;
        for (idx, lesson) in self.lessons.iter().enumerate() {
            if last_difficulty != Some(lesson.difficulty) {
                rows.push(DisplayRow::Header(lesson.difficulty));
                last_difficulty = Some(lesson.difficulty);
            }
            rows.push(DisplayRow::Lesson(idx));
        }
        rows
    }

    /// Keep the selected lesson's display row visible
    fn ensure_visible(&mut self, rows: &[DisplayRow], visible_height: usize) {
        if visible_height == 0 {
            return;
        }
        let selected_row = rows
            .iter()
            .position(|r| *r == DisplayRow::Lesson(self.selected_index))
            .unwrap_or(0);

        if selected_row < self.scroll_offset {
            // Pull the group header into view too when it's directly above
            self.scroll_offset = selected_row.saturating_sub(1);
        } else if selected_row >= self.scroll_offset + visible_height {
            self.scroll_offset = selected_row + 1 - visible_height;
        }
    }

    fn difficulty_style(theme: &Theme, difficulty: Difficulty) -> Style {
        match difficulty {
            Difficulty::Beginner => theme.style_success(),
            Difficulty::Intermediate => theme.style_info(),
            Difficulty::Advanced => theme.style_warning(),
            Difficulty::Expert => theme.style_error(),
        }
    }

    /// Render the lesson map overlay (centered popup)
    pub fn render(
        &mut self,
        frame: &mut Frame,
        theme: &Theme,
        completed_lessons: &HashSet<String>,
        user_stats: &arct_core::UserStats,
        recommendation_engine: &arct_core::RecommendationEngine,
        lesson_progress: &HashMap<String, LessonResumeState>,
    ) {
        let area = Self::centered_rect(74, 70, frame.size());

        // Clear the background
        frame.render_widget(Clear, area);

        let done = self
            .lessons
            .iter()
            .filter(|l| completed_lessons.contains(&l.id))
            .count();
        let block = Block::default()
            .title(format!(
                " {}Lesson Map — {}/{} completed ",
                icons::lesson().content,
                done,
                self.lessons.len()
            ))
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
                Constraint::Length(2), // Recommended lesson
                Constraint::Min(8),    // Lesson map
                Constraint::Length(3), // Selected lesson detail
                Constraint::Length(2), // Controls / status
            ])
            .split(inner);

        // Recommended lesson (top pick)
        let recommendations =
            recommendation_engine.get_recommendations(completed_lessons, user_stats, 1);
        let rec_line = match recommendations.first() {
            Some(rec) => Line::from(vec![
                icons::target(),
                Span::styled("Recommended: ", theme.style_dim()),
                Span::styled(
                    rec.lesson.title.clone(),
                    theme.style_accent().add_modifier(Modifier::BOLD),
                ),
            ]),
            None => Line::from(vec![
                icons::target(),
                Span::styled("Pick any available (▶) lesson to begin!", theme.style_dim()),
            ]),
        };
        frame.render_widget(
            Paragraph::new(vec![rec_line, Line::from("")]).alignment(Alignment::Center),
            chunks[0],
        );

        // Lesson map with scrolling
        let rows = self.display_rows();
        let list_height = chunks[1].height as usize;
        self.ensure_visible(&rows, list_height);
        // Clamp scroll if the list shrank
        self.scroll_offset = self
            .scroll_offset
            .min(rows.len().saturating_sub(list_height.max(1)));

        let mut items: Vec<ListItem> = Vec::new();
        let mut row_map: Vec<Option<usize>> = Vec::new();

        for row in rows.iter().skip(self.scroll_offset).take(list_height) {
            match row {
                DisplayRow::Header(difficulty) => {
                    let name = difficulty_name(*difficulty);
                    let rule = "─".repeat(30usize.saturating_sub(name.len()));
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("  ── ", theme.style_dim()),
                        Span::styled(
                            name,
                            Self::difficulty_style(theme, *difficulty)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(format!(" {}", rule), theme.style_dim()),
                    ])));
                    row_map.push(None);
                }
                DisplayRow::Lesson(idx) => {
                    let lesson = &self.lessons[*idx];
                    let state = lesson_state(lesson, completed_lessons);
                    let is_selected = *idx == self.selected_index;

                    let (glyph, glyph_style) = match state {
                        LessonState::Completed => ("✓ ", theme.style_success()),
                        LessonState::Available => ("▶ ", theme.style_accent()),
                        LessonState::Locked => ("· ", theme.style_dim()),
                    };

                    let title_style = if is_selected {
                        theme
                            .style_selection()
                            .add_modifier(Modifier::BOLD)
                    } else if state == LessonState::Locked {
                        theme.style_dim()
                    } else {
                        theme.style_normal()
                    };

                    let mut spans = vec![
                        Span::styled(if is_selected { " ❯" } else { "  " }, theme.style_accent()),
                        Span::styled(format!("{:>2}. ", idx + 1), theme.style_dim()),
                        Span::styled(glyph, glyph_style),
                        Span::styled(format!(" {:<32}", truncate(&lesson.title, 32)), title_style),
                    ];

                    // Resume indicator for partially done lessons
                    let progress = lesson_progress
                        .get(&lesson.id)
                        .filter(|p| p.current_step_index > 0 && state != LessonState::Completed);
                    if let Some(p) = progress {
                        let total = lesson.steps.len();
                        spans.push(Span::styled(
                            format!(" ◐ {}/{}", p.current_step_index.min(total), total),
                            theme.style_warning(),
                        ));
                    } else {
                        spans.push(Span::raw("      "));
                    }

                    spans.push(Span::styled(
                        format!("  {:>3} min", lesson.estimated_minutes),
                        Self::difficulty_style(theme, lesson.difficulty),
                    ));

                    items.push(ListItem::new(Line::from(spans)));
                    row_map.push(Some(*idx));
                }
            }
        }

        self.last_list_area = Some(chunks[1]);
        self.last_row_map = row_map;

        frame.render_widget(List::new(items), chunks[1]);

        // Selected lesson detail: description, or what unlocks it
        let detail_lines = match self.lessons.get(self.selected_index) {
            Some(lesson) => match lesson_state(lesson, completed_lessons) {
                LessonState::Locked => {
                    let missing = self
                        .missing_prerequisite_titles(lesson, completed_lessons)
                        .join(", ");
                    vec![
                        Line::from(""),
                        Line::from(vec![
                            Span::styled("  Locked: complete ", theme.style_warning()),
                            Span::styled(missing, theme.style_accent().add_modifier(Modifier::BOLD)),
                            Span::styled(" first", theme.style_warning()),
                        ]),
                    ]
                }
                _ => vec![
                    Line::from(""),
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled(lesson.description.clone(), theme.style_secondary()),
                    ]),
                ],
            },
            None => vec![Line::from("")],
        };
        frame.render_widget(Paragraph::new(detail_lines), chunks[2]);

        // Controls / status line
        let controls_line = match &self.status_message {
            Some(status) => Line::from(vec![Span::styled(
                status.clone(),
                theme.style_warning().add_modifier(Modifier::BOLD),
            )]),
            None => Line::from(vec![
                Span::styled("↑/↓", theme.style_accent()),
                Span::raw(" select  "),
                Span::styled("1-9,0", theme.style_accent()),
                Span::raw(" jump  "),
                Span::styled("Enter", theme.style_accent()),
                Span::raw(" start  "),
                Span::styled("Esc", theme.style_accent()),
                Span::raw(" close"),
            ]),
        };
        frame.render_widget(
            Paragraph::new(vec![Line::from(""), controls_line]).alignment(Alignment::Center),
            chunks[3],
        );
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

/// Truncate a string to at most `max` chars, adding an ellipsis if cut
fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let cut: String = text.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", cut)
    }
}

impl Default for LessonMenuPanel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn completed(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    fn find(lessons: &[Lesson], id: &str) -> Lesson {
        lessons
            .iter()
            .find(|l| l.id == id)
            .cloned()
            .unwrap_or_else(|| panic!("built-in lesson '{}' not found", id))
    }

    #[test]
    fn test_lesson_state_no_prereqs_is_available() {
        let lessons = grouped_lessons(&LessonLibrary::new());
        let nav = find(&lessons, "nav-basics");
        assert!(nav.prerequisites.is_empty());
        assert_eq!(lesson_state(&nav, &completed(&[])), LessonState::Available);
    }

    #[test]
    fn test_lesson_state_locked_until_prereqs_done() {
        let lessons = grouped_lessons(&LessonLibrary::new());
        // Find any lesson with at least one prerequisite
        let gated = lessons
            .iter()
            .find(|l| !l.prerequisites.is_empty())
            .expect("library has at least one gated lesson");

        assert_eq!(lesson_state(gated, &completed(&[])), LessonState::Locked);

        // Completing all prerequisites unlocks it
        let prereqs: Vec<&str> = gated.prerequisites.iter().map(String::as_str).collect();
        assert_eq!(
            lesson_state(gated, &completed(&prereqs)),
            LessonState::Available
        );
    }

    #[test]
    fn test_lesson_state_completed_wins_over_locked() {
        let lessons = grouped_lessons(&LessonLibrary::new());
        let gated = lessons
            .iter()
            .find(|l| !l.prerequisites.is_empty())
            .unwrap();
        // Completed even though its prerequisites aren't (e.g. imported progress)
        let state = lesson_state(gated, &completed(&[gated.id.as_str()]));
        assert_eq!(state, LessonState::Completed);
    }

    #[test]
    fn test_grouped_lessons_are_ordered_by_difficulty() {
        let lessons = grouped_lessons(&LessonLibrary::new());
        assert!(!lessons.is_empty());
        let ranks: Vec<u8> = lessons.iter().map(|l| difficulty_rank(l.difficulty)).collect();
        let mut sorted = ranks.clone();
        sorted.sort();
        assert_eq!(ranks, sorted, "lessons must be grouped by difficulty");

        // Grouping is a reordering, not a filter: same set as the library
        let library = LessonLibrary::new();
        assert_eq!(lessons.len(), library.all().len());
    }

    #[test]
    fn test_missing_prerequisite_titles_resolve_to_names() {
        let menu = LessonMenuPanel::new();
        let gated = menu
            .lessons
            .iter()
            .find(|l| !l.prerequisites.is_empty())
            .cloned()
            .unwrap();
        let titles = menu.missing_prerequisite_titles(&gated, &completed(&[]));
        assert_eq!(titles.len(), gated.prerequisites.len());
        // Titles are human names, not raw ids
        for (title, id) in titles.iter().zip(gated.prerequisites.iter()) {
            assert_ne!(title, id);
        }
        // Once completed, nothing is missing
        let prereqs: Vec<&str> = gated.prerequisites.iter().map(String::as_str).collect();
        assert!(menu
            .missing_prerequisite_titles(&gated, &completed(&prereqs))
            .is_empty());
    }

    #[test]
    fn test_selection_navigation_wraps() {
        let mut menu = LessonMenuPanel::new();
        let total = menu.lessons.len();
        assert!(total > 1);
        menu.select_previous();
        assert_eq!(menu.selected_index, total - 1);
        menu.select_next();
        assert_eq!(menu.selected_index, 0);
        menu.select_by_number(2);
        assert_eq!(menu.selected_index, 1);
    }

    #[test]
    fn test_selected_state_reports_locked() {
        let mut menu = LessonMenuPanel::new();
        let gated_idx = menu
            .lessons
            .iter()
            .position(|l| !l.prerequisites.is_empty())
            .unwrap();
        menu.select_by_number(gated_idx + 1);
        assert_eq!(
            menu.selected_state(&completed(&[])),
            Some(LessonState::Locked)
        );
    }
}
