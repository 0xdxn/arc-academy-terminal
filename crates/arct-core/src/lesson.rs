//! Interactive lesson system for teaching Linux/Bash concepts
//!
//! This module provides a comprehensive framework for creating, validating,
//! and tracking progress through interactive lessons.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A complete lesson module (e.g., "Navigation Basics")
///
/// # TOML lesson-pack format
///
/// Lessons can be authored as `*.toml` files (loaded from
/// `~/.config/arct/lessons/` via [`LessonLibrary::load_from_dir`]). The
/// top-level fields map 1:1 onto this struct:
///
/// ```toml
/// id = "my-lesson"
/// title = "My Lesson"
/// description = "What this lesson teaches."
/// difficulty = "Beginner"          # Beginner | Intermediate | Advanced | Expert
/// estimated_minutes = 10
/// prerequisites = ["nav-basics"]   # lesson ids to complete first
/// tags = ["beginner", "files"]
///
/// # Optional starter files, materialized into the practice environment
/// # before the lesson starts (omit entirely if the lesson needs none).
/// # In simulated mode they are seeded into the virtual sandbox filesystem;
/// # in real practice mode they are written under
/// # ~/ArcAcademy/playground/<lesson-id>/ and the session cd's there.
/// # `path` is relative to the lesson's practice directory (subdirectories
/// # are created as needed; absolute paths and `..` are rejected).
/// [[setup]]
/// path = "notes.txt"
/// contents = "starter content\n"
///
/// [[setup]]
/// path = "logs/server.log"
/// contents = "line 1\nline 2\n"
///
/// [[steps]]
/// step_number = 1
/// title = "First step"
/// instruction = "Type the command..."
/// hint = "Try 'pwd'"
///
/// [steps.step_type.CommandExercise]
/// expected_command = "pwd"
/// validation = "CommandOnly"
/// success_message = "Nice!"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lesson {
    pub id: String,
    pub title: String,
    pub description: String,
    pub difficulty: Difficulty,
    pub estimated_minutes: u32,
    pub steps: Vec<LessonStep>,
    pub prerequisites: Vec<String>, // IDs of lessons that should be completed first
    pub tags: Vec<String>,          // e.g., ["beginner", "navigation", "essential"]
    /// Optional starter files for the lesson's practice environment.
    ///
    /// Defaults to empty, so existing lesson-pack TOML files without a
    /// `setup` array keep parsing unchanged.
    #[serde(default)]
    pub setup: Vec<SetupFile>,
    /// Which learning track the lesson belongs to. Defaults to Operator so
    /// existing lesson packs keep their place.
    #[serde(default)]
    pub track: Track,
}

/// Learning track: Explorer is written for younger learners (story-led,
/// short steps, a hint ladder); Operator is the deeper adult path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Track {
    Explorer,
    #[default]
    Operator,
}

/// A starter file materialized into a lesson's practice environment.
///
/// In TOML lesson packs this is an entry in the `[[setup]]` array with a
/// relative `path` and the literal file `contents`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupFile {
    /// Path relative to the lesson's practice directory (no `..`, not absolute).
    pub path: String,
    /// Full file contents to write.
    pub contents: String,
}

impl Lesson {
    /// Serialize this lesson to a TOML document (the on-disk lesson format).
    pub fn to_toml(&self) -> anyhow::Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// Parse a lesson from a TOML document.
    pub fn from_toml(content: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(content)?)
    }
}

/// Difficulty level of a lesson
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Difficulty {
    Beginner,
    Intermediate,
    Advanced,
    Expert,
}

/// A single step within a lesson
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LessonStep {
    pub step_number: u32,
    pub title: String,
    pub instruction: String,
    /// Single hint (older lessons). Shown after the first miss.
    pub hint: Option<String>,
    pub step_type: StepType,
    /// Hint ladder: one more rung is revealed after each wrong attempt.
    /// After the last rung the answer is shown (never on challenge steps).
    #[serde(default)]
    pub hints: Vec<String>,
    /// A challenge step gives no hints and never reveals the answer.
    #[serde(default)]
    pub challenge: bool,
    /// Commands that complete this step, in order. Not shown to learners;
    /// the test suite plays every built-in lesson with them. Needed for
    /// task steps; other steps fall back to their expected answer.
    #[serde(default)]
    pub solution: Vec<String>,
}

impl LessonStep {
    /// The hint ladder for this step (falls back to the single `hint`)
    pub fn hint_ladder(&self) -> Vec<&str> {
        if !self.hints.is_empty() {
            self.hints.iter().map(String::as_str).collect()
        } else {
            self.hint.iter().map(String::as_str).collect()
        }
    }
}

