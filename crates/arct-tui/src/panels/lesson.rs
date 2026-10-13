//! Lesson panel - displays interactive lessons and tracks progress

use crate::icons;
use crate::theme::Theme;
use arct_core::{Attempt, Lesson, LessonStep, LessonValidator, PracticeWorld, StepType, ValidationResult};
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

/// Lesson panel state
pub struct LessonPanel {
    pub current_lesson: Option<Lesson>,
    current_step_index: usize,
    user_input: String,
    validator: LessonValidator,
    last_validation: Option<ValidationResult>,
    completed_steps: Vec<usize>,
    /// When this lesson run started (for the "speed_lesson" achievement)
    started_at: Option<std::time::Instant>,
    /// Wrong answers in this lesson run (for the "perfect_lesson" achievement)
    wrong_answers: usize,
    /// Misses on the current step: each one reveals the next hint rung
    step_misses: usize,
    /// The answer was shown at some point in this run
    answer_revealed: bool,
    /// This run was resumed from a save, so its earlier steps are unknown
    resumed: bool,
}

impl LessonPanel {
    pub fn new() -> Self {
        Self {
            current_lesson: None,
            current_step_index: 0,
            user_input: String::new(),
            validator: LessonValidator::new(),
            last_validation: None,
            completed_steps: Vec::new(),
            started_at: None,
            wrong_answers: 0,
            step_misses: 0,
            answer_revealed: false,
            resumed: false,
        }
    }

    /// Load a lesson (fresh run starting at step 0)
    pub fn load_lesson(&mut self, lesson: Lesson) {
        self.current_lesson = Some(lesson);
        self.current_step_index = 0;
        self.user_input.clear();
        self.last_validation = None;
        self.completed_steps.clear();
        self.started_at = Some(std::time::Instant::now());
        self.wrong_answers = 0;
        self.step_misses = 0;
        self.answer_revealed = false;
        self.resumed = false;
    }

    /// Resume a previously started lesson at a saved step.
    ///
    /// Returns the (0-based) step index actually resumed at (clamped to the
    /// lesson's step count).
    pub fn resume_at(&mut self, step_index: usize, completed_steps: Vec<usize>) -> usize {
        if let Some(lesson) = &self.current_lesson {
            let max = lesson.steps.len().saturating_sub(1);
            self.current_step_index = step_index.min(max);
            self.step_misses = 0;
            self.resumed = true;
            self.completed_steps = completed_steps
                .into_iter()
                .filter(|&s| s < lesson.steps.len())
                .collect();
        }
        self.current_step_index
    }

    /// Restart the current lesson from step 0
    pub fn restart(&mut self) {
        self.current_step_index = 0;
        self.user_input.clear();
        self.last_validation = None;
        self.completed_steps.clear();
        self.started_at = Some(std::time::Instant::now());
        self.wrong_answers = 0;
        self.step_misses = 0;
        self.answer_revealed = false;
        self.resumed = false;
    }

    /// Current (0-based) step index
    pub fn current_step_index(&self) -> usize {
        self.current_step_index
    }

    /// Steps completed so far in this run (0-based indices)
    pub fn completed_steps(&self) -> &[usize] {
        &self.completed_steps
    }

    /// Seconds elapsed since this lesson run started
    pub fn elapsed_seconds(&self) -> u64 {
        self.started_at
            .map(|s| s.elapsed().as_secs())
            .unwrap_or(0)
    }

    /// Wrong answers recorded during this lesson run
    pub fn wrong_answers(&self) -> usize {
        self.wrong_answers
    }

    /// This run counts toward mastery: it was played from the start and
    /// the answer was never revealed (hints are fine)
    pub fn mastered_run(&self) -> bool {
        !self.answer_revealed && !self.resumed
    }

    /// The current step is a quiz: what's typed is an answer, not a command
    pub fn expects_answer(&self) -> bool {
        matches!(
            self.current_step().map(|s| &s.step_type),
            Some(StepType::MultipleChoice { .. })
        )
    }

