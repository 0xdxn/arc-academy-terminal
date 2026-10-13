//! How the streak is put into words, in one place so every screen agrees.
//!
//! The streak counts learning days. Up to two days off between them are
//! rest days and keep it going; a longer break starts a new run. None of
//! the wording scolds: a missed day is never "broken" or "lost".

use arct_core::{StreakStatus, TrackProgress, Track};

/// One line describing the streak, for the dashboard and progress screens
pub fn describe(status: StreakStatus) -> String {
    match status {
        StreakStatus::NotStarted => {
            "Your first learning day starts with your first command.".to_string()
        }
        StreakStatus::LearnedToday { days: 1 } => "1 learning day so far.".to_string(),
        StreakStatus::LearnedToday { days } => format!("{days} learning days and counting."),
        StreakStatus::Resting { days, rest_days_left: 0 } => {
            format!("{days} learning days. Learn today and it keeps growing.")
        }
        StreakStatus::Resting { days, .. } => {
            format!("{days} learning days. Taking a rest day is fine.")
        }
        StreakStatus::WelcomeBack { best } => {
            format!("Your best run so far is {best} learning days. Today can start a new one.")
        }
    }
}

/// Short form for narrow panels: "4 learning days"
pub fn short(status: StreakStatus) -> String {
    let days = match status {
        StreakStatus::LearnedToday { days } | StreakStatus::Resting { days, .. } => days,
        StreakStatus::NotStarted | StreakStatus::WelcomeBack { .. } => 0,
    };
    match days {
        1 => "1 learning day".to_string(),
        n => format!("{n} learning days"),
    }
}

/// Display name of a track
pub fn track_name(track: Track) -> &'static str {
    match track {
        Track::Explorer => "Explorer",
        Track::Operator => "Operator",
    }
}

/// One line per track: how far along it is, and what's been earned
pub fn track_line(progress: &TrackProgress) -> String {
    let name = track_name(progress.track);
    if progress.is_mastered() {
        format!(
            "{name}: all {} lessons, every one without needing the answer. Certificate: arct certificate --track {}",
            progress.total,
            name.to_lowercase()
        )
    } else if progress.is_complete() {
        format!(
            "{name}: all {} lessons done ({} without needing the answer). Certificate: arct certificate --track {}",
            progress.total,
            progress.mastered,
            name.to_lowercase()
        )
    } else {
        format!(
            "{name}: {} of {} lessons done.",
            progress.completed.len(),
            progress.total
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_wording_scolds() {
        let all = [
            StreakStatus::NotStarted,
            StreakStatus::LearnedToday { days: 1 },
            StreakStatus::LearnedToday { days: 9 },
            StreakStatus::Resting { days: 3, rest_days_left: 1 },
            StreakStatus::Resting { days: 3, rest_days_left: 0 },
            StreakStatus::WelcomeBack { best: 12 },
        ];
        for status in all {
            let text = describe(status).to_lowercase();
            for word in ["broke", "lost", "lose", "miss", "fail", "don't", "hurry"] {
                assert!(!text.contains(word), "{text:?} contains {word:?}");
            }
        }
    }

    #[test]
    fn short_form_counts_days() {
        assert_eq!(short(StreakStatus::LearnedToday { days: 1 }), "1 learning day");
        assert_eq!(
            short(StreakStatus::Resting { days: 4, rest_days_left: 2 }),
            "4 learning days"
        );
        assert_eq!(short(StreakStatus::WelcomeBack { best: 7 }), "0 learning days");
    }
}