/// Type of lesson step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StepType {
    /// User must execute a specific command
    CommandExercise {
        expected_command: String,
        validation: CommandValidation,
        success_message: String,
    },
    /// Multiple choice question
    MultipleChoice {
        question: String,
        options: Vec<String>,
        correct_index: usize,
        explanation: String,
    },
    /// Fill in the blank in a command
    FillInBlank {
        template: String,       // e.g., "ls {flags} /home"
        correct_answers: Vec<String>, // e.g., ["-la", "-l -a"]
        explanation: String,
    },
    /// Information/explanation only (no validation)
    Information {
        content: String,
    },
    /// Free-form practice with validation
    Practice {
        goal: String,
        validation: PracticeValidation,
        hints: Vec<String>,
    },
    /// Reach a goal with any commands: passes when every check holds in the
    /// practice environment, however the learner got there
    Task {
        goal: String,
        checks: Vec<Check>,
        success_message: String,
    },
}

/// One condition on the practice environment. Paths are relative to the
/// lesson's home directory (where the lesson starts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Check {
    /// A directory exists
    DirExists(String),
    /// A file exists
    FileExists(String),
    /// Nothing exists at this path
    Missing(String),
    /// A file exists and contains the text
    FileContains { path: String, text: String },
    /// The current directory is this path ("" is the lesson home)
    CwdIs(String),
    /// The last command's output contains the text
    OutputContains(String),
    /// The last command ran this program (e.g. "grep")
    Used(String),
}

/// The practice environment a lesson runs in, as seen by outcome checks
pub trait PracticeWorld {
    /// Real path for a path relative to the lesson home, if it stays inside
    fn locate(&self, lesson_path: &str) -> Option<std::path::PathBuf>;
    /// Current directory relative to the lesson home ("" at the home)
    fn cwd_relative(&self) -> String;
}

/// What the last command did, for outcome checks
pub struct Attempt<'a> {
    pub input: &'a str,
    pub output: &'a str,
}

/// Evaluate checks in order; the first one that fails explains what's
/// still missing, in words a learner can act on
pub fn evaluate_checks(
    checks: &[Check],
    world: Option<&dyn PracticeWorld>,
    attempt: &Attempt,
) -> Result<(), String> {
    let trim = |p: &str| p.trim_matches('/').to_string();
    for check in checks {
        let ok = match check {
            Check::DirExists(path) => world
                .and_then(|w| w.locate(path))
                .is_some_and(|p| p.is_dir()),
            Check::FileExists(path) => world
                .and_then(|w| w.locate(path))
                .is_some_and(|p| p.is_file()),
            Check::Missing(path) => world
                .and_then(|w| w.locate(path))
                .is_some_and(|p| !p.exists()),
            Check::FileContains { path, text } => world
                .and_then(|w| w.locate(path))
                .and_then(|p| std::fs::read_to_string(p).ok())
                .is_some_and(|c| c.contains(text.as_str())),
            Check::CwdIs(path) => world.is_some_and(|w| trim(&w.cwd_relative()) == trim(path)),
            Check::OutputContains(text) => attempt.output.contains(text.as_str()),
            Check::Used(program) => attempt
                .input
                .split(|c| c == '|' || c == ';' || c == '&')
                .any(|seg| seg.split_whitespace().next() == Some(program.as_str())),
        };
        if !ok {
            return Err(match check {
                Check::DirExists(p) => format!("Not yet: there's no folder called '{p}'."),
                Check::FileExists(p) => format!("Not yet: there's no file called '{p}'."),
                Check::Missing(p) => format!("Not yet: '{p}' is still there."),
                Check::FileContains { path, text } => {
                    format!("Not yet: '{path}' doesn't say \"{text}\" yet.")
                }
                Check::CwdIs(p) if p.is_empty() => "Not yet: go back to where you started.".to_string(),
                Check::CwdIs(p) => format!("Not yet: you need to be inside '{p}'."),
                Check::OutputContains(_) => "Not yet: that didn't show what we're looking for.".to_string(),
                Check::Used(program) => format!("Not yet: this one needs '{program}'."),
            });
        }
    }
    Ok(())
}

/// Map the older practice validations onto checks
fn practice_checks(validation: &PracticeValidation) -> Vec<Check> {
    match validation {
        PracticeValidation::FileExists(p) => vec![Check::FileExists(p.clone())],
        PracticeValidation::DirectoryExists(p) => vec![Check::DirExists(p.clone())],
        PracticeValidation::FileContains { path, content } => vec![Check::FileContains {
            path: path.clone(),
            text: content.clone(),
        }],
        PracticeValidation::OutputMatches { pattern, .. } => {
            vec![Check::OutputContains(pattern.clone())]
        }
        PracticeValidation::Custom(_) => Vec::new(),
    }
}

