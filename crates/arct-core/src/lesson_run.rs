//! Playing a lesson, with no user interface: the hint ladder ([`Coach`])
//! and a whole lesson run in the simulated sandbox ([`LessonRun`]).
//!
//! The terminal app's lesson panel keeps its own step position and uses
//! [`Coach`] for hints; the browser plays a lesson with [`LessonRun`],
//! which does what the terminal app does when Enter is pressed in lesson
//! mode: run the line in the sandbox (unless it answers a quiz), then
//! check the step against what was typed, what it printed and the
//! practice files.

use crate::command::CommandAnalyzer;
use crate::guard::{GuardVerdict, PlaygroundGuard};
use crate::lesson::{
    validate_step, Attempt, Lesson, LessonStep, LessonValidator, PracticeWorld, StepType,
    ValidationResult,
};
use crate::sandbox::{Marks, Sandbox, ShellState};
use crate::virtual_fs::VirtualFileSystem;

/// The hint ladder for one lesson run: each miss on a step reveals the
/// next hint; the answer only after the last one, and never on a
/// challenge step
#[derive(Debug, Clone, Default)]
pub struct Coach {
    /// Wrong answers in this lesson run (for the "perfect_lesson" achievement)
    wrong_answers: usize,
    /// Misses on the current step: each one reveals the next hint rung
    step_misses: usize,
    /// The answer was shown at some point in this run
    answer_revealed: bool,
}

impl Coach {
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate a step against what was typed, what the command printed,
    /// whether it failed, and the practice environment (for task steps).
    /// A miss climbs the hint ladder.
    pub fn check(
        &mut self,
        step: &LessonStep,
        validator: &LessonValidator,
        attempt: &Attempt,
        command_failed: bool,
        world: Option<&dyn PracticeWorld>,
    ) -> ValidationResult {
        let mut result = validate_step(step, validator, attempt, command_failed, world);

        if !result.is_success() {
            self.wrong_answers += 1;
            self.step_misses += 1;
            if !step.challenge && self.step_misses > step.hint_ladder().len() {
                self.answer_revealed = true;
            }
            result = coach(step, result, self.step_misses);
        }
        result
    }

    /// A new step: its hints start from the first rung
    pub fn new_step(&mut self) {
        self.step_misses = 0;
    }

    /// Wrong answers so far in this run
    pub fn wrong_answers(&self) -> usize {
        self.wrong_answers
    }

    /// The answer was shown at some point in this run
    pub fn answer_revealed(&self) -> bool {
        self.answer_revealed
    }
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

/// What one submitted line did
#[derive(Debug, Clone)]
pub struct Turn {
    /// The current folder when the line was typed, as a prompt shows it
    /// (`~`, `~/station`)
    pub prompt_cwd: String,
    /// The line as typed
    pub command: String,
    /// What the command printed (nothing for a quiz answer)
    pub output: String,
    /// How the step check went; `None` when the line wasn't checked (it
    /// couldn't be read as a command, or the lesson was already finished)
    pub result: Option<ValidationResult>,
    /// This line finished the lesson
    pub finished: bool,
}

/// One lesson played in a sandbox of its own
pub struct LessonRun {
    lesson: Lesson,
    step: usize,
    done: bool,
    validator: LessonValidator,
    coach: Coach,
    analyzer: CommandAnalyzer,
    marks: Marks,
    world: VirtualFileSystem,
    shell: ShellState,
}

impl LessonRun {
    /// Start `lesson` from its first step, with its practice files held in
    /// memory
    pub fn new(lesson: Lesson) -> anyhow::Result<Self> {
        Self::with_world(lesson, VirtualFileSystem::for_lesson_in_memory)
    }

    /// Start `lesson` with practice files made by `make` (in memory, or on
    /// disk for comparing the two)
    pub fn with_world(
        lesson: Lesson,
        make: impl Fn(&Lesson) -> anyhow::Result<VirtualFileSystem>,
    ) -> anyhow::Result<Self> {
        let world = make(&lesson)?;
        Ok(Self {
            lesson,
            step: 0,
            done: false,
            validator: LessonValidator::new(),
            coach: Coach::new(),
            analyzer: CommandAnalyzer::new(),
            marks: Marks::plain(),
            world,
            shell: ShellState::new(),
        })
    }

    /// Use these marks for messages from now on
    pub fn set_marks(&mut self, marks: Marks) {
        self.marks = marks;
    }

    pub fn lesson(&self) -> &Lesson {
        &self.lesson
    }

    /// The current step (the last one once the lesson is finished)
    pub fn step(&self) -> &LessonStep {
        &self.lesson.steps[self.step.min(self.lesson.steps.len().saturating_sub(1))]
    }

    /// 0-based index of the current step
    pub fn step_index(&self) -> usize {
        self.step
    }

    /// Every step has passed
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// The run never needed the answer (hints are fine)
    pub fn mastered(&self) -> bool {
        !self.coach.answer_revealed()
    }

    /// The practice files
    pub fn world(&self) -> &VirtualFileSystem {
        &self.world
    }

    /// The current folder as a prompt shows it: `~`, `~/station`
    pub fn display_cwd(&self) -> String {
        let rel = self.world.cwd_relative();
        if rel.is_empty() {
            "~".to_string()
        } else {
            format!("~/{rel}")
        }
    }

    /// Run one typed line and check the step with it
    pub fn submit(&mut self, line: &str) -> Turn {
        let prompt_cwd = self.display_cwd();
        let mut turn = Turn {
            prompt_cwd,
            command: line.to_string(),
            output: String::new(),
            result: None,
            finished: false,
        };
        if self.done || self.lesson.steps.is_empty() {
            return turn;
        }
        let step = self.step().clone();

        let result = if line.is_empty() {
            // Enter on its own: continues past information, nothing else
            self.coach.check(&step, &self.validator, &Attempt { input: line, output: "" }, false, None)
        } else {
            if let Err(e) = self.analyzer.parse(line) {
                turn.output = self.marks.error(e);
                return turn;
            }
            // A quiz answer ("1") is not a command: don't run it
            let answering = matches!(step.step_type, StepType::MultipleChoice { .. });
            let output = if answering {
                None
            } else if let GuardVerdict::Refuse { reason } = PlaygroundGuard::check_catastrophic(line) {
                Some(format!("{}Blocked. Nothing ran.\n\n{}\n", self.marks.warning, reason))
            } else {
                let mut sandbox = Sandbox::new(&mut self.shell, &mut self.world, &self.analyzer, &self.marks);
                Some(sandbox.run_line(line))
            };
            let output = output.unwrap_or_default();
            let failed = self.marks.reports_failure(&output);
            let attempt = Attempt { input: line, output: &output };
            let result = self.coach.check(&step, &self.validator, &attempt, failed, Some(&self.world));
            turn.output = output;
            result
        };

        if result.is_success() {
            if self.step + 1 < self.lesson.steps.len() {
                self.step += 1;
                self.coach.new_step();
            } else {
                self.done = true;
                turn.finished = true;
            }
        }
        turn.result = Some(result);
        turn
    }
}
