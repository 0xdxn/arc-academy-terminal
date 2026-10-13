//! The shell transcript: what the learner ran, and what came back
//!
//! The output area reads like a real terminal. Each command is kept as an
//! entry: a prompt line with the folder it ran in, its output, then any
//! lesson feedback for it (quieter, so it never reads as command output).
//! Messages that aren't command output (welcome text, mode switches,
//! lesson loaded) are kept as notes. Newest is at the bottom; the view
//! follows new output unless the learner has scrolled back.
//!
//! The app keeps writing `App::last_output` the way it always has; a turn
//! (one key or mouse event) is bracketed by `begin_turn` / `finish_turn`,
//! and whatever the turn left in `last_output` lands in the transcript:
//! as the body of the command entry the turn opened, or as a note.

use crate::theme::Theme;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::collections::VecDeque;
use unicode_width::UnicodeWidthChar;

/// Entries kept in memory; the oldest go first
pub const MAX_ENTRIES: usize = 300;

/// Output lines kept per entry; a huge output keeps its last lines
pub const MAX_LINES_PER_ENTRY: usize = 2000;

/// Output bytes kept per entry, whatever its line count (one huge line, or
/// `yes` until the timeout); the end is kept
pub const MAX_BYTES_PER_ENTRY: usize = 256 * 1024;

/// The end of `text`, at most MAX_BYTES_PER_ENTRY, cut at a character
fn kept(text: &str) -> &str {
    if text.len() <= MAX_BYTES_PER_ENTRY {
        return text;
    }
    let mut start = text.len() - MAX_BYTES_PER_ENTRY;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

/// A cheap fingerprint of the output text, to tell whether a turn changed it
fn fingerprint(text: &str) -> (usize, u64) {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    (text.len(), h.finish())
}

/// What kind of lesson feedback a line is
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// The step passed
    Success,
    /// The step didn't pass yet
    NotYet,
    /// A hint toward the answer
    Hint,
    /// Anything else the lesson says (lesson complete, what's next)
    Info,
}

/// One line (or block) of lesson feedback under a command
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feedback {
    pub tone: Tone,
    pub text: String,
}

/// A command the learner ran, or a note from the app
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    Command { folder: String, command: String },
    /// An answer to a lesson question (typed, not run)
    Answer { answer: String },
    Note,
}

/// One transcript entry
#[derive(Debug, Clone)]
pub struct Entry {
    pub kind: EntryKind,
    output: String,
    /// `output` parsed into lines (ANSI colours kept), cached
    lines: Vec<Line<'static>>,
    pub feedback: Vec<Feedback>,
}

impl Entry {
    fn new(kind: EntryKind) -> Self {
        Self {
            kind,
            output: String::new(),
            lines: Vec::new(),
            feedback: Vec::new(),
        }
    }

    /// The text this entry printed
    pub fn output(&self) -> &str {
        &self.output
    }

    /// Store what this entry printed; false if it already held exactly that
    fn set_output(&mut self, text: &str) -> bool {
        let cut = text.len() > MAX_BYTES_PER_ENTRY;
        let text = kept(text);
        if self.output == text {
            return false;
        }
        self.output = text.to_string();
        let mut lines = crate::ansi::parse_ansi(text.trim_end_matches('\n'));
        if cut {
            lines.insert(0, Line::from("(the start of this output was not kept)"));
        }
        while lines.last().is_some_and(|l| l.width() == 0) {
            lines.pop();
        }
        if lines.len() > MAX_LINES_PER_ENTRY {
            let dropped = lines.len() - MAX_LINES_PER_ENTRY;
            lines.drain(..dropped);
            lines.insert(
                0,
                Line::from(format!("({dropped} earlier lines of this output not kept)")),
            );
        }
        self.lines = lines;
        true
    }
}

/// Rows wrapped for one width and theme, at one generation
#[derive(Debug)]
struct Wrapped {
    generation: u64,
    width: usize,
    theme: String,
    rows: Arc<Vec<Line<'static>>>,
}

