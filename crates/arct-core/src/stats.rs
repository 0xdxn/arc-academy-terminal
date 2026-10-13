//! User statistics and progress tracking
//!
//! This module tracks user progress, maintains learning streaks, and
//! manages achievement unlocking logic.

use crate::achievement::{Achievement, UnlockCondition, UserAchievements, all_achievements};
use crate::lesson::{Difficulty, LessonLibrary, Track};
use chrono::{DateTime, Datelike, Local, NaiveDate, Timelike, Utc, Weekday};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// Lesson counts per difficulty for the built-in lesson library, computed once.
///
/// Used by the no-argument achievement APIs; callers that load user lessons
/// should prefer the `*_with_library` variants for accurate totals.
fn builtin_difficulty_counts() -> &'static HashMap<Difficulty, usize> {
    static COUNTS: OnceLock<HashMap<Difficulty, usize>> = OnceLock::new();
    COUNTS.get_or_init(|| LessonLibrary::new().difficulty_counts())
}

/// Comprehensive user statistics and progress tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserStats {
    /// Set of completed lesson IDs
    pub lessons_completed: HashSet<String>,
    /// Count of lessons completed by difficulty level
    pub lessons_by_difficulty: HashMap<Difficulty, usize>,
    /// Set of unique commands that have been used
    pub commands_used: HashSet<String>,
    /// Total number of commands executed (including duplicates)
    pub total_commands_executed: usize,
    /// Current consecutive days streak
    pub current_streak: usize,
    /// Longest streak ever achieved
    pub longest_streak: usize,
    /// Last date the user was active
    pub last_active_date: NaiveDate,
    /// Total time spent in the app (seconds)
    pub total_time_seconds: u64,
    /// When the current session started
    #[serde(skip)]
    pub session_start: DateTime<Utc>,
    /// User's achievement progress
    pub achievements: UserAchievements,
    /// Completed challenges (for exploration/challenge achievements)
    pub completed_challenges: HashSet<String>,
    /// Lessons finished in a run where the answer was never revealed.
    /// `#[serde(default)]` keeps older save files loadable.
    #[serde(default)]
    pub lessons_mastered: HashSet<String>,
}

/// Days a learner can take off between learning days without ending
/// their streak. A missed day is a rest day, not a failure.
pub const REST_DAYS: i64 = 2;

/// Where a learner's streak stands on a given day
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreakStatus {
    /// No learning days yet
    NotStarted,
    /// Learned today; `days` learning days in this run
    LearnedToday { days: usize },
    /// Resting: the run of `days` continues if they learn within `rest_days_left` days
    Resting { days: usize, rest_days_left: i64 },
    /// The last run ended after a long break; the next learning day starts a new one
    WelcomeBack { best: usize },
}

/// One track's progress against a lesson library
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackProgress {
    pub track: Track,
    /// Lessons in the track, in library order
    pub total: usize,
    /// Titles of finished lessons, in library order
    pub completed: Vec<String>,
    /// Titles of lessons still to do, in library order
    pub remaining: Vec<String>,
    /// Finished lessons that were also mastered
    pub mastered: usize,
}

impl TrackProgress {
    /// Every lesson in the track is finished (and the track has lessons)
    pub fn is_complete(&self) -> bool {
        self.total > 0 && self.remaining.is_empty()
    }

    /// Every lesson in the track was finished without needing the answer
    pub fn is_mastered(&self) -> bool {
        self.is_complete() && self.mastered == self.total
    }
}

/// Today's date where the learner is (streaks follow the learner's own days)
pub fn local_today() -> NaiveDate {
    Local::now().date_naive()
}

impl UserStats {
    /// Create a new user stats tracker
    pub fn new() -> Self {
        Self {
            lessons_completed: HashSet::new(),
            lessons_by_difficulty: HashMap::new(),
            commands_used: HashSet::new(),
            total_commands_executed: 0,
            current_streak: 0,
            longest_streak: 0,
            last_active_date: local_today(),
            total_time_seconds: 0,
            session_start: Utc::now(),
            achievements: UserAchievements::new(),
            completed_challenges: HashSet::new(),
            lessons_mastered: HashSet::new(),
        }
    }

    /// Count today as a learning day. Call when the learner actually does
    /// something (runs a command, passes a step), not merely on start-up.
    pub fn update_streak(&mut self) {
        self.record_learning_day(local_today());
    }