/// Validate any step against what the learner typed and what happened.
///
/// `command_failed` is true when the command reported an error; a command
/// exercise only passes if the right command also worked.
pub fn validate_step(
    step: &LessonStep,
    validator: &LessonValidator,
    attempt: &Attempt,
    command_failed: bool,
    world: Option<&dyn PracticeWorld>,
) -> ValidationResult {
    match &step.step_type {
        StepType::CommandExercise {
            expected_command,
            validation,
            success_message,
        } => {
            let result = validator.validate_command(attempt.input, expected_command, validation);
            if !result.is_success() {
                result
            } else if command_failed {
                ValidationResult::Failure {
                    message: "That's the right command, but it didn't work here. Read the error, then try again."
                        .to_string(),
                    hint: Some("Check the spelling of names and where you are (pwd).".to_string()),
                }
            } else {
                ValidationResult::Success {
                    message: success_message.clone(),
                }
            }
        }
        StepType::MultipleChoice { correct_index, .. } => match attempt.input.trim().parse::<usize>() {
            Ok(choice) => validator.validate_multiple_choice(choice, *correct_index),
            Err(_) => ValidationResult::Failure {
                message: "Type the number of your answer.".to_string(),
                hint: None,
            },
        },
        StepType::FillInBlank {
            template,
            correct_answers,
            explanation,
        } => {
            let norm = |t: &str| t.split_whitespace().collect::<Vec<_>>().join(" ");
            let input = norm(attempt.input);
            let blank = template
                .find('{')
                .and_then(|a| template[a..].find('}').map(|b| (a, a + b + 1)));
            let accepted = correct_answers.iter().any(|answer| {
                let full = match blank {
                    Some((a, b)) => format!("{}{}{}", &template[..a], answer, &template[b..]),
                    None => answer.clone(),
                };
                input == norm(&full) || input == norm(answer)
            });
            if accepted {
                ValidationResult::Success {
                    message: explanation.clone(),
                }
            } else {
                ValidationResult::Failure {
                    message: "Not quite.".to_string(),
                    hint: correct_answers.first().map(|a| format!("Expected: {a}")),
                }
            }
        }
        StepType::Information { .. } => ValidationResult::Success {
            message: "Continue to next step.".to_string(),
        },
        StepType::Practice { validation, .. } => {
            match evaluate_checks(&practice_checks(validation), world, attempt) {
                Ok(()) => ValidationResult::Success {
                    message: "You did it!".to_string(),
                },
                Err(message) => ValidationResult::Failure { message, hint: None },
            }
        }
        StepType::Task {
            checks,
            success_message,
            ..
        } => match evaluate_checks(checks, world, attempt) {
            Ok(()) => ValidationResult::Success {
                message: success_message.clone(),
            },
            Err(message) => ValidationResult::Failure { message, hint: None },
        },
    }
}

/// How to validate a command exercise
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandValidation {
    /// Exact command match (ignoring whitespace)
    Exact,
    /// Command and flags must match (args can vary)
    CommandAndFlags,
    /// Just the base command must match
    CommandOnly,
    /// Custom validation with regex
    Regex(String),
    /// Multiple acceptable commands
    AnyOf(Vec<String>),
}

/// Validation for practice exercises
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PracticeValidation {
    /// File exists at path
    FileExists(String),
    /// Directory exists at path
    DirectoryExists(String),
    /// File contains specific content
    FileContains { path: String, content: String },
    /// Command output matches pattern
    OutputMatches { command: String, pattern: String },
    /// Custom validator (description only, actual validation in engine)
    Custom(String),
}

/// Progress tracking for a user through lessons
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LessonProgress {
    pub lesson_id: String,
    pub current_step: u32,
    pub completed_steps: Vec<u32>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub attempts: HashMap<u32, u32>, // step_number -> attempt_count
}

impl LessonProgress {
    pub fn new(lesson_id: String) -> Self {
        Self {
            lesson_id,
            current_step: 1,
            completed_steps: Vec::new(),
            started_at: chrono::Utc::now(),
            completed_at: None,
            attempts: HashMap::new(),
        }
    }

    pub fn is_completed(&self) -> bool {
        self.completed_at.is_some()
    }

