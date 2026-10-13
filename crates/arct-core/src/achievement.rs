//! Achievement and gamification system
//!
//! This module provides a comprehensive achievement system to encourage
//! learning and engagement with the Arc Academy Terminal.

use crate::lesson::Difficulty;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Represents a single achievement that can be unlocked
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Achievement {
    pub id: String,
    pub title: String,
    pub description: String,
    pub icon: &'static str,
    pub category: AchievementCategory,
    pub condition: UnlockCondition,
    pub points: u32,
}

/// Category of achievements for organization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum AchievementCategory {
    Lessons,
    Commands,
    Streaks,
    Challenges,
    Exploration,
}

/// Conditions that trigger achievement unlocks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UnlockCondition {
    /// Complete a specific lesson by ID
    CompleteLesson(String),
    /// Complete N lessons (any lessons)
    CompleteLessons(usize),
    /// Complete all lessons of a specific difficulty
    CompleteAllDifficulty(Difficulty),
    /// Use N unique commands
    UseCommands(usize),
    /// Maintain a streak of N days
    MaintainStreak(usize),
    /// Complete a specific challenge by ID
    CompleteChallenge(String),
    /// Use the app during a specific time window (24-hour format)
    TimeOfDay { start: u8, end: u8 },
    /// Use the app on a weekend
    IsWeekend,
}

/// User's unlocked achievements and timestamps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAchievements {
    /// Set of unlocked achievement IDs
    pub unlocked: HashSet<String>,
    /// Timestamps when each achievement was unlocked
    pub unlock_timestamps: HashMap<String, DateTime<Utc>>,
}

impl UserAchievements {
    /// Create a new empty achievements tracker
    pub fn new() -> Self {
        Self {
            unlocked: HashSet::new(),
            unlock_timestamps: HashMap::new(),
        }
    }

    /// Unlock an achievement
    pub fn unlock(&mut self, achievement_id: String) {
        if !self.unlocked.contains(&achievement_id) {
            self.unlocked.insert(achievement_id.clone());
            self.unlock_timestamps.insert(achievement_id, Utc::now());
        }
    }

    /// Check if an achievement is unlocked
    pub fn is_unlocked(&self, achievement_id: &str) -> bool {
        self.unlocked.contains(achievement_id)
    }

    /// Get total number of unlocked achievements
    pub fn total_unlocked(&self) -> usize {
        self.unlocked.len()
    }

    /// Get achievements by category
    pub fn unlocked_in_category(&self, category: AchievementCategory) -> Vec<String> {
        all_achievements()
            .into_iter()
            .filter(|a| a.category == category && self.unlocked.contains(&a.id))
            .map(|a| a.id)
            .collect()
    }

    /// Get total points earned
    pub fn total_points(&self) -> u32 {
        all_achievements()
            .into_iter()
            .filter(|a| self.unlocked.contains(&a.id))
            .map(|a| a.points)
            .sum()
    }
}

impl Default for UserAchievements {
    fn default() -> Self {
        Self::new()
    }
}