    /// Get current step
    fn current_step(&self) -> Option<&LessonStep> {
        self.current_lesson
            .as_ref()
            .and_then(|lesson| lesson.steps.get(self.current_step_index))
    }

    /// Check if user input is valid for current step (no command output
    /// or practice environment: steps that need them will not pass)
    pub fn validate_current_step(&mut self, input: &str) -> ValidationResult {
        self.validate_step_with(input, "", false, None)
    }

    /// Validate the current step against what was typed, what the command
    /// printed, whether it failed, and the practice environment (for task
    /// steps). Misses climb the hint ladder; the answer is only shown after
    /// the last rung, and never on a challenge step.
    pub fn validate_step_with(
        &mut self,
        input: &str,
        output: &str,
        command_failed: bool,
        world: Option<&dyn PracticeWorld>,
    ) -> ValidationResult {
        let Some(step) = self.current_step().cloned() else {
            return ValidationResult::Failure {
                message: "No active step.".to_string(),
                hint: None,
            };
        };
        let attempt = Attempt { input, output };
        let mut result =
            arct_core::validate_step(&step, &self.validator, &attempt, command_failed, world);

        if !result.is_success() {
            self.wrong_answers += 1;
            self.step_misses += 1;
            if !step.challenge && self.step_misses > step.hint_ladder().len() {
                self.answer_revealed = true;
            }
            result = Self::coach(&step, result, self.step_misses);
        }

        self.last_validation = Some(result.clone());
        result
    }

    /// Turn a raw failure into coaching: a hint per miss, the answer last
    fn coach(step: &LessonStep, raw: ValidationResult, misses: usize) -> ValidationResult {
        let ValidationResult::Failure { message, hint } = raw else {
            return raw;
        };
        // Task-style failures say what is missing without giving the answer
        let outcome_based = matches!(
            step.step_type,
            StepType::Task { .. } | StepType::Practice { .. }
        );
        let gentle = if outcome_based { message.clone() } else { "Not quite.".to_string() };

        if step.challenge {
            return ValidationResult::Failure {
                message: gentle,
                hint: None,
            };
        }
        let ladder = step.hint_ladder();
        if misses <= ladder.len() {
            ValidationResult::Failure {
                message: gentle,
                hint: Some(ladder[misses - 1].to_string()),
            }
        } else {
            // Out of hints: show what the validator knows (the answer)
            ValidationResult::Failure {
                message,
                hint: hint.or_else(|| ladder.last().map(|h| h.to_string())),
            }
        }
    }