    pub fn completion_percentage(&self, total_steps: u32) -> f32 {
        if total_steps == 0 {
            return 0.0;
        }
        (self.completed_steps.len() as f32 / total_steps as f32) * 100.0
    }

    pub fn record_attempt(&mut self, step_number: u32) {
        *self.attempts.entry(step_number).or_insert(0) += 1;
    }

    pub fn complete_step(&mut self, step_number: u32) {
        if !self.completed_steps.contains(&step_number) {
            self.completed_steps.push(step_number);
            self.completed_steps.sort();
        }
    }

    pub fn complete_lesson(&mut self) {
        self.completed_at = Some(chrono::Utc::now());
    }
}

/// Lesson library that stores all available lessons
#[derive(Debug, Clone)]
pub struct LessonLibrary {
    lessons: HashMap<String, Lesson>,
    /// Registration order; `all()` derives a stable learning order from it
    order: Vec<String>,
}

impl LessonLibrary {
    pub fn new() -> Self {
        let mut library = Self {
            lessons: HashMap::new(),
            order: Vec::new(),
        };
        library.load_default_lessons();
        library
    }

    /// Register a lesson in the library
    pub fn register(&mut self, lesson: Lesson) {
        if !self.lessons.contains_key(&lesson.id) {
            self.order.push(lesson.id.clone());
        }
        self.lessons.insert(lesson.id.clone(), lesson);
    }

    /// Get a lesson by ID
    pub fn get(&self, id: &str) -> Option<&Lesson> {
        self.lessons.get(id)
    }

    /// All lessons in learning order: registration order, except that a
    /// lesson always comes after the prerequisites it names. Stable across
    /// runs (the map used to come out in random order).
    pub fn all(&self) -> Vec<&Lesson> {
        let mut placed: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut ordered: Vec<&Lesson> = Vec::with_capacity(self.order.len());
        while ordered.len() < self.order.len() {
            let before = ordered.len();
            for id in &self.order {
                if placed.contains(id.as_str()) {
                    continue;
                }
                let lesson = &self.lessons[id];
                let ready = lesson
                    .prerequisites
                    .iter()
                    .all(|p| placed.contains(p.as_str()) || !self.lessons.contains_key(p));
                if ready {
                    placed.insert(id.as_str());
                    ordered.push(lesson);
                    // Restart so earlier-registered lessons that just became
                    // ready keep their place ahead of later ones
                    break;
                }
            }
            if ordered.len() == before {
                // Prerequisite cycle: append the rest in registration order
                for id in &self.order {
                    if placed.insert(id.as_str()) {
                        ordered.push(&self.lessons[id]);
                    }
                }
            }
        }
        ordered
    }

    /// Get lessons by difficulty
    pub fn by_difficulty(&self, difficulty: Difficulty) -> Vec<&Lesson> {
        self.all()
            .into_iter()
            .filter(|l| l.difficulty == difficulty)
            .collect()
    }

    /// Get lessons by tag
    pub fn by_tag(&self, tag: &str) -> Vec<&Lesson> {
        self.all()
            .into_iter()
            .filter(|l| l.tags.iter().any(|t| t == tag))
            .collect()
    }

    /// Count lessons per difficulty level (used for achievement unlock checks)
    pub fn difficulty_counts(&self) -> HashMap<Difficulty, usize> {
        let mut counts = HashMap::new();
        for lesson in self.lessons.values() {
            *counts.entry(lesson.difficulty).or_insert(0) += 1;
        }
        counts
    }

    /// Load user-provided `*.toml` lesson files from a directory and merge
    /// them with the built-in lessons. User lessons override built-ins with
    /// the same id. Returns the number of lessons loaded.
    ///
    /// A missing directory is not an error (returns 0); an unreadable or
    /// unparsable lesson file is.
    pub fn load_from_dir(&mut self, path: &std::path::Path) -> anyhow::Result<usize> {
        use anyhow::Context;

        if !path.is_dir() {
            return Ok(0);
        }

        let mut loaded = 0;
        for entry in std::fs::read_dir(path)
            .with_context(|| format!("Failed to read lesson directory: {}", path.display()))?
        {
            let entry = entry?;
            let file_path = entry.path();
            if file_path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }

            let content = std::fs::read_to_string(&file_path)
                .with_context(|| format!("Failed to read lesson file: {}", file_path.display()))?;
            let lesson = Lesson::from_toml(&content)
                .with_context(|| format!("Failed to parse lesson file: {}", file_path.display()))?;

            self.register(lesson);
            loaded += 1;
        }

