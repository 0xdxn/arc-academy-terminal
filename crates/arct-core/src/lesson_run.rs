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
    practice_checks, validate_step, Attempt, Check, Lesson, LessonStep, LessonValidator,
    PracticeWorld, StepType, ValidationResult,
};
use crate::sandbox::{Marks, Sandbox, ShellState};
use crate::shell_syntax::Redirect;
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

        if !result.is_success() && is_look_around(step, attempt.input) {
            // Looking isn't a wrong answer: say what's still missing, but
            // don't climb the ladder or count it against the run
            if let ValidationResult::Failure { message, .. } = result {
                result = ValidationResult::Failure { message, hint: None };
            }
        } else if !result.is_success() {
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

/// Programs that show where things are, never what's in them
const FINDING: &[&str] = &[
    "ls", "pwd", "cd", "tree", "find", "file", "stat", "du", "df", "which", "type", "whoami", "id",
    "hostname", "uname", "date", "history", "clear", "help", "man",
];

/// Programs that show what's in files or the shell, and change nothing
const READING: &[&str] = &[
    "cat", "less", "more", "head", "tail", "grep", "wc", "sort", "uniq", "cut", "diff", "cmp", "echo",
    "printf", "env", "printenv",
];

/// find options that write, delete or run something
const FIND_ACTIONS: &[&str] = &["-delete", "-exec", "-execdir", "-ok", "-okdir", "-fprint", "-fprint0", "-fprintf", "-fls"];

/// The programs a line runs, in order; `None` when the line can't be read
/// or does more than run programs (assignments, writing to a file)
fn looking_programs(line: &str) -> Option<Vec<(String, Vec<String>)>> {
    let list = crate::shell_syntax::parse(line, &|_| None).ok()?;
    let mut programs = Vec::new();
    for (_, pipeline) in list {
        for stage in pipeline.stages {
            if !stage.assignments.is_empty() {
                return None;
            }
            let writes = stage.redirects.iter().any(|r| match r {
                Redirect::Out { target, .. } | Redirect::Err { target, .. } => target != "/dev/null",
                Redirect::ErrToOut | Redirect::In(_) => false,
            });
            if writes {
                return None;
            }
            let mut words = stage.words.into_iter().map(|w| w.text);
            let program = words.next()?;
            programs.push((program, words.collect()));
        }
    }
    Some(programs)
}

/// A line that only looks around (ls, cd, cat ...) on a step judged by
/// its outcome, where that kind of looking can't be the answer. It
/// doesn't count as a miss. Looking is the answer when the step checks
/// for that program (`Used`) or its solution runs it, when it's `cd` and
/// the step checks where you are, or when it reads and the step checks
/// what a command shows.
fn is_look_around(step: &LessonStep, line: &str) -> bool {
    let checks = match &step.step_type {
        StepType::Task { checks, .. } => checks.clone(),
        StepType::Practice { validation, .. } => practice_checks(validation),
        _ => return false,
    };
    let Some(programs) = looking_programs(line) else {
        return false;
    };
    if programs.is_empty() {
        return false;
    }
    let solution: Vec<String> = step
        .solution
        .iter()
        .filter_map(|s| looking_programs(s))
        .flatten()
        .map(|(program, _)| program)
        .collect();
    let judges_output = checks.iter().any(|c| matches!(c, Check::OutputContains(_)));
    let judges_cwd = checks.iter().any(|c| matches!(c, Check::CwdIs(_)));

    programs.iter().all(|(program, args)| {
        let program = program.as_str();
        let checked = checks.iter().any(|c| matches!(c, Check::Used(p) if p == program))
            || solution.iter().any(|p| p == program);
        if checked {
            return false;
        }
        if program == "find" && args.iter().any(|a| FIND_ACTIONS.contains(&a.as_str())) {
            return false;
        }
        if program == "cd" {
            return !judges_cwd;
        }
        if FINDING.contains(&program) {
            return true;
        }
        READING.contains(&program) && !judges_output
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn task(checks: Vec<Check>, solution: &[&str]) -> LessonStep {
        LessonStep {
            step_number: 1,
            title: "t".into(),
            instruction: String::new(),
            hint: None,
            step_type: StepType::Task {
                goal: String::new(),
                checks,
                success_message: String::new(),
            },
            hints: vec![],
            challenge: false,
            solution: solution.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn looking_around_a_world_task_is_not_an_answer() {
        let step = task(vec![Check::FileExists("bakery.tar.gz".into())], &["tar -czf bakery.tar.gz bakery"]);
        for line in [
            "ls",
            "ls -la bakery",
            "pwd",
            "cd bakery",
            "cd ..",
            "cat bakery/notes.txt",
            "ls | wc -l",
            "ls nope 2>/dev/null",
            "cd bakery; ls; cd ..",
            "find . -name '*.txt'",
            "grep -r Oven bakery",
        ] {
            assert!(is_look_around(&step, line), "{line} only looks");
        }
        for line in [
            "",
            "tar -cf bakery.tar bakery",
            "mkdir bakery.tar.gz",
            "ls > list.txt",
            "cat notes.txt >> bakery.tar.gz",
            "ls 2> errors.txt",
            "ls; touch x",
            "ls | tee out.txt",
            "find . -name '*.txt' -delete",
            "find . -exec rm {} ;",
            "X=1 ls",
            "frobnicate",
            "$(ls)",
        ] {
            assert!(!is_look_around(&step, line), "{line} is an attempt");
        }
    }

    #[test]
    fn looking_is_the_answer_when_the_step_judges_it() {
        // Reading is how an output step is answered; listing still only looks
        let read = task(vec![Check::OutputContains("Pip".into())], &["cat station/vents/.stowaway.txt"]);
        assert!(!is_look_around(&read, "cat station/vents/stowaway.txt"));
        assert!(!is_look_around(&read, "head station/vents/.stowaway.txt"));
        assert!(is_look_around(&read, "ls -a station/vents"));
        assert!(is_look_around(&read, "cd station"));

        // The solution lists: a listing is an attempt
        let list = task(vec![Check::OutputContains(".stowaway".into())], &["ls -a station/vents"]);
        assert!(!is_look_around(&list, "ls station/vents"));

        // Checked for by name
        let used = task(vec![Check::Used("find".into()), Check::OutputContains("x".into())], &[]);
        assert!(!is_look_around(&used, "find . -name x"));
        assert!(is_look_around(&used, "ls"));

        // Where you are is checked: cd is an attempt, ls still looks
        let cwd = task(vec![Check::CwdIs("station".into())], &["cd station"]);
        assert!(!is_look_around(&cwd, "cd statoin"));
        let cwd_no_solution = task(vec![Check::CwdIs("station".into())], &[]);
        assert!(!is_look_around(&cwd_no_solution, "cd statoin"));
        assert!(is_look_around(&cwd_no_solution, "ls"));
    }

    #[test]
    fn only_outcome_steps_have_look_arounds() {
        let mut step = task(vec![], &[]);
        step.step_type = StepType::CommandExercise {
            expected_command: "pwd".into(),
            validation: crate::lesson::CommandValidation::CommandOnly,
            success_message: String::new(),
        };
        assert!(!is_look_around(&step, "ls"));
    }
}