    /// Lines showing the last validation result
    fn validation_lines(&self, theme: &Theme) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        match &self.last_validation {
            Some(ValidationResult::Success { message }) => lines.push(Line::from(vec![
                icons::success(),
                Span::styled(message.clone(), theme.style_success()),
            ])),
            Some(ValidationResult::Failure { message, hint }) => {
                lines.push(Line::from(vec![
                    icons::error(),
                    Span::styled(message.clone(), theme.style_error()),
                ]));
                if let Some(h) = hint {
                    lines.push(Line::from(vec![
                        icons::hint(),
                        Span::styled(format!("Hint: {h}"), theme.style_dim()),
                    ]));
                }
            }
            Some(ValidationResult::Partial { message, progress }) => lines.push(Line::from(vec![
                icons::warning(),
                Span::styled(format!("{} ({:.0}%)", message, progress), theme.style_warning()),
            ])),
            None => {}
        }
        lines
    }

    /// Badge line for challenge steps
    fn challenge_line(step: &LessonStep, theme: &Theme) -> Option<Line<'static>> {
        step.challenge.then(|| {
            Line::from(vec![Span::styled(
                "CHALLENGE: on your own, no hints",
                theme.style_warning().add_modifier(ratatui::style::Modifier::BOLD),
            )])
        })
    }

    /// Move to next step
    pub fn next_step(&mut self) -> bool {
        if let Some(lesson) = &self.current_lesson {
            if !self.completed_steps.contains(&self.current_step_index) {
                self.completed_steps.push(self.current_step_index);
            }

            if self.current_step_index + 1 < lesson.steps.len() {
                self.current_step_index += 1;
                self.user_input.clear();
                self.last_validation = None;
                self.step_misses = 0;
                true
            } else {
                false // Lesson complete
            }
        } else {
            false
        }
    }

    /// Move to previous step
    pub fn previous_step(&mut self) {
        if self.current_step_index > 0 {
            self.current_step_index -= 1;
            self.user_input.clear();
            self.last_validation = None;
            self.step_misses = 0;
        }
    }

    /// Get completion percentage
    pub fn completion_percentage(&self) -> f32 {
        if let Some(lesson) = &self.current_lesson {
            let total = lesson.steps.len();
            if total == 0 {
                return 0.0;
            }
            (self.completed_steps.len() as f32 / total as f32) * 100.0
        } else {
            0.0
        }
    }

    /// Header line describing where lesson commands execute
    fn practice_mode_line(practice_real: bool, theme: &Theme) -> Line<'static> {
        if practice_real {
            Line::from(vec![Span::styled(
                "REAL FILES — ~/ArcAcademy/playground",
                theme
                    .style_warning()
                    .add_modifier(ratatui::style::Modifier::BOLD),
            )])
        } else {
            Line::from(vec![Span::styled(
                "SIMULATED SANDBOX",
                theme.style_dim(),
            )])
        }
    }

    /// Render the lesson panel.
    ///
    /// `practice_real` selects the header badge: real-filesystem playground
    /// vs the simulated sandbox.
    pub fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        focused: bool,
        theme: &Theme,
        practice_real: bool,
    ) {
        let border_style = if focused {
            theme.style_border_focused()
        } else {
            theme.style_border()
        };

        let title_style = theme.style_title(focused);

        if let Some(lesson) = &self.current_lesson {
            // Render current step (includes header info in title)
            if let Some(step) = self.current_step() {
                self.render_step(
                    frame,
                    area,
                    lesson,
                    step,
                    theme,
                    border_style,
                    title_style,
                    practice_real,
                );
            }
        } else {
            // No lesson loaded - show lesson selection screen
            self.render_lesson_selection(
                frame,
                area,
                theme,
                border_style,
                title_style,
                practice_real,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_step(
        &self,
        frame: &mut Frame,
        area: Rect,
        lesson: &Lesson,
        step: &LessonStep,
        theme: &Theme,
        border_style: Style,
        title_style: Style,
        practice_real: bool,
    ) {
        let mut lines = Vec::new();

        // Practice-mode header: where do the commands actually run?
        lines.push(Self::practice_mode_line(practice_real, theme));

        // Step title - always show
        lines.push(Line::from(vec![
            Span::styled(format!("Step {}: ", step.step_number), theme.style_accent()),
            Span::styled(&step.title, theme.style_header()),
        ]));

        // Render based on step type
        match &step.step_type {
            StepType::CommandExercise { .. } => {
                // HOW TO instruction
                lines.push(Line::from(vec![
                    Span::styled("▶ Type command in Shell → Enter", theme.style_warning()),
                ]));
                // Task instruction
                if !step.instruction.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("Task: ", theme.style_accent()),
                        Span::styled(&step.instruction, theme.style_normal()),
                    ]));
                }
                lines.extend(Self::challenge_line(step, theme));
                // Hints are earned: they appear after a miss
                lines.extend(self.validation_lines(theme));
            }
            StepType::MultipleChoice {
                question,
                options,
                explanation,
                ..
            } => {
                // HOW TO + Question combined
                lines.push(Line::from(vec![
                    Span::styled(format!("▶ Type 0-{} → ", options.len() - 1), theme.style_warning()),
                    icons::question(),
                    Span::styled(question, theme.style_normal()),
                ]));
                // Options
                for (i, option) in options.iter().enumerate() {
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {}. ", i), theme.style_accent()),
                        Span::styled(option, theme.style_normal()),
                    ]));
                }
                // Show explanation if answered correctly
                if let Some(ValidationResult::Success { .. }) = &self.last_validation {
                    lines.push(Line::from(vec![
                        icons::success(),
                        Span::styled(explanation, theme.style_success()),
                    ]));
                }
            }
            StepType::Information { content } => {
                // HOW TO
                lines.push(Line::from(vec![
                    Span::styled("▶ Press Enter to continue", theme.style_warning()),
                ]));
                // Display information
                for line in content.lines() {
                    lines.push(Line::from(vec![Span::styled(line, theme.style_normal())]));
                }
            }
            StepType::FillInBlank { template, .. } => {
                // HOW TO
                lines.push(Line::from(vec![
                    Span::styled("▶ Fill blank, type in Shell → Enter", theme.style_warning()),
                ]));
                lines.push(Line::from(vec![
                    icons::note(),
                    Span::styled(&step.instruction, theme.style_normal()),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("Template: ", theme.style_accent()),
                    Span::styled(template, theme.style_dim()),
                ]));
                lines.extend(self.validation_lines(theme));
            }
            StepType::Practice { goal, .. } | StepType::Task { goal, .. } => {
                // Any commands that reach the goal count
                lines.push(Line::from(vec![
                    Span::styled("▶ Use any commands you like → ", theme.style_warning()),
                    icons::target(),
                    Span::styled(goal, theme.style_normal()),
                ]));
                if !step.instruction.is_empty() && step.instruction != *goal {
                    lines.push(Line::from(vec![Span::styled(
                        &step.instruction,
                        theme.style_normal(),
                    )]));
                }
                lines.extend(Self::challenge_line(step, theme));
                lines.extend(self.validation_lines(theme));
            }
        }

        // Build title with progress info (compact)
        let progress = self.completion_percentage();
        let title = format!(
            " {} {}/{} | {:.0}% ",
            lesson.title,
            self.current_step_index + 1,
            lesson.steps.len(),
            progress
        );

        let block = Block::default()
            .title(title)
            .title_style(title_style)
            .borders(Borders::ALL)
            .border_style(border_style)
            .style(theme.style_block());

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
    }

    fn render_lesson_selection(
        &self,
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        border_style: Style,
        title_style: Style,
        practice_real: bool,
    ) {
        let block = Block::default()
            .title(format!(" {}Interactive Lessons ", icons::lesson().content))
            .title_style(title_style)
            .borders(Borders::ALL)
            .border_style(border_style)
            .style(theme.style_block());  // Set background for light themes

        let paragraph = Paragraph::new(vec![
            Self::practice_mode_line(practice_real, theme),
            Line::from(vec![
                icons::welcome(),
                Span::styled("Welcome to Interactive Lessons!", theme.style_accent()),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("", theme.style_accent()),
                Span::styled("10 comprehensive lessons", theme.style_normal()),
                Span::styled(" available", theme.style_dim()),
            ]),
            Line::from(vec![
                Span::styled("", theme.style_accent()),
                Span::styled("Track progress & earn achievements", theme.style_normal()),
            ]),
            Line::from(vec![
                Span::styled("", theme.style_accent()),
                Span::styled("Safe virtual filesystem", theme.style_normal()),
                Span::styled(" for hands-on practice", theme.style_dim()),
            ]),
            Line::from(""),
            Line::from(""),
            Line::from(vec![
                Span::styled("Select a lesson:", theme.style_header()),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Press ", theme.style_dim()),
                Span::styled("m", theme.style_accent()),
                Span::styled(" to open the lesson menu", theme.style_dim()),
            ]),
            Line::from(vec![
                Span::styled("  Use ", theme.style_dim()),
                Span::styled("↑/↓", theme.style_accent()),
                Span::styled(" or ", theme.style_dim()),
                Span::styled("1-9,0", theme.style_accent()),
                Span::styled(" to select", theme.style_dim()),
            ]),
            Line::from(vec![
                Span::styled("  Press ", theme.style_dim()),
                Span::styled("Enter", theme.style_accent()),
                Span::styled(" to start learning!", theme.style_dim()),
            ]),
            Line::from(""),
            Line::from(""),
            Line::from(vec![
                Span::styled("Tip: ", theme.style_accent()),
                Span::styled("Complete lessons to unlock achievements", theme.style_dim()),
            ]),
            Line::from(vec![
                Span::styled("       and build your learning streak!", theme.style_dim()),
            ]),
        ])
        .block(block)
        .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }
}