        Ok(loaded)
    }

    /// Load default lessons (initial set)
    fn load_default_lessons(&mut self) {
        for (name, source) in BUILTIN_LESSONS {
            let lesson = Lesson::from_toml(source)
                .unwrap_or_else(|e| panic!("built-in lesson {name} is invalid: {e}"));
            self.register(lesson);
        }
    }
}

/// Built-in lessons, in registration order. Each one is a TOML file under
/// `crates/arct-core/lessons/`, the same format as user lesson packs, so a
/// new lesson is a new file, not new Rust.
const BUILTIN_LESSONS: &[(&str, &str)] = &[
    ("explorer/look-around.toml", include_str!("../lessons/explorer/look-around.toml")),
    ("explorer/walk-the-halls.toml", include_str!("../lessons/explorer/walk-the-halls.toml")),
    ("explorer/read-the-logs.toml", include_str!("../lessons/explorer/read-the-logs.toml")),
    ("explorer/hidden-compartments.toml", include_str!("../lessons/explorer/hidden-compartments.toml")),
    ("explorer/build-your-bunk.toml", include_str!("../lessons/explorer/build-your-bunk.toml")),
    ("explorer/move-the-cargo.toml", include_str!("../lessons/explorer/move-the-cargo.toml")),
    ("explorer/careful-with-rm.toml", include_str!("../lessons/explorer/careful-with-rm.toml")),
    ("explorer/search-the-station.toml", include_str!("../lessons/explorer/search-the-station.toml")),
    ("explorer/write-in-the-log.toml", include_str!("../lessons/explorer/write-in-the-log.toml")),
    ("explorer/first-script.toml", include_str!("../lessons/explorer/first-script.toml")),
    ("operator/nav-basics.toml", include_str!("../lessons/operator/nav-basics.toml")),
    ("operator/file-mgmt.toml", include_str!("../lessons/operator/file-mgmt.toml")),
    ("operator/safety-essentials.toml", include_str!("../lessons/operator/safety-essentials.toml")),
    ("operator/file-viewing.toml", include_str!("../lessons/operator/file-viewing.toml")),
    ("operator/permissions.toml", include_str!("../lessons/operator/permissions.toml")),
    ("operator/process-mgmt.toml", include_str!("../lessons/operator/process-mgmt.toml")),
    ("operator/text-processing.toml", include_str!("../lessons/operator/text-processing.toml")),
    ("operator/package-mgmt.toml", include_str!("../lessons/operator/package-mgmt.toml")),
    ("operator/network-basics.toml", include_str!("../lessons/operator/network-basics.toml")),
    ("operator/git-fundamentals.toml", include_str!("../lessons/operator/git-fundamentals.toml")),
];

impl Default for LessonLibrary {
    fn default() -> Self {
        Self::new()
    }
}

/// Lesson validation engine
pub struct LessonValidator {
    // For now, just basic validation
    // Will expand with actual command execution and output checking
}

impl LessonValidator {
    pub fn new() -> Self {
        Self {}
    }