/// Get all available achievements in the system
pub fn all_achievements() -> Vec<Achievement> {
    vec![
        // ===== LESSON-BASED ACHIEVEMENTS (14) =====
        Achievement {
            id: "first_steps".to_string(),
            title: "First Steps".to_string(),
            description: "Complete your very first lesson. Every expert was once a beginner!".to_string(),
            icon: "\u{f06c}", // Seedling/leaf
            category: AchievementCategory::Lessons,
            condition: UnlockCondition::CompleteLessons(1),
            points: 10,
        },
        Achievement {
            id: "getting_started".to_string(),
            title: "Getting Started".to_string(),
            description: "Complete 3 lessons. You're building momentum!".to_string(),
            icon: "\u{f135}", // Rocket
            category: AchievementCategory::Lessons,
            condition: UnlockCondition::CompleteLessons(3),
            points: 25,
        },
        Achievement {
            id: "committed_learner".to_string(),
            title: "Committed Learner".to_string(),
            description: "Complete 5 lessons. Your dedication is impressive!".to_string(),
            icon: "\u{f02d}", // Book
            category: AchievementCategory::Lessons,
            condition: UnlockCondition::CompleteLessons(5),
            points: 50,
        },
        Achievement {
            id: "lesson_enthusiast".to_string(),
            title: "Lesson Enthusiast".to_string(),
            description: "Complete 10 lessons. You're really getting the hang of this!".to_string(),
            icon: "\u{f19d}", // Graduation cap
            category: AchievementCategory::Lessons,
            condition: UnlockCondition::CompleteLessons(10),
            points: 100,
        },
        Achievement {
            id: "lesson_master".to_string(),
            title: "Lesson Master".to_string(),
            description: "Complete 20 lessons. You've mastered a wide range of shell skills!".to_string(),
            icon: "\u{f005}", // Star
            category: AchievementCategory::Lessons,
            condition: UnlockCondition::CompleteLessons(20),
            points: 200,
        },
        Achievement {
            id: "station_crew".to_string(),
            title: "Station Crew".to_string(),
            description: "Finish every Explorer lesson on Arc Station.".to_string(),
            icon: "\u{f197}", // Space shuttle
            category: AchievementCategory::Lessons,
            condition: UnlockCondition::CompleteChallenge("explorer_complete".to_string()),
            points: 150,
        },
        Achievement {
            id: "station_ace".to_string(),
            title: "Station Ace".to_string(),
            description: "Finish every Explorer lesson without needing the answer.".to_string(),
            icon: "\u{f005}", // Star
            category: AchievementCategory::Lessons,
            condition: UnlockCondition::CompleteChallenge("explorer_mastered".to_string()),
            points: 200,
        },
        Achievement {
            id: "operator".to_string(),
            title: "Operator".to_string(),
            description: "Finish every Operator lesson.".to_string(),
            icon: "\u{f120}", // Terminal
            category: AchievementCategory::Lessons,
            condition: UnlockCondition::CompleteChallenge("operator_complete".to_string()),
            points: 300,
        },
        Achievement {
            id: "first_keys".to_string(),
            title: "First Keys".to_string(),
            description: "Use the magic words ls, cd and pwd in First Keys.".to_string(),
            icon: "\u{f11c}", // Keyboard
            category: AchievementCategory::Exploration,
            condition: UnlockCondition::CompleteChallenge("first_keys_complete".to_string()),
            points: 50,
        },
        Achievement {
            id: "senior_operator".to_string(),
            title: "Senior Operator".to_string(),
            description: "Finish every Operator lesson without needing the answer.".to_string(),
            icon: "\u{f091}", // Trophy
            category: AchievementCategory::Lessons,
            condition: UnlockCondition::CompleteChallenge("operator_mastered".to_string()),
            points: 400,
        },
        Achievement {
            id: "beginner_complete".to_string(),
            title: "Beginner Graduate".to_string(),
            description: "Complete all beginner lessons. You've mastered the fundamentals!".to_string(),
            icon: "\u{f0a3}", // Certificate
            category: AchievementCategory::Lessons,
            condition: UnlockCondition::CompleteAllDifficulty(Difficulty::Beginner),
            points: 150,
        },
        Achievement {
            id: "intermediate_complete".to_string(),
            title: "Intermediate Expert".to_string(),
            description: "Complete all intermediate lessons. You're becoming a shell pro!".to_string(),
            icon: "\u{f091}", // Trophy
            category: AchievementCategory::Lessons,
            condition: UnlockCondition::CompleteAllDifficulty(Difficulty::Intermediate),
            points: 300,
        },
        Achievement {
            id: "advanced_complete".to_string(),
            title: "Advanced Achiever".to_string(),
            description: "Complete all advanced lessons. Your skills are exceptional!".to_string(),
            icon: "\u{f521}", // Medal
            category: AchievementCategory::Lessons,
            condition: UnlockCondition::CompleteAllDifficulty(Difficulty::Advanced),
            points: 500,
        },
        Achievement {
            id: "expert_complete".to_string(),
            title: "Shell Expert".to_string(),
            description: "Complete all expert lessons. You've reached the pinnacle of shell mastery!".to_string(),
            icon: "\u{f559}", // Crown
            category: AchievementCategory::Lessons,
            condition: UnlockCondition::CompleteAllDifficulty(Difficulty::Expert),
            points: 1000,
        },
        Achievement {
            id: "navigation_master".to_string(),
            title: "Navigation Master".to_string(),
            description: "Complete the Navigation Basics lesson. You can now find your way around!".to_string(),
            icon: "\u{f14e}", // Compass
            category: AchievementCategory::Lessons,
            condition: UnlockCondition::CompleteLesson("nav-basics".to_string()),
            points: 20,
        },

        // ===== STREAK-BASED ACHIEVEMENTS (7) =====
        Achievement {
            id: "three_day_streak".to_string(),
            title: "Three Day Streak".to_string(),
            description: "Learn on 3 days without a long break.".to_string(),
            icon: "\u{f06d}", // Fire
            category: AchievementCategory::Streaks,
            condition: UnlockCondition::MaintainStreak(3),
            points: 30,
        },
        Achievement {
            id: "week_warrior".to_string(),
            title: "Seven Days".to_string(),
            description: "Learn on 7 days without a long break.".to_string(),
            icon: "\u{f073}", // Calendar
            category: AchievementCategory::Streaks,
            condition: UnlockCondition::MaintainStreak(7),
            points: 75,
        },
        Achievement {
            id: "two_week_streak".to_string(),
            title: "Fourteen Days".to_string(),
            description: "Learn on 14 days without a long break.".to_string(),
            icon: "\u{f274}", // Calendar check
            category: AchievementCategory::Streaks,
            condition: UnlockCondition::MaintainStreak(14),
            points: 150,
        },
        Achievement {
            id: "month_master".to_string(),
            title: "Thirty Days".to_string(),
            description: "Learn on 30 days without a long break.".to_string(),
            icon: "\u{f1ec}", // Trophy variant
            category: AchievementCategory::Streaks,
            condition: UnlockCondition::MaintainStreak(30),
            points: 300,
        },
        Achievement {
            id: "hundred_day_hero".to_string(),
            title: "Hundred Days".to_string(),
            description: "Learn on 100 days without a long break.".to_string(),
            icon: "\u{f2db}", // Hundred points
            category: AchievementCategory::Streaks,
            condition: UnlockCondition::MaintainStreak(100),
            points: 1000,
        },
        Achievement {
            id: "weekend_warrior".to_string(),
            title: "Weekend Warrior".to_string(),
            description: "Learn on the weekend. Dedication knows no bounds!".to_string(),
            icon: "\u{f133}", // Calendar weekend
            category: AchievementCategory::Streaks,
            condition: UnlockCondition::IsWeekend,
            points: 20,
        },
        Achievement {
            id: "early_bird".to_string(),
            title: "Early Bird".to_string(),
            description: "Use Arc Academy before 8 AM. The early bird gets the shell!".to_string(),
            icon: "\u{f54c}", // Sun rising
            category: AchievementCategory::Streaks,
            condition: UnlockCondition::TimeOfDay { start: 5, end: 8 },
            points: 25,
        },

        // ===== COMMAND-BASED ACHIEVEMENTS (5) =====
        Achievement {
            id: "ten_commands".to_string(),
            title: "Command Novice".to_string(),
            description: "Use 10 different commands. You're expanding your toolkit!".to_string(),
            icon: "\u{f120}", // Terminal
            category: AchievementCategory::Commands,
            condition: UnlockCondition::UseCommands(10),
            points: 40,
        },
        Achievement {
            id: "fifty_commands".to_string(),
            title: "Command Explorer".to_string(),
            description: "Use 50 unique commands. You're becoming versatile!".to_string(),
            icon: "\u{f489}", // Shell prompt
            category: AchievementCategory::Commands,
            condition: UnlockCondition::UseCommands(50),
            points: 100,
        },
        Achievement {
            id: "hundred_commands".to_string(),
            title: "Command Wizard".to_string(),
            description: "Use 100 different commands. Your command knowledge is vast!".to_string(),
            icon: "\u{f0d0}", // Magic wand
            category: AchievementCategory::Commands,
            condition: UnlockCondition::UseCommands(100),
            points: 200,
        },
        Achievement {
            id: "power_user".to_string(),
            title: "Power User".to_string(),
            description: "Use 200 unique commands. You're a true shell power user!".to_string(),
            icon: "\u{f0e7}", // Lightning bolt
            category: AchievementCategory::Commands,
            condition: UnlockCondition::UseCommands(200),
            points: 400,
        },
        Achievement {
            id: "command_master".to_string(),
            title: "Command Master".to_string(),
            description: "Use 500 different commands. You've mastered the shell!".to_string(),
            icon: "\u{f2db}", // Hundred points symbol
            category: AchievementCategory::Commands,
            condition: UnlockCondition::UseCommands(500),
            points: 1000,
        },

        // ===== EXPLORATION ACHIEVEMENTS (5) =====
        Achievement {
            id: "ai_assistant".to_string(),
            title: "AI Assistant".to_string(),
            description: "Use the AI assistant for help. Smart learning is efficient learning!".to_string(),
            icon: "\u{f544}", // Robot
            category: AchievementCategory::Exploration,
            condition: UnlockCondition::CompleteChallenge("use_ai".to_string()),
            points: 15,
        },
        Achievement {
            id: "theme_explorer".to_string(),
            title: "Theme Explorer".to_string(),
            description: "Change the theme. Personalization matters!".to_string(),
            icon: "\u{f53f}", // Palette
            category: AchievementCategory::Exploration,
            condition: UnlockCondition::CompleteChallenge("change_theme".to_string()),
            points: 10,
        },
        Achievement {
            id: "panel_master".to_string(),
            title: "Panel Master".to_string(),
            description: "Explore all panels in the interface. You know your way around!".to_string(),
            icon: "\u{f24e}", // Layout
            category: AchievementCategory::Exploration,
            condition: UnlockCondition::CompleteChallenge("visit_all_panels".to_string()),
            points: 20,
        },
        Achievement {
            id: "safe_hands".to_string(),
            title: "Safe Hands".to_string(),
            description: "The safety guard stopped a dangerous command, and you learned why.".to_string(),
            icon: "\u{f132}", // Shield
            category: AchievementCategory::Exploration,
            condition: UnlockCondition::CompleteChallenge("guard_refusal".to_string()),
            points: 10,
        },
        Achievement {
            id: "context_aware".to_string(),
            title: "Context Aware".to_string(),
            description: "Use the context detection feature. Understanding your environment is key!".to_string(),
            icon: "\u{f0eb}", // Lightbulb
            category: AchievementCategory::Exploration,
            condition: UnlockCondition::CompleteChallenge("use_context".to_string()),
            points: 15,
        },

        // ===== CHALLENGE ACHIEVEMENTS (3) =====
        Achievement {
            id: "speed_learner".to_string(),
            title: "Speed Learner".to_string(),
            description: "Complete a lesson in under 5 minutes. Quick and efficient!".to_string(),
            icon: "\u{f3fd}", // Stopwatch
            category: AchievementCategory::Challenges,
            condition: UnlockCondition::CompleteChallenge("speed_lesson".to_string()),
            points: 50,
        },
        Achievement {
            id: "perfect_lesson".to_string(),
            title: "Perfect Lesson".to_string(),
            description: "Complete a lesson without any mistakes. Flawless execution!".to_string(),
            icon: "\u{f005}", // Star
            category: AchievementCategory::Challenges,
            condition: UnlockCondition::CompleteChallenge("perfect_lesson".to_string()),
            points: 75,
        },
        Achievement {
            id: "challenge_accepted".to_string(),
            title: "Challenge Accepted".to_string(),
            description: "Complete your first challenge. You're not afraid to push yourself!".to_string(),
            icon: "\u{f140}", // Target
            category: AchievementCategory::Challenges,
            condition: UnlockCondition::CompleteChallenge("first_challenge".to_string()),
            points: 30,
        },
    ]
}

