//! Lesson panel - displays interactive lessons and tracks progress

use crate::icons;
use crate::theme::Theme;
use arct_core::{Lesson, LessonStep, StepType, ValidationResult, LessonValidator};
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
    }

    /// Resume a previously started lesson at a saved step.
    ///
    /// Returns the (0-based) step index actually resumed at (clamped to the
    /// lesson's step count).
    pub fn resume_at(&mut self, step_index: usize, completed_steps: Vec<usize>) -> usize {
        if let Some(lesson) = &self.current_lesson {
            let max = lesson.steps.len().saturating_sub(1);
            self.current_step_index = step_index.min(max);
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

    /// Get current step
    fn current_step(&self) -> Option<&LessonStep> {
        self.current_lesson
            .as_ref()
            .and_then(|lesson| lesson.steps.get(self.current_step_index))
    }

    /// Check if user input is valid for current step
    pub fn validate_current_step(&mut self, input: &str) -> ValidationResult {
        if let Some(step) = self.current_step() {
            let result = match &step.step_type {
                StepType::CommandExercise {
                    expected_command,
                    validation,
                    success_message,
                } => {
                    let validation_result =
                        self.validator.validate_command(input, expected_command, validation);

                    if validation_result.is_success() {
                        ValidationResult::Success {
                            message: success_message.clone(),
                        }
                    } else {
                        validation_result
                    }
                }
                StepType::MultipleChoice {
                    correct_index, ..
                } => {
                    if let Ok(choice) = input.parse::<usize>() {
                        self.validator.validate_multiple_choice(choice, *correct_index)
                    } else {
                        ValidationResult::Failure {
                            message: "Please enter a number.".to_string(),
                            hint: None,
                        }
                    }
                }
                StepType::Information { .. } => {
                    // Information steps just need any key press to continue
                    ValidationResult::Success {
                        message: "Continue to next step.".to_string(),
                    }
                }
                _ => ValidationResult::Success {
                    message: "Continue.".to_string(),
                },
            };

            // Track wrong answers for the "perfect_lesson" achievement
            // (Information steps never fail, so this only counts real misses)
            if !result.is_success() {
                self.wrong_answers += 1;
            }

            self.last_validation = Some(result.clone());
            result
        } else {
            ValidationResult::Failure {
                message: "No active step.".to_string(),
                hint: None,
            }
        }
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
                // Hint
                if let Some(hint) = &step.hint {
                    lines.push(Line::from(vec![
                        icons::hint(),
                        Span::styled(hint, theme.style_dim()),
                    ]));
                }
                // Validation result
                if let Some(validation) = &self.last_validation {
                    match validation {
                        ValidationResult::Success { message } => {
                            lines.push(Line::from(vec![
                                icons::success(),
                                Span::styled(message, theme.style_success()),
                            ]));
                        }
                        ValidationResult::Failure { message, hint } => {
                            lines.push(Line::from(vec![
                                icons::error(),
                                Span::styled(message, theme.style_error()),
                            ]));
                            if let Some(h) = hint {
                                lines.push(Line::from(vec![
                                    icons::hint(),
                                    Span::styled(h, theme.style_dim()),
                                ]));
                            }
                        }
                        ValidationResult::Partial { message, progress } => {
                            lines.push(Line::from(vec![
                                icons::warning(),
                                Span::styled(
                                    format!("{} ({:.0}%)", message, progress),
                                    theme.style_warning(),
                                ),
                            ]));
                        }
                    }
                }
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
            }
            StepType::Practice { goal, hints, .. } => {
                // HOW TO + Goal combined
                lines.push(Line::from(vec![
                    Span::styled("▶ Try commands → ", theme.style_warning()),
                    icons::target(),
                    Span::styled(goal, theme.style_normal()),
                ]));
                // Hints inline
                if !hints.is_empty() {
                    for hint in hints {
                        lines.push(Line::from(vec![
                            icons::hint(),
                            Span::styled(hint, theme.style_dim()),
                        ]));
                    }
                }
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