    /// Validate a command exercise
    pub fn validate_command(
        &self,
        user_input: &str,
        expected: &str,
        validation: &CommandValidation,
    ) -> ValidationResult {
        match validation {
            CommandValidation::Exact => {
                let normalized_input = user_input.trim().split_whitespace().collect::<Vec<_>>().join(" ");
                let normalized_expected = expected.trim().split_whitespace().collect::<Vec<_>>().join(" ");

                if normalized_input == normalized_expected {
                    ValidationResult::Success {
                        message: "Perfect! That's exactly right.".to_string(),
                    }
                } else {
                    ValidationResult::Failure {
                        message: format!("Not quite. Expected: {}", expected),
                        hint: Some("Check the command and flags carefully.".to_string()),
                    }
                }
            }
            CommandValidation::CommandOnly => {
                let user_cmd = user_input.trim().split_whitespace().next().unwrap_or("");
                let expected_cmd = expected.trim().split_whitespace().next().unwrap_or("");

                if user_cmd == expected_cmd {
                    ValidationResult::Success {
                        message: "Correct command!".to_string(),
                    }
                } else {
                    ValidationResult::Failure {
                        message: format!("Wrong command. Expected: {}", expected_cmd),
                        hint: None,
                    }
                }
            }
            CommandValidation::AnyOf(commands) => {
                let normalized_input = user_input.trim().split_whitespace().collect::<Vec<_>>().join(" ");

                for cmd in commands {
                    let normalized_cmd = cmd.trim().split_whitespace().collect::<Vec<_>>().join(" ");
                    if normalized_input == normalized_cmd {
                        return ValidationResult::Success {
                            message: "Correct!".to_string(),
                        };
                    }
                }

                ValidationResult::Failure {
                    message: "Not quite. Try one of the expected commands.".to_string(),
                    hint: Some(format!("Expected one of: {}", commands.join(" OR "))),
                }
            }
            CommandValidation::CommandAndFlags => {
                let (user_cmd, user_flags, user_args) = Self::split_command(user_input);
                let (expected_cmd, expected_flags, expected_args) = Self::split_command(expected);

                if user_cmd.is_empty() {
                    return ValidationResult::Failure {
                        message: "No command entered.".to_string(),
                        hint: Some(format!("Try: {}", expected)),
                    };
                }

                if user_cmd != expected_cmd {
                    return ValidationResult::Failure {
                        message: format!("Wrong command. Expected: {}", expected_cmd),
                        hint: Some("Check which program you're running.".to_string()),
                    };
                }

                if user_flags != expected_flags {
                    let expected_list: Vec<String> = expected_flags.iter().cloned().collect();
                    return ValidationResult::Failure {
                        message: "The flags don't match.".to_string(),
                        hint: Some(if expected_list.is_empty() {
                            "This command doesn't need any flags.".to_string()
                        } else {
                            format!("Expected flags: {}", expected_list.join(" "))
                        }),
                    };
                }

                if user_args != expected_args {
                    return ValidationResult::Failure {
                        message: "The arguments don't match.".to_string(),
                        hint: Some(format!("Expected: {}", expected)),
                    };
                }

                ValidationResult::Success {
                    message: "Correct command, flags, and arguments!".to_string(),
                }
            }
            CommandValidation::Regex(pattern) => match regex::Regex::new(pattern) {
                Ok(re) => {
                    if re.is_match(user_input.trim()) {
                        ValidationResult::Success {
                            message: "Correct!".to_string(),
                        }
                    } else {
                        ValidationResult::Failure {
                            message: "That command doesn't match what's expected.".to_string(),
                            hint: None,
                        }
                    }
                }
                Err(e) => ValidationResult::Failure {
                    message: format!("Invalid validation pattern in lesson data: {}", e),
                    hint: None,
                },
            },
        }
    }

    /// Split a command line into (program, flag set, positional args).
    ///
    /// Combined short flags are expanded so `-la`, `-al`, and `-l -a` all
    /// produce the flag set {"-l", "-a"}. Long flags (`--foo`) are kept whole.
    /// Flag comparison is order-insensitive; positional args keep their order.
    fn split_command(input: &str) -> (String, std::collections::BTreeSet<String>, Vec<String>) {
        let tokens = shellwords::split(input.trim())
            .unwrap_or_else(|_| input.split_whitespace().map(String::from).collect());

        let mut iter = tokens.into_iter();
        let program = iter.next().unwrap_or_default();

        let mut flags = std::collections::BTreeSet::new();
        let mut args = Vec::new();

        for token in iter {
            if token.starts_with("--") && token.len() > 2 {
                flags.insert(token);
            } else if token.starts_with('-') && token.len() > 1 {
                // Expand combined short flags: -la -> -l, -a
                for ch in token.chars().skip(1) {
                    flags.insert(format!("-{}", ch));
                }
            } else {
                args.push(token);
            }
        }

        (program, flags, args)
    }

    /// Validate a multiple choice answer
    pub fn validate_multiple_choice(&self, user_choice: usize, correct: usize) -> ValidationResult {
        if user_choice == correct {
            ValidationResult::Success {
                message: "Correct!".to_string(),
            }
        } else {
            ValidationResult::Failure {
                message: "Not quite. Try again!".to_string(),
                hint: None,
            }
        }
    }
}

impl Default for LessonValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of validating user input for a lesson step
#[derive(Debug, Clone)]
pub enum ValidationResult {
    Success { message: String },
    Failure { message: String, hint: Option<String> },
    Partial { message: String, progress: f32 },
}