impl Default for LessonPanel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod ladder_tests {
    use super::*;
    use arct_core::{CommandValidation, Difficulty, Track};

    fn lesson(challenge: bool) -> Lesson {
        Lesson {
            id: "t".into(),
            title: "T".into(),
            description: String::new(),
            difficulty: Difficulty::Beginner,
            estimated_minutes: 1,
            steps: vec![LessonStep {
                step_number: 1,
                title: "pwd".into(),
                instruction: "Where are you?".into(),
                hint: None,
                step_type: StepType::CommandExercise {
                    expected_command: "pwd".into(),
                    validation: CommandValidation::CommandOnly,
                    success_message: "Yes".into(),
                },
                hints: vec!["It prints the folder you're in.".into(), "p, w, d".into()],
                challenge,
                solution: vec![],
            }],
            prerequisites: vec![],
            tags: vec![],
            setup: vec![],
            track: Track::Explorer,
        }
    }

    fn failure(r: &ValidationResult) -> (String, Option<String>) {
        match r {
            ValidationResult::Failure { message, hint } => (message.clone(), hint.clone()),
            other => panic!("expected failure, got {other:?}"),
        }
    }

    #[test]
    fn each_miss_reveals_the_next_rung_then_the_answer() {
        let mut panel = LessonPanel::new();
        panel.load_lesson(lesson(false));

        let (msg, hint) = failure(&panel.validate_current_step("ls"));
        assert_eq!(msg, "Not quite.");
        assert_eq!(hint.as_deref(), Some("It prints the folder you're in."));

        let (msg, hint) = failure(&panel.validate_current_step("ls"));
        assert_eq!(msg, "Not quite.");
        assert_eq!(hint.as_deref(), Some("p, w, d"));

        let (msg, _) = failure(&panel.validate_current_step("ls"));
        assert!(msg.contains("pwd"), "third miss shows the answer: {msg}");

        assert!(panel.validate_current_step("pwd").is_success());
    }