/// The scrollback of a shell session
#[derive(Debug)]
pub struct Transcript {
    entries: VecDeque<Entry>,
    cap: usize,
    /// The newest entry belongs to the turn in progress
    open: bool,
    /// Fingerprint of what `last_output` held when the turn began
    turn_start: Option<(usize, u64)>,
    /// Rows scrolled back from the bottom (0 = following new output)
    scroll: usize,
    /// The furthest back the last render could scroll
    max_scroll: Cell<usize>,
    /// Bumped on every change, so a render can reuse the wrapped rows
    generation: u64,
    /// Wrapped rows from the last render
    wrapped: RefCell<Option<Wrapped>>,
}

impl Default for Transcript {
    fn default() -> Self {
        Self::with_cap(MAX_ENTRIES)
    }
}

impl Transcript {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_cap(cap: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            cap: cap.max(1),
            open: false,
            turn_start: None,
            scroll: 0,
            max_scroll: Cell::new(usize::MAX),
            generation: 0,
            wrapped: RefCell::new(None),
        }
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn last(&self) -> Option<&Entry> {
        self.entries.back()
    }

    fn push(&mut self, entry: Entry) {
        self.generation += 1;
        self.entries.push_back(entry);
        while self.entries.len() > self.cap {
            self.entries.pop_front();
        }
        self.open = true;
        self.scroll = 0;
    }

    /// Add a note (not command output) right away
    pub fn note(&mut self, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        let mut entry = Entry::new(EntryKind::Note);
        entry.set_output(text);
        self.push(entry);
        self.open = false;
    }

    /// A turn (one key or mouse event) begins; `output` is what the
    /// output text holds now
    pub fn begin_turn(&mut self, output: &str) {
        self.turn_start = Some(fingerprint(output));
        self.open = false;
    }

    /// The learner ran `command` in `folder`: open its entry
    pub fn begin_command(&mut self, folder: &str, command: &str) {
        if self.turn_start.is_none() {
            self.turn_start = Some(fingerprint(""));
        }
        self.push(Entry::new(EntryKind::Command {
            folder: folder.to_string(),
            command: command.to_string(),
        }));
    }

    /// The learner answered a lesson question: open its entry
    pub fn begin_answer(&mut self, answer: &str) {
        if self.turn_start.is_none() {
            self.turn_start = Some(fingerprint(""));
        }
        self.push(Entry::new(EntryKind::Answer {
            answer: answer.to_string(),
        }));
    }

    /// Lesson feedback for this turn's command (or a note, when the turn
    /// ran no command, e.g. Enter on a reading step)
    pub fn feedback(&mut self, tone: Tone, text: impl Into<String>) {
        let text = text.into();
        if text.trim().is_empty() {
            return;
        }
        if !self.open {
            self.push(Entry::new(EntryKind::Note));
        }
        if let Some(entry) = self.entries.back_mut() {
            entry.feedback.push(Feedback { tone, text });
        }
        self.generation += 1;
        self.scroll = 0;
    }

    /// A lesson validation result as feedback: success, or not yet plus
    /// the hint it earned
    pub fn feedback_for(&mut self, result: &arct_core::ValidationResult) {
        use arct_core::ValidationResult;
        match result {
            ValidationResult::Success { message } => self.feedback(Tone::Success, message.as_str()),
            ValidationResult::Failure { message, hint } => {
                self.feedback(Tone::NotYet, message.as_str());
                if let Some(hint) = hint {
                    self.feedback(Tone::Hint, format!("Hint: {hint}"));
                }
            }
            ValidationResult::Partial { message, progress } => {
                self.feedback(Tone::NotYet, format!("{message} ({progress:.0}% there)"))
            }
        }
    }

    /// The turn is over; `output` is what the output text holds now
    pub fn finish_turn(&mut self, output: &str) {
        let start = self.turn_start.take();
        let changed = start != Some(fingerprint(output));
        match self.entries.back_mut() {
            Some(entry)
                if self.open
                    && matches!(entry.kind, EntryKind::Command { .. } | EntryKind::Answer { .. }) =>
            {
                if entry.set_output(output) {
                    self.generation += 1;
                }
            }
            Some(entry) if self.open && changed => {
                // A note opened by feedback: what the turn printed goes first
                if entry.set_output(output) {
                    self.generation += 1;
                }
            }
            _ if changed && !output.trim().is_empty() => {
                let mut entry = Entry::new(EntryKind::Note);
                entry.set_output(output);
                self.push(entry);
            }
            _ => {}
        }
        self.open = false;
    }