    /// Count `today` as a learning day. Up to `REST_DAYS` days off between
    /// learning days keep the run going; a longer break starts a new run.
    /// The best run is always kept.
    pub fn record_learning_day(&mut self, today: NaiveDate) {
        let gap = (today - self.last_active_date).num_days();
        if self.current_streak == 0 {
            self.current_streak = 1;
        } else if gap <= 0 {
            // Same day (or the clock moved back): nothing to add
            return;
        } else if gap <= REST_DAYS + 1 {
            self.current_streak += 1;
        } else {
            self.current_streak = 1;
        }
        self.last_active_date = today;
        self.longest_streak = self.longest_streak.max(self.current_streak);
    }

    /// End a run whose rest days have run out, without counting `today` as
    /// a learning day. Call on start-up so screens never show a run that
    /// has already ended.
    pub fn settle_streak(&mut self, today: NaiveDate) {
        if self.current_streak > 0 && (today - self.last_active_date).num_days() > REST_DAYS + 1 {
            self.current_streak = 0;
        }
    }

    /// Where the streak stands on `today`
    pub fn streak_status(&self, today: NaiveDate) -> StreakStatus {
        let gap = (today - self.last_active_date).num_days();
        if self.current_streak == 0 {
            if self.longest_streak > 0 {
                StreakStatus::WelcomeBack { best: self.longest_streak }
            } else {
                StreakStatus::NotStarted
            }
        } else if gap <= 0 {
            StreakStatus::LearnedToday { days: self.current_streak }
        } else if gap <= REST_DAYS + 1 {
            StreakStatus::Resting {
                days: self.current_streak,
                rest_days_left: REST_DAYS + 1 - gap,
            }
        } else {
            StreakStatus::WelcomeBack { best: self.longest_streak }
        }
    }

    /// Record that a lesson was finished without the answer being revealed
    pub fn record_lesson_mastered(&mut self, lesson_id: String) {
        self.lessons_mastered.insert(lesson_id);
    }

    /// Progress through one track of the given library
    pub fn track_progress(&self, library: &LessonLibrary, track: Track) -> TrackProgress {
        let mut progress = TrackProgress {
            track,
            total: 0,
            completed: Vec::new(),
            remaining: Vec::new(),
            mastered: 0,
        };
        for lesson in library.all().into_iter().filter(|l| l.track == track) {
            progress.total += 1;
            if self.lessons_completed.contains(&lesson.id) {
                progress.completed.push(lesson.title.clone());
                if self.lessons_mastered.contains(&lesson.id) {
                    progress.mastered += 1;
                }
            } else {
                progress.remaining.push(lesson.title.clone());
            }
        }
        progress
    }

    /// Record completion of a lesson
    pub fn record_lesson_completion(&mut self, lesson_id: String, difficulty: Difficulty) {
        if self.lessons_completed.insert(lesson_id) {
            // Only increment if this is a new completion
            *self.lessons_by_difficulty.entry(difficulty).or_insert(0) += 1;
        }
    }

    /// Record use of a command
    pub fn record_command_use(&mut self, command: String) {
        self.commands_used.insert(command);
        self.total_commands_executed += 1;
    }

    /// Mark a challenge as completed
    pub fn complete_challenge(&mut self, challenge_id: String) {
        self.completed_challenges.insert(challenge_id);
    }

    /// Record a one-shot feature-usage event from the UI layer and return any
    /// newly unlocked achievements.
    ///
    /// The TUI should call this at the corresponding feature-usage call sites.
    /// Recognized event keys (each backs a `CompleteChallenge` achievement
    /// condition, see `achievement::all_achievements`):
    ///
    /// - `"use_ai"`           — the AI assistant was invoked/toggled
    /// - `"change_theme"`     — the user changed the color theme
    /// - `"visit_all_panels"` — the user has visited every panel at least once
    /// - `"use_context"`      — the context detection feature was used
    /// - `"speed_lesson"`     — a lesson was completed in under 5 minutes
    /// - `"perfect_lesson"`   — a lesson was completed without failed attempts
    /// - `"first_challenge"`  — the user completed their first daily/weekly
    ///   challenge (sent automatically by `ChallengeManager::check_command`)
    ///
    /// Calling this repeatedly with the same key is safe (idempotent).
    pub fn record_event(&mut self, event_key: &str) -> Vec<Achievement> {
        self.complete_challenge(event_key.to_string());
        self.check_achievements()
    }