impl ValidationResult {
    pub fn is_success(&self) -> bool {
        matches!(self, ValidationResult::Success { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_navigation_basics_lesson() -> Lesson {
        LessonLibrary::new().get("nav-basics").expect("built-in").clone()
    }

    #[test]
    fn test_lesson_progress_new() {
        let progress = LessonProgress::new("test-lesson".to_string());
        assert_eq!(progress.lesson_id, "test-lesson");
        assert_eq!(progress.current_step, 1);
        assert!(!progress.is_completed());
    }

    #[test]
    fn test_lesson_progress_completion() {
        let mut progress = LessonProgress::new("test-lesson".to_string());
        progress.complete_step(1);
        progress.complete_step(2);
        progress.complete_step(3);

        assert_eq!(progress.completion_percentage(3), 100.0);
        assert_eq!(progress.completion_percentage(6), 50.0);
    }

    #[test]
    fn test_command_validation_exact() {
        let validator = LessonValidator::new();
        let result = validator.validate_command(
            "ls -la",
            "ls -la",
            &CommandValidation::Exact,
        );
        assert!(result.is_success());

        let result = validator.validate_command(
            "ls -l",
            "ls -la",
            &CommandValidation::Exact,
        );
        assert!(!result.is_success());
    }

    #[test]
    fn test_command_validation_command_only() {
        let validator = LessonValidator::new();
        let result = validator.validate_command(
            "ls -la /home",
            "ls",
            &CommandValidation::CommandOnly,
        );
        assert!(result.is_success());

        // Wrong command
        let result = validator.validate_command("cd /home", "ls", &CommandValidation::CommandOnly);
        assert!(!result.is_success());

        // Empty input
        let result = validator.validate_command("", "ls", &CommandValidation::CommandOnly);
        assert!(!result.is_success());
    }

    #[test]
    fn test_command_validation_exact_negative() {
        let validator = LessonValidator::new();
        let validation = CommandValidation::Exact;

        // Wrong command
        assert!(!validator.validate_command("pwd", "ls -la", &validation).is_success());
        // Wrong flags
        assert!(!validator.validate_command("ls -l", "ls -la", &validation).is_success());
        // Empty input
        assert!(!validator.validate_command("", "ls -la", &validation).is_success());
    }

    #[test]
    fn test_command_validation_command_and_flags() {
        let validator = LessonValidator::new();
        let validation = CommandValidation::CommandAndFlags;

        // Exact match
        assert!(validator
            .validate_command("ls -la /home", "ls -la /home", &validation)
            .is_success());

        // Flags are order-insensitive and combined short flags expand:
        // -la == -al == -l -a
        assert!(validator
            .validate_command("ls -al /home", "ls -la /home", &validation)
            .is_success());
        assert!(validator
            .validate_command("ls -l -a /home", "ls -la /home", &validation)
            .is_success());
        assert!(validator
            .validate_command("ls -a -l /home", "ls -l -a /home", &validation)
            .is_success());

        // Wrong command
        assert!(!validator
            .validate_command("dir -la /home", "ls -la /home", &validation)
            .is_success());

        // Wrong flags (missing / extra)
        assert!(!validator
            .validate_command("ls -l /home", "ls -la /home", &validation)
            .is_success());
        assert!(!validator
            .validate_command("ls -lah /home", "ls -la /home", &validation)
            .is_success());

        // Wrong positional args
        assert!(!validator
            .validate_command("ls -la /tmp", "ls -la /home", &validation)
            .is_success());

        // Empty input
        assert!(!validator.validate_command("", "ls -la /home", &validation).is_success());
    }

    #[test]
    fn test_command_validation_regex() {
        let validator = LessonValidator::new();
        let validation = CommandValidation::Regex("ps.*8080|lsof.*8080".to_string());

        // Matching input
        assert!(validator
            .validate_command("ps aux | grep 8080", "", &validation)
            .is_success());
        assert!(validator
            .validate_command("lsof -i :8080", "", &validation)
            .is_success());

        // Wrong command
        assert!(!validator.validate_command("ls -la", "", &validation).is_success());
        // Empty input
        assert!(!validator.validate_command("", "", &validation).is_success());

        // Invalid pattern must fail, not silently pass
        let bad = CommandValidation::Regex("(unclosed".to_string());
        assert!(!validator.validate_command("anything", "", &bad).is_success());
    }

    #[test]
    fn test_command_validation_any_of_negative() {
        let validator = LessonValidator::new();
        let validation = CommandValidation::AnyOf(vec!["cd".to_string(), "cd ~".to_string()]);

        assert!(validator.validate_command("cd ~", "cd", &validation).is_success());
        // Wrong command
        assert!(!validator.validate_command("pwd", "cd", &validation).is_success());
        // Wrong flags/args
        assert!(!validator.validate_command("cd /tmp", "cd", &validation).is_success());
        // Empty input
        assert!(!validator.validate_command("", "cd", &validation).is_success());
    }

    #[test]
    fn test_lesson_toml_round_trip() {
        let lesson = create_navigation_basics_lesson();

        let toml_text = lesson.to_toml().expect("Lesson should serialize to TOML");
        let restored = Lesson::from_toml(&toml_text).expect("TOML should parse back to Lesson");

        assert_eq!(restored.id, lesson.id);
        assert_eq!(restored.title, lesson.title);
        assert_eq!(restored.difficulty, lesson.difficulty);
        assert_eq!(restored.steps.len(), lesson.steps.len());
        assert_eq!(restored.prerequisites, lesson.prerequisites);
        assert_eq!(restored.tags, lesson.tags);
        assert_eq!(restored.setup, lesson.setup);
    }

    #[test]
    fn test_lesson_toml_without_setup_still_parses() {
        // Backward compat: lesson-pack TOML written before the `setup` field
        // existed must keep parsing (setup defaults to empty)
        let mut lesson = create_navigation_basics_lesson();
        lesson.setup.clear();
        let toml_text = lesson.to_toml().unwrap();
        assert!(
            !toml_text.contains("[[setup]]"),
            "empty setup should not serialize an array"
        );

        let restored = Lesson::from_toml(&toml_text).expect("legacy TOML should parse");
        assert!(restored.setup.is_empty());
    }

    #[test]
    fn test_lesson_toml_setup_round_trip() {
        let mut lesson = create_navigation_basics_lesson();
        lesson.setup = vec![SetupFile {
            path: "data/sample.txt".to_string(),
            contents: "hello\nworld\n".to_string(),
        }];

        let toml_text = lesson.to_toml().unwrap();
        assert!(toml_text.contains("[[setup]]"));

        let restored = Lesson::from_toml(&toml_text).unwrap();
        assert_eq!(restored.setup.len(), 1);
        assert_eq!(restored.setup[0].path, "data/sample.txt");
        assert_eq!(restored.setup[0].contents, "hello\nworld\n");
    }

    #[test]
    fn test_builtin_lessons_have_starter_files_where_needed() {
        let library = LessonLibrary::new();
        for id in ["file-viewing", "file-mgmt", "text-processing"] {
            let lesson = library.get(id).expect("built-in lesson should exist");
            assert!(
                !lesson.setup.is_empty(),
                "lesson '{}' should ship starter files",
                id
            );
            for file in &lesson.setup {
                assert!(!file.path.starts_with('/'), "setup path must be relative");
                assert!(!file.path.contains(".."), "setup path must not contain ..");
                assert!(!file.contents.is_empty(), "setup contents must not be empty");
            }
        }
    }

    #[test]
    fn test_library_load_from_dir() {
        let dir = std::env::temp_dir().join(format!(
            "arct-lesson-dir-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        // Write an example user lesson that overrides a built-in by id
        let mut custom = create_navigation_basics_lesson();
        custom.title = "Navigation Basics (User Override)".to_string();
        std::fs::write(dir.join("nav-basics.toml"), custom.to_toml().unwrap()).unwrap();

        // And a brand-new user lesson
        let mut extra = create_navigation_basics_lesson();
        extra.id = "user-extra".to_string();
        extra.title = "User Extra Lesson".to_string();
        std::fs::write(dir.join("user-extra.toml"), extra.to_toml().unwrap()).unwrap();

        // Non-TOML files are ignored
        std::fs::write(dir.join("notes.txt"), "not a lesson").unwrap();

        let mut library = LessonLibrary::new();
        let builtin_count = library.all().len();
        let loaded = library.load_from_dir(&dir).expect("load_from_dir should succeed");

        assert_eq!(loaded, 2);
        // Override replaces, new lesson adds
        assert_eq!(library.all().len(), builtin_count + 1);
        assert_eq!(
            library.get("nav-basics").unwrap().title,
            "Navigation Basics (User Override)"
        );
        assert_eq!(library.get("user-extra").unwrap().title, "User Extra Lesson");

        // Missing directory is not an error
        let mut fresh = LessonLibrary::new();
        assert_eq!(fresh.load_from_dir(&dir.join("does-not-exist")).unwrap(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_library_order_is_stable_and_respects_prerequisites() {
        let library = LessonLibrary::new();
        let ids: Vec<&str> = library.all().iter().map(|l| l.id.as_str()).collect();
        assert!(ids.contains(&"nav-basics") && ids.contains(&"explorer-look-around"));
        let other = LessonLibrary::new();
        let again: Vec<&str> = other.all().iter().map(|l| l.id.as_str()).collect();
        assert_eq!(ids, again, "order must not change between runs");
        for (i, lesson) in library.all().iter().enumerate() {
            for prereq in &lesson.prerequisites {
                let at = ids.iter().position(|id| id == prereq).expect("prereq exists");
                assert!(at < i, "{} must come after {}", lesson.id, prereq);
            }
        }
    }
}