    /// Rows scrolled back from the newest output
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    pub fn scroll_up(&mut self, rows: usize) {
        self.scroll = self.scroll.saturating_add(rows).min(self.max_scroll.get());
    }

    pub fn scroll_down(&mut self, rows: usize) {
        self.scroll = self.scroll.saturating_sub(rows);
    }

    pub fn scroll_to_end(&mut self) {
        self.scroll = 0;
    }

    pub fn scroll_to_start(&mut self) {
        self.scroll = self.max_scroll.get();
    }

    /// Every entry as display lines, oldest first (not yet wrapped)
    pub fn lines(&self, theme: &Theme) -> Vec<Line<'static>> {
        let mut out = Vec::new();
        for (i, entry) in self.entries.iter().enumerate() {
            if i > 0 {
                out.push(Line::from(""));
            }
            match &entry.kind {
                EntryKind::Command { folder, command } => {
                    out.push(prompt_line(folder, command, theme));
                    out.extend(entry.lines.iter().cloned());
                }
                EntryKind::Answer { answer } => {
                    out.push(Line::from(vec![
                        Span::styled("Your answer: ".to_string(), theme.style_dim()),
                        Span::styled(
                            answer.clone(),
                            theme.style_normal().add_modifier(Modifier::BOLD),
                        ),
                    ]));
                    out.extend(entry.lines.iter().cloned());
                }
                EntryKind::Note => {
                    out.extend(entry.lines.iter().map(|l| restyle(l, theme.style_dim())));
                }
            }
            for fb in &entry.feedback {
                let style = match fb.tone {
                    Tone::Success => theme.style_success(),
                    Tone::NotYet => theme.style_warning().remove_modifier(Modifier::BOLD),
                    Tone::Hint | Tone::Info => theme.style_dim(),
                };
                for text in fb.text.trim_end_matches('\n').lines() {
                    out.push(Line::from(Span::styled(format!("  {text}"), style)));
                }
            }
        }
        out
    }

    /// The rows to show in a box `width` x `height`: lines hard-wrapped at
    /// the edge like a terminal does, scrolled `scroll` rows back from the
    /// bottom. Also returns how many rows sit below the view.
    pub fn view(&self, theme: &Theme, width: usize, height: usize) -> (Vec<Line<'static>>, usize) {
        let rows = self.wrapped_rows(theme, width);
        let max_scroll = rows.len().saturating_sub(height);
        self.max_scroll.set(max_scroll);
        let scroll = self.scroll.min(max_scroll);
        let end = rows.len() - scroll;
        let start = end.saturating_sub(height);
        (rows[start..end].to_vec(), scroll)
    }
}

impl Transcript {
    /// Every line wrapped at `width`, reused between renders until the
    /// transcript, the width or the theme changes
    fn wrapped_rows(&self, theme: &Theme, width: usize) -> Arc<Vec<Line<'static>>> {
        if let Some(w) = self.wrapped.borrow().as_ref() {
            if w.generation == self.generation && w.width == width && w.theme == theme.name {
                return Arc::clone(&w.rows);
            }
        }
        let rows: Arc<Vec<Line<'static>>> = Arc::new(
            self.lines(theme)
                .iter()
                .flat_map(|l| wrap_line(l, width))
                .collect(),
        );
        *self.wrapped.borrow_mut() = Some(Wrapped {
            generation: self.generation,
            width,
            theme: theme.name.clone(),
            rows: Arc::clone(&rows),
        });
        rows
    }
}

/// `~/ArcAcademy/home/notes $ ls -l`
pub fn prompt_line(folder: &str, command: &str, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(folder.to_string(), theme.style_accent()),
        Span::styled(" $ ".to_string(), theme.style_accent()),
        Span::styled(
            command.to_string(),
            theme.style_normal().add_modifier(Modifier::BOLD),
        ),
    ])
}