    /// Check all achievements against the built-in lesson library and return
    /// newly unlocked ones.
    ///
    /// If user lessons have been loaded via `LessonLibrary::load_from_dir`,
    /// prefer `check_achievements_with_library` so per-difficulty totals are
    /// accurate.
    pub fn check_achievements(&mut self) -> Vec<Achievement> {
        self.check_achievements_with_counts(builtin_difficulty_counts())
    }

    /// Check all achievements using real per-difficulty lesson counts from
    /// the given library, and return newly unlocked ones.
    pub fn check_achievements_with_library(&mut self, library: &LessonLibrary) -> Vec<Achievement> {
        self.check_achievements_with_counts(&library.difficulty_counts())
    }

    fn check_achievements_with_counts(
        &mut self,
        difficulty_totals: &HashMap<Difficulty, usize>,
    ) -> Vec<Achievement> {
        let mut newly_unlocked = Vec::new();

        for achievement in all_achievements() {
            // Skip if already unlocked
            if self.achievements.is_unlocked(&achievement.id) {
                continue;
            }

            // Check if condition is met
            if self.check_unlock_condition(&achievement.condition, difficulty_totals) {
                self.achievements.unlock(achievement.id.clone());
                newly_unlocked.push(achievement);
            }
        }

        newly_unlocked
    }

    /// Check if a specific unlock condition is met
    fn check_unlock_condition(
        &self,
        condition: &UnlockCondition,
        difficulty_totals: &HashMap<Difficulty, usize>,
    ) -> bool {
        match condition {
            UnlockCondition::CompleteLesson(lesson_id) => {
                self.lessons_completed.contains(lesson_id)
            }
            UnlockCondition::CompleteLessons(count) => {
                self.lessons_completed.len() >= *count
            }
            UnlockCondition::CompleteAllDifficulty(difficulty) => {
                let completed = self.lessons_by_difficulty.get(difficulty).unwrap_or(&0);
                // Only unlockable when lessons of this difficulty actually
                // exist; a difficulty with zero lessons never auto-unlocks.
                match difficulty_totals.get(difficulty) {
                    Some(&total) if total > 0 => *completed >= total,
                    _ => false,
                }
            }
            UnlockCondition::UseCommands(count) => {
                self.commands_used.len() >= *count
            }
            UnlockCondition::MaintainStreak(days) => {
                self.current_streak >= *days
            }
            UnlockCondition::CompleteChallenge(challenge_id) => {
                self.completed_challenges.contains(challenge_id)
            }
            UnlockCondition::TimeOfDay { start, end } => {
                let current_hour = Local::now().hour() as u8;
                if start < end {
                    current_hour >= *start && current_hour < *end
                } else {
                    // Handles wrap-around (e.g., 22-2 for late night)
                    current_hour >= *start || current_hour < *end
                }
            }
            UnlockCondition::IsWeekend => {
                let weekday = Local::now().weekday();
                weekday == Weekday::Sat || weekday == Weekday::Sun
            }
        }
    }

    /// Calculate completion percentage for a difficulty level, based on the
    /// built-in lesson library. Use `completion_percentage_with_library` when
    /// user lessons have been loaded.
    pub fn completion_percentage(&self, difficulty: Difficulty) -> f32 {
        self.completion_percentage_with_counts(difficulty, builtin_difficulty_counts())
    }

    /// Calculate completion percentage for a difficulty level using real
    /// lesson counts from the given library.
    pub fn completion_percentage_with_library(
        &self,
        difficulty: Difficulty,
        library: &LessonLibrary,
    ) -> f32 {
        self.completion_percentage_with_counts(difficulty, &library.difficulty_counts())
    }

    fn completion_percentage_with_counts(
        &self,
        difficulty: Difficulty,
        difficulty_totals: &HashMap<Difficulty, usize>,
    ) -> f32 {
        let completed = self.lessons_by_difficulty.get(&difficulty).unwrap_or(&0);
        let total = *difficulty_totals.get(&difficulty).unwrap_or(&0);
        if total == 0 {
            return 0.0;
        }
        (*completed as f32 / total as f32) * 100.0
    }

    /// Get overall completion percentage across all lessons
    pub fn overall_completion_percentage(&self, total_lessons: usize) -> f32 {
        if total_lessons == 0 {
            return 0.0;
        }
        (self.lessons_completed.len() as f32 / total_lessons as f32) * 100.0
    }

