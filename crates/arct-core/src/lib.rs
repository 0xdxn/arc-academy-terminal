//! Arc Academy Terminal - Core Domain
//!
//! This crate contains the core domain logic for the Arc Academy Terminal,
//! a production-grade interactive learning shell.

pub mod command;
pub mod education;
pub mod guard;
pub mod text_filter;
pub mod shell_syntax;
pub mod lesson;
pub mod playground;
#[cfg(feature = "progress")]
pub mod session;
pub mod context;
pub mod types;
pub mod virtual_fs;
pub mod virtual_fs_tools;
pub mod sandbox;
pub mod lesson_run;
mod sandbox_commands;
pub mod achievement;
#[cfg(feature = "progress")]
pub mod stats;
#[cfg(feature = "progress")]
pub mod challenge;
#[cfg(feature = "progress")]
pub mod recommendation;

pub use command::{Command, CommandAnalyzer, CommandCategory, DangerLevel};
pub use education::{Educator, Explanation, LearningTip};
pub use guard::{GuardVerdict, PlaygroundGuard, PLAYGROUND_DISPLAY};
pub use lesson::{
    Lesson, LessonStep, StepType, LessonProgress, LessonLibrary,
    LessonValidator, ValidationResult, Difficulty, CommandValidation, SetupFile,
    Track, Check, PracticeWorld, EntryKind, Attempt, validate_step, evaluate_checks,
};
pub use playground::Playground;
#[cfg(feature = "progress")]
pub use session::{Session, SessionState};
pub use context::{Context, ContextDetector};
pub use types::{Error, Result};
pub use virtual_fs::{VirtualFileSystem, DirEntry, TreeNode, Meta};
pub use sandbox::{Marks, Sandbox, ShellState};
pub use lesson_run::{Coach, LessonRun, Turn};
pub use achievement::{Achievement, AchievementCategory, UnlockCondition, UserAchievements, all_achievements};
#[cfg(feature = "progress")]
pub use stats::{UserStats, ProgressSummary, StreakStatus, TrackProgress, REST_DAYS, local_today};
#[cfg(feature = "progress")]
pub use challenge::{Challenge, ChallengeType, ChallengeStep, ChallengeManager, all_daily_challenges, all_weekly_challenges};
#[cfg(feature = "progress")]
pub use recommendation::{LessonRecommendation, RecommendationReason, RecommendationEngine};