    #[test]
    fn hints_keep_mastery_the_answer_does_not() {
        let mut panel = LessonPanel::new();
        panel.load_lesson(lesson(false));
        panel.validate_current_step("ls");
        panel.validate_current_step("ls");
        assert!(panel.mastered_run(), "two hint rungs still count");
        panel.validate_current_step("ls");
        assert!(!panel.mastered_run(), "seeing the answer doesn't");

        panel.restart();
        assert!(panel.mastered_run(), "a fresh run can earn it again");
    }

    #[test]
    fn challenge_misses_keep_mastery_and_resumed_runs_dont_count() {
        let mut panel = LessonPanel::new();
        panel.load_lesson(lesson(true));
        for _ in 0..5 {
            panel.validate_current_step("ls");
        }
        assert!(panel.mastered_run(), "a challenge never shows the answer");

        panel.load_lesson(lesson(false));
        panel.resume_at(0, vec![]);
        assert!(!panel.mastered_run());
    }

    #[test]
    fn challenge_steps_never_give_the_answer() {
        let mut panel = LessonPanel::new();
        panel.load_lesson(lesson(true));
        for _ in 0..5 {
            let (msg, hint) = failure(&panel.validate_current_step("ls"));
            assert!(!msg.contains("pwd"), "{msg}");
            assert!(!hint.unwrap_or_default().contains("pwd"));
        }
    }

    #[test]
    fn misses_reset_on_a_new_step() {
        let mut panel = LessonPanel::new();
        let mut l = lesson(false);
        let mut second = l.steps[0].clone();
        second.step_number = 2;
        l.steps.push(second);
        panel.load_lesson(l);
        let _ = panel.validate_current_step("ls");
        let _ = panel.validate_current_step("ls");
        assert!(panel.validate_current_step("pwd").is_success());
        panel.next_step();
        let (_, hint) = failure(&panel.validate_current_step("ls"));
        assert_eq!(hint.as_deref(), Some("It prints the folder you're in."));
    }
}