    /// Update total time spent so "Time Invested" accumulates.
    ///
    /// This is the TUI heartbeat API: call it periodically (e.g. once per
    /// render tick or once a minute) AND on session end / before persisting
    /// stats. Each call folds the elapsed time since `session_start` into
    /// `total_time_seconds` and resets `session_start` to now, so calling it
    /// at any frequency never double-counts time.
    pub fn update_session_time(&mut self) {
        let now = Utc::now();
        let session_duration = (now - self.session_start).num_seconds();
        if session_duration > 0 {
            self.total_time_seconds += session_duration as u64;
        }
        self.session_start = now;
    }

    /// Get formatted time spent string (e.g., "2h 30m")
    pub fn formatted_time_spent(&self) -> String {
        let hours = self.total_time_seconds / 3600;
        let minutes = (self.total_time_seconds % 3600) / 60;

        if hours > 0 {
            format!("{}h {}m", hours, minutes)
        } else {
            format!("{}m", minutes)
        }
    }

    /// Get progress summary
    pub fn progress_summary(&self) -> ProgressSummary {
        ProgressSummary {
            total_lessons_completed: self.lessons_completed.len(),
            unique_commands_used: self.commands_used.len(),
            total_commands_executed: self.total_commands_executed,
            current_streak: self.current_streak,
            longest_streak: self.longest_streak,
            achievements_unlocked: self.achievements.total_unlocked(),
            total_points: self.achievements.total_points(),
            time_spent: self.formatted_time_spent(),
        }
    }

    /// Get achievements that are close to being unlocked (for motivation)
    pub fn nearly_unlocked_achievements(&self) -> Vec<(Achievement, f32)> {
        let mut nearly_unlocked = Vec::new();

        for achievement in all_achievements() {
            if self.achievements.is_unlocked(&achievement.id) {
                continue;
            }

            let progress = self.achievement_progress(&achievement.condition);
            if progress >= 0.5 && progress < 1.0 {
                nearly_unlocked.push((achievement, progress));
            }
        }

        // Sort by progress (closest to completion first)
        nearly_unlocked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        nearly_unlocked
    }