/// Apply `style` under each span's own (ANSI) style
fn restyle(line: &Line<'static>, style: Style) -> Line<'static> {
    Line::from(
        line.spans
            .iter()
            .map(|s| Span::styled(s.content.clone(), style.patch(s.style)))
            .collect::<Vec<_>>(),
    )
}

/// Wrap a line at `width` columns: at the last space that fits, or right
/// at the edge for a word longer than the row (like a terminal does)
pub fn wrap_line(line: &Line<'static>, width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let cells: Vec<(char, Style)> = line
        .spans
        .iter()
        .flat_map(|span| span.content.chars().map(move |c| (c, span.style)))
        .collect();

    let mut rows = Vec::new();
    let mut start = 0;
    while start < cells.len() {
        // Take as many cells as fit
        let mut used = 0;
        let mut end = start;
        while end < cells.len() {
            let w = cells[end].0.width().unwrap_or(0);
            if used + w > width && end > start {
                break;
            }
            used += w;
            end += 1;
        }
        // Mid-word: go back to the last space in the row, if there is one
        let mut next = end;
        if end < cells.len() && cells[end].0 != ' ' {
            if let Some(space) = (start + 1..end).rev().find(|&i| cells[i].0 == ' ') {
                end = space;
                next = space + 1;
            }
        } else if end < cells.len() {
            next = end + 1;
        }
        rows.push(spans_of(&cells[start..end]));
        start = next;
    }
    if rows.is_empty() {
        rows.push(Line::from(""));
    }
    rows
}

/// Regroup styled cells into spans
fn spans_of(cells: &[(char, Style)]) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut text = String::new();
    let mut style = cells.first().map(|c| c.1).unwrap_or_default();
    for &(c, s) in cells {
        if s != style && !text.is_empty() {
            spans.push(Span::styled(std::mem::take(&mut text), style));
        }
        style = s;
        text.push(c);
    }
    if !text.is_empty() {
        spans.push(Span::styled(text, style));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_huge_output_keeps_only_its_end_in_memory() {
        let mut t = Transcript::new();
        t.begin_turn("");
        t.begin_command("~", "yes");
        let huge = "y".repeat(MAX_BYTES_PER_ENTRY * 4) + "é" + "end";
        t.finish_turn(&huge);
        let e = t.last().unwrap();
        assert!(e.output().len() <= MAX_BYTES_PER_ENTRY);
        assert!(e.output().ends_with("éend"));
    }

    #[test]
    fn a_turn_that_changes_nothing_keeps_the_wrapped_rows() {
        let mut t = Transcript::new();
        t.begin_turn("");
        t.begin_command("~", "ls");
        t.finish_turn("a\n");
        let generation = t.generation;
        for _ in 0..5 {
            t.begin_turn("a\n");
            t.finish_turn("a\n");
        }
        assert_eq!(t.generation, generation, "an unchanged turn re-wrapped everything");
    }

    fn text(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    fn theme() -> Theme {
        Theme::default()
    }

    #[test]
    fn a_command_turn_keeps_prompt_output_and_feedback_in_order() {
        let mut t = Transcript::new();
        t.begin_turn("old output");
        t.begin_command("/lesson-home/station", "ls");
        t.feedback(Tone::Success, "See the station/ folder?");
        t.finish_turn("bridge/  cargo/\n");

        assert_eq!(t.len(), 1);
        assert_eq!(
            text(&t.lines(&theme())),
            vec!["/lesson-home/station $ ls", "bridge/  cargo/", "  See the station/ folder?"]
        );
    }

    #[test]
    fn a_quiz_answer_is_shown_as_an_answer_not_a_command() {
        let mut t = Transcript::new();
        t.begin_turn("");
        t.begin_answer("1");
        t.feedback(Tone::Success, "Correct!");
        t.finish_turn("");
        assert_eq!(text(&t.lines(&theme())), vec!["Your answer: 1", "  Correct!"]);
    }

    #[test]
    fn earlier_commands_stay_visible() {
        let mut t = Transcript::new();
        for (cmd, out) in [("pwd", "/lesson-home\n"), ("ls", "station/\n")] {
            t.begin_turn("");
            t.begin_command("/lesson-home", cmd);
            t.finish_turn(out);
        }
        let lines = text(&t.lines(&theme()));
        assert_eq!(
            lines,
            vec!["/lesson-home $ pwd", "/lesson-home", "", "/lesson-home $ ls", "station/"]
        );
    }

    #[test]
    fn same_output_twice_is_still_two_entries() {
        let mut t = Transcript::new();
        for _ in 0..2 {
            t.begin_turn("a\n");
            t.begin_command("~", "echo a");
            t.finish_turn("a\n");
        }
        assert_eq!(t.len(), 2);
        assert!(t.entries().all(|e| e.output() == "a\n"));
    }

    #[test]
    fn messages_outside_commands_become_notes_and_unchanged_output_adds_nothing() {
        let mut t = Transcript::new();
        t.begin_turn("");
        t.finish_turn("Lesson loaded!\n");
        t.begin_turn("Lesson loaded!\n");
        t.finish_turn("Lesson loaded!\n");
        assert_eq!(t.len(), 1);
        assert_eq!(t.last().unwrap().kind, EntryKind::Note);
    }

    #[test]
    fn feedback_without_a_command_is_a_note() {
        let mut t = Transcript::new();
        t.begin_turn("x");
        t.feedback(Tone::Info, "Lesson complete.");
        t.finish_turn("x");
        assert_eq!(t.len(), 1);
        let e = t.last().unwrap();
        assert_eq!(e.kind, EntryKind::Note);
        assert_eq!(e.output(), "");
        assert_eq!(e.feedback[0].text, "Lesson complete.");
    }

    #[test]
    fn the_oldest_entries_go_past_the_cap() {
        let mut t = Transcript::with_cap(3);
        for i in 0..5 {
            t.begin_turn("");
            t.begin_command("~", &format!("echo {i}"));
            t.finish_turn(&format!("{i}\n"));
        }
        assert_eq!(t.len(), 3);
        let first = t.entries().next().unwrap();
        assert_eq!(
            first.kind,
            EntryKind::Command { folder: "~".into(), command: "echo 2".into() }
        );
    }

    #[test]
    fn huge_output_keeps_its_last_lines() {
        let mut t = Transcript::new();
        let big: String = (0..MAX_LINES_PER_ENTRY + 10).map(|i| format!("{i}\n")).collect();
        t.begin_command("~", "seq");
        t.finish_turn(&big);
        let lines = text(&t.lines(&theme()));
        assert_eq!(lines.len(), MAX_LINES_PER_ENTRY + 2); // prompt + "not kept" + kept
        assert!(lines[1].contains("10 earlier lines"));
        assert_eq!(lines.last().unwrap(), &format!("{}", MAX_LINES_PER_ENTRY + 9));
    }

    #[test]
    fn view_follows_the_bottom_and_scrolls_back() {
        let mut t = Transcript::new();
        t.begin_command("~", "seq 10");
        t.finish_turn(&(1..=10).map(|i| format!("{i}\n")).collect::<String>());
        let (rows, below) = t.view(&theme(), 40, 3);
        assert_eq!(text(&rows), vec!["8", "9", "10"]);
        assert_eq!(below, 0);

        t.scroll_up(2);
        let (rows, below) = t.view(&theme(), 40, 3);
        assert_eq!(text(&rows), vec!["6", "7", "8"]);
        assert_eq!(below, 2);

        // Can't scroll above the first row
        t.scroll_up(100);
        let (rows, _) = t.view(&theme(), 40, 3);
        assert_eq!(text(&rows)[0], "~ $ seq 10");

        // New output jumps back to the end
        t.begin_command("~", "pwd");
        t.finish_turn("/\n");
        assert_eq!(t.scroll(), 0);
    }

    #[test]
    fn long_lines_wrap_between_words_or_at_the_edge() {
        let rows = wrap_line(&Line::from("abcdefghij"), 4);
        assert_eq!(text(&rows), vec!["abcd", "efgh", "ij"]);
        assert_eq!(text(&wrap_line(&Line::from(""), 4)), vec![""]);
        let rows = wrap_line(&Line::from("stay inside it now"), 10);
        assert_eq!(text(&rows), vec!["stay", "inside it", "now"]);
        // Styles survive the wrap
        let line = Line::from(vec![Span::raw("ab "), Span::styled("cd", Style::default().add_modifier(Modifier::BOLD))]);
        let rows = wrap_line(&line, 3);
        assert_eq!(text(&rows), vec!["ab", "cd"]);
        assert_eq!(rows[1].spans[0].style, Style::default().add_modifier(Modifier::BOLD));
    }
}