/// Get achievements by category
pub fn achievements_by_category(category: AchievementCategory) -> Vec<Achievement> {
    all_achievements()
        .into_iter()
        .filter(|a| a.category == category)
        .collect()
}

/// Get total available points in the system
pub fn total_available_points() -> u32 {
    all_achievements().iter().map(|a| a.points).sum()
}

/// Get achievement by ID
pub fn get_achievement(id: &str) -> Option<Achievement> {
    all_achievements().into_iter().find(|a| a.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_achievement_count() {
        let achievements = all_achievements();
        assert_eq!(achievements.len(), 35, "Should have exactly 35 achievements");
    }

    #[test]
    fn test_category_distribution() {
        let achievements = all_achievements();
        let lessons = achievements.iter().filter(|a| matches!(a.category, AchievementCategory::Lessons)).count();
        let streaks = achievements.iter().filter(|a| matches!(a.category, AchievementCategory::Streaks)).count();
        let commands = achievements.iter().filter(|a| matches!(a.category, AchievementCategory::Commands)).count();
        let challenges = achievements.iter().filter(|a| matches!(a.category, AchievementCategory::Challenges)).count();
        let exploration = achievements.iter().filter(|a| matches!(a.category, AchievementCategory::Exploration)).count();

        assert_eq!(lessons, 14, "Should have 14 lesson achievements");
        assert_eq!(streaks, 7, "Should have 7 streak achievements");
        assert_eq!(commands, 5, "Should have 5 command achievements");
        assert_eq!(challenges, 3, "Should have 3 challenge achievements");
        assert_eq!(exploration, 6, "Should have 6 exploration achievements");
    }

    #[test]
    fn test_unique_ids() {
        let achievements = all_achievements();
        let ids: HashSet<_> = achievements.iter().map(|a| &a.id).collect();
        assert_eq!(ids.len(), achievements.len(), "All achievement IDs should be unique");
    }

    #[test]
    fn test_user_achievements() {
        let mut user_achievements = UserAchievements::new();
        assert_eq!(user_achievements.total_unlocked(), 0);

        user_achievements.unlock("first_steps".to_string());
        assert!(user_achievements.is_unlocked("first_steps"));
        assert_eq!(user_achievements.total_unlocked(), 1);
    }
}