    /// Calculate progress towards an achievement (0.0 to 1.0)
    fn achievement_progress(&self, condition: &UnlockCondition) -> f32 {
        match condition {
            UnlockCondition::CompleteLessons(target) => {
                (self.lessons_completed.len() as f32 / *target as f32).min(1.0)
            }
            UnlockCondition::UseCommands(target) => {
                (self.commands_used.len() as f32 / *target as f32).min(1.0)
            }
            UnlockCondition::MaintainStreak(target) => {
                (self.current_streak as f32 / *target as f32).min(1.0)
            }
            UnlockCondition::CompleteLesson(_) |
            UnlockCondition::CompleteAllDifficulty(_) |
            UnlockCondition::CompleteChallenge(_) |
            UnlockCondition::TimeOfDay { .. } |
            UnlockCondition::IsWeekend => {
                if self.check_unlock_condition(condition, builtin_difficulty_counts()) {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

impl Default for UserStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of user progress
#[derive(Debug, Clone)]
pub struct ProgressSummary {
    pub total_lessons_completed: usize,
    pub unique_commands_used: usize,
    pub total_commands_executed: usize,
    pub current_streak: usize,
    pub longest_streak: usize,
    pub achievements_unlocked: usize,
    pub total_points: u32,
    pub time_spent: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_stats() {
        let stats = UserStats::new();
        assert_eq!(stats.lessons_completed.len(), 0);
        assert_eq!(stats.current_streak, 0);
        assert_eq!(stats.total_commands_executed, 0);
    }

    #[test]
    fn test_record_lesson() {
        let mut stats = UserStats::new();
        stats.record_lesson_completion("lesson1".to_string(), Difficulty::Beginner);

        assert_eq!(stats.lessons_completed.len(), 1);
        assert_eq!(*stats.lessons_by_difficulty.get(&Difficulty::Beginner).unwrap(), 1);

        // Recording same lesson again shouldn't increase count
        stats.record_lesson_completion("lesson1".to_string(), Difficulty::Beginner);
        assert_eq!(stats.lessons_completed.len(), 1);
    }

    #[test]
    fn test_record_command() {
        let mut stats = UserStats::new();
        stats.record_command_use("ls".to_string());
        stats.record_command_use("cd".to_string());
        stats.record_command_use("ls".to_string()); // Duplicate

        assert_eq!(stats.commands_used.len(), 2);
        assert_eq!(stats.total_commands_executed, 3);
    }

    #[test]
    fn test_check_achievements() {
        let mut stats = UserStats::new();

        // Complete first lesson
        stats.record_lesson_completion("lesson1".to_string(), Difficulty::Beginner);

        let unlocked = stats.check_achievements();
        assert!(!unlocked.is_empty());
        assert!(unlocked.iter().any(|a| a.id == "first_steps"));
    }

    #[test]
    fn test_completion_percentage() {
        let mut stats = UserStats::new();
        stats.record_lesson_completion("lesson1".to_string(), Difficulty::Beginner);
        stats.record_lesson_completion("lesson2".to_string(), Difficulty::Beginner);

        let total_beginner = LessonLibrary::new()
            .by_difficulty(Difficulty::Beginner)
            .len();
        let expected = (2.0 / total_beginner as f32) * 100.0;
        let percentage = stats.completion_percentage(Difficulty::Beginner);
        assert_eq!(percentage, expected);

        // A difficulty with no available lessons reports 0%
        assert_eq!(stats.completion_percentage(Difficulty::Expert), 0.0);
    }

    #[test]
    fn test_complete_all_difficulty_uses_real_lesson_counts() {
        let library = LessonLibrary::new();
        let beginner_lessons: Vec<String> = library
            .by_difficulty(Difficulty::Beginner)
            .iter()
            .map(|l| l.id.clone())
            .collect();
        assert!(!beginner_lessons.is_empty());

        let mut stats = UserStats::new();

        // Complete all but one beginner lesson: not yet unlocked
        for id in &beginner_lessons[..beginner_lessons.len() - 1] {
            stats.record_lesson_completion(id.clone(), Difficulty::Beginner);
        }
        stats.check_achievements_with_library(&library);
        assert!(!stats.achievements.is_unlocked("beginner_complete"));

        // Complete the last one: unlocked
        stats.record_lesson_completion(
            beginner_lessons.last().unwrap().clone(),
            Difficulty::Beginner,
        );
        stats.check_achievements_with_library(&library);
        assert!(stats.achievements.is_unlocked("beginner_complete"));

        // Difficulties with zero available lessons never auto-unlock
        assert!(!stats.achievements.is_unlocked("advanced_complete"));
        assert!(!stats.achievements.is_unlocked("expert_complete"));
    }

    #[test]
    fn test_navigation_master_matches_real_lesson_id() {
        let mut stats = UserStats::new();
        stats.record_lesson_completion("nav-basics".to_string(), Difficulty::Beginner);
        stats.check_achievements();
        assert!(stats.achievements.is_unlocked("navigation_master"));
    }

    #[test]
    fn test_complete_lesson_achievement_ids_exist_in_library() {
        // Guard against typos: every CompleteLesson condition must reference
        // a real lesson id in the built-in library.
        let library = LessonLibrary::new();
        for achievement in all_achievements() {
            if let UnlockCondition::CompleteLesson(lesson_id) = &achievement.condition {
                assert!(
                    library.get(lesson_id).is_some(),
                    "Achievement '{}' references unknown lesson id '{}'",
                    achievement.id,
                    lesson_id
                );
            }
        }
    }

    #[test]
    fn test_record_event_unlocks_exploration_achievements() {
        let mut stats = UserStats::new();

        let unlocked = stats.record_event("use_ai");
        assert!(unlocked.iter().any(|a| a.id == "ai_assistant"));

        stats.record_event("change_theme");
        assert!(stats.achievements.is_unlocked("theme_explorer"));

        stats.record_event("visit_all_panels");
        assert!(stats.achievements.is_unlocked("panel_master"));

        stats.record_event("use_context");
        assert!(stats.achievements.is_unlocked("context_aware"));

        stats.record_event("speed_lesson");
        assert!(stats.achievements.is_unlocked("speed_learner"));

        stats.record_event("perfect_lesson");
        assert!(stats.achievements.is_unlocked("perfect_lesson"));

        stats.record_event("first_challenge");
        assert!(stats.achievements.is_unlocked("challenge_accepted"));

        // Idempotent: repeating an event doesn't re-unlock
        let again = stats.record_event("use_ai");
        assert!(again.is_empty());
    }

    #[test]
    fn test_time_tracking() {
        let mut stats = UserStats::new();
        stats.total_time_seconds = 7200 + 1800; // 2h 30m

        let formatted = stats.formatted_time_spent();
        assert_eq!(formatted, "2h 30m");
    }

    #[test]
    fn test_progress_summary() {
        let mut stats = UserStats::new();
        stats.record_lesson_completion("lesson1".to_string(), Difficulty::Beginner);
        stats.record_command_use("ls".to_string());

        let summary = stats.progress_summary();
        assert_eq!(summary.total_lessons_completed, 1);
        assert_eq!(summary.unique_commands_used, 1);
    }

    fn day(n: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, n).unwrap()
    }

    #[test]
    fn first_learning_day_counts() {
        let mut stats = UserStats::new();
        stats.last_active_date = day(1);
        stats.record_learning_day(day(1));
        assert_eq!(stats.current_streak, 1);
        assert_eq!(stats.streak_status(day(1)), StreakStatus::LearnedToday { days: 1 });
    }

    #[test]
    fn rest_days_keep_the_streak() {
        let mut stats = UserStats::new();
        stats.last_active_date = day(1);
        stats.record_learning_day(day(1));
        stats.record_learning_day(day(1)); // same day twice
        stats.record_learning_day(day(2));
        assert_eq!(stats.current_streak, 2);

        // Two days off, then back: the run continues
        assert_eq!(
            stats.streak_status(day(4)),
            StreakStatus::Resting { days: 2, rest_days_left: 1 }
        );
        // Day 5 is the last day that keeps it going
        assert_eq!(
            stats.streak_status(day(5)),
            StreakStatus::Resting { days: 2, rest_days_left: 0 }
        );
        stats.settle_streak(day(5));
        assert_eq!(stats.current_streak, 2);
        stats.record_learning_day(day(5));
        assert_eq!(stats.current_streak, 3);
        assert_eq!(stats.longest_streak, 3);
    }

    #[test]
    fn a_long_break_starts_a_new_run_and_keeps_the_best() {
        let mut stats = UserStats::new();
        stats.last_active_date = day(1);
        for d in 1..=4 {
            stats.record_learning_day(day(d));
        }
        assert_eq!(stats.current_streak, 4);

        // Three days off is past the rest allowance
        stats.settle_streak(day(8));
        assert_eq!(stats.current_streak, 0);
        assert_eq!(stats.streak_status(day(8)), StreakStatus::WelcomeBack { best: 4 });
        stats.record_learning_day(day(8));
        assert_eq!(stats.current_streak, 1);
        assert_eq!(stats.longest_streak, 4);
    }

    #[test]
    fn clock_moving_back_changes_nothing() {
        let mut stats = UserStats::new();
        stats.last_active_date = day(5);
        stats.record_learning_day(day(5));
        stats.record_learning_day(day(6));
        stats.record_learning_day(day(3));
        assert_eq!(stats.current_streak, 2);
        assert_eq!(stats.last_active_date, day(6));
    }

    #[test]
    fn track_progress_counts_completion_and_mastery() {
        let library = LessonLibrary::new();
        let explorer: Vec<_> = library
            .all()
            .into_iter()
            .filter(|l| l.track == Track::Explorer)
            .map(|l| l.id.clone())
            .collect();
        assert!(explorer.len() > 1);

        let mut stats = UserStats::new();
        let fresh = stats.track_progress(&library, Track::Explorer);
        assert_eq!(fresh.total, explorer.len());
        assert!(fresh.completed.is_empty());
        assert!(!fresh.is_complete());

        for id in &explorer {
            stats.lessons_completed.insert(id.clone());
        }
        stats.record_lesson_mastered(explorer[0].clone());
        let done = stats.track_progress(&library, Track::Explorer);
        assert!(done.is_complete());
        assert!(!done.is_mastered());
        assert_eq!(done.mastered, 1);

        for id in &explorer {
            stats.record_lesson_mastered(id.clone());
        }
        assert!(stats.track_progress(&library, Track::Explorer).is_mastered());
        // The other track is untouched
        assert!(!stats.track_progress(&library, Track::Operator).is_complete());
    }

    #[test]
    fn old_save_files_without_mastery_still_load() {
        let mut value = serde_json::to_value(UserStats::new()).unwrap();
        value.as_object_mut().unwrap().remove("lessons_mastered");
        let stats: UserStats = serde_json::from_value(value).unwrap();
        assert!(stats.lessons_mastered.is_empty());
    }

    #[test]
    fn test_challenge_completion() {
        let mut stats = UserStats::new();
        stats.complete_challenge("use_ai".to_string());

        assert!(stats.completed_challenges.contains("use_ai"));
    }
}
