//! Arc Academy Terminal - Core Domain
//!
//! This crate contains the core domain logic for the Arc Academy Terminal,
//! a production-grade interactive learning shell.

pub mod command;
pub mod education;
pub mod guard;
pub mod lesson;
pub mod playground;
pub mod session;
pub mod context;
pub mod types;
pub mod virtual_fs;
pub mod achievement;
pub mod stats;
pub mod challenge;
pub mod recommendation;

pub use command::{Command, CommandAnalyzer, CommandCategory, DangerLevel};
pub use education::{Educator, Explanation, LearningTip};
pub use guard::{GuardVerdict, PlaygroundGuard, PLAYGROUND_DISPLAY};
pub use lesson::{
    Lesson, LessonStep, StepType, LessonProgress, LessonLibrary,
    LessonValidator, ValidationResult, Difficulty, CommandValidation, SetupFile,
    Track, Check, PracticeWorld, Attempt, validate_step, evaluate_checks,
};
pub use playground::Playground;
pub use session::{Session, SessionState};
pub use context::{Context, ContextDetector};
pub use types::{Error, Result};
pub use virtual_fs::{VirtualFileSystem, DirEntry, TreeNode};
pub use achievement::{Achievement, AchievementCategory, UnlockCondition, UserAchievements, all_achievements};
pub use stats::{UserStats, ProgressSummary};
pub use challenge::{Challenge, ChallengeType, ChallengeStep, ChallengeManager, all_daily_challenges, all_weekly_challenges};
pub use recommendation::{LessonRecommendation, RecommendationReason, RecommendationEngine};
