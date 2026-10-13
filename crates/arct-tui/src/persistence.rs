//! Session persistence for command history and user progress

use anyhow::Result;
use arct_core::{ChallengeManager, UserStats};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Per-lesson resume state so users can pick up mid-lesson where they left off
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LessonResumeState {
    /// Index of the step the user was on (0-based)
    pub current_step_index: usize,
    /// Indices of steps already completed in this lesson run
    #[serde(default)]
    pub completed_steps: Vec<usize>,
}

/// Session data that gets persisted
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub command_history: Vec<String>,
    pub last_updated: String,
    /// User statistics and progress (achievements, streaks, lessons completed, etc.)
    pub user_stats: UserStats,
    /// Completed lesson IDs
    pub completed_lessons: HashSet<String>,
    /// Challenge manager state (daily/weekly challenges completed)
    pub challenge_manager: ChallengeManager,
    /// In-progress lesson state keyed by lesson id (step-level resume).
    /// `#[serde(default)]` keeps old session files loadable.
    #[serde(default)]
    pub lesson_progress: HashMap<String, LessonResumeState>,
}

impl SessionData {
    pub fn new() -> Self {
        Self {
            command_history: Vec::new(),
            last_updated: chrono::Local::now().to_rfc3339(),
            user_stats: UserStats::new(),
            completed_lessons: HashSet::new(),
            challenge_manager: ChallengeManager::new(),
            lesson_progress: HashMap::new(),
        }
    }
}

/// arct's data folder: $ARCT_DATA_DIR if set (portable installs, tests),
/// otherwise the platform's local data folder + /arct
pub fn arct_data_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("ARCT_DATA_DIR").filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    Ok(dirs::data_local_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find local data directory"))?
        .join("arct"))
}

/// Get the path to the session file
pub fn get_session_file_path() -> Result<PathBuf> {
    let arct_dir = arct_data_dir()?;

    // Create directory if it doesn't exist
    if !arct_dir.exists() {
        fs::create_dir_all(&arct_dir)?;
    }

    Ok(arct_dir.join("session.json"))
}

/// Path of the backup kept alongside the session file (last known-good copy)
fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

/// Path a corrupt session file is preserved at for post-mortem analysis
fn corrupt_path(path: &Path) -> PathBuf {
    path.with_extension("json.corrupt")
}

/// Save session data to disk
pub fn save_session(data: &SessionData) -> Result<()> {
    let path = get_session_file_path()?;
    save_session_to(&path, data)
}

/// Save session data to a specific path (atomic write with backup)
fn save_session_to(path: &Path, data: &SessionData) -> Result<()> {
    let json = serde_json::to_string_pretty(data)?;

    // Write to a temp file first, then rename over the target so a crash
    // mid-write can never truncate the real session file (rename is atomic
    // on POSIX)
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, &json)?;

    // Keep the previous good file around as a backup before replacing it
    if path.exists() {
        let _ = fs::copy(path, backup_path(path));
    }

    fs::rename(&tmp_path, path)?;
    Ok(())
}

/// Load session data from disk
pub fn load_session() -> Result<SessionData> {
    let path = get_session_file_path()?;
    load_session_from(&path)
}

/// Load session data from a specific path, recovering from the backup if the
/// primary file is corrupt
fn load_session_from(path: &Path) -> Result<SessionData> {
    if !path.exists() {
        return Ok(SessionData::new());
    }

    match read_session(path) {
        Ok(data) => Ok(data),
        Err(e) => {
            tracing::warn!(
                "Session file corrupt ({}), preserving as {} and trying backup",
                e,
                corrupt_path(path).display()
            );

            // Preserve the corrupt file for post-mortem instead of
            // overwriting it (this also prevents a later save from copying
            // the corrupt file over the good backup)
            let _ = fs::rename(path, corrupt_path(path));

            let backup = backup_path(path);
            if backup.exists() {
                match read_session(&backup) {
                    Ok(data) => {
                        tracing::info!("Recovered session from backup: {}", backup.display());
                        return Ok(data);
                    }
                    Err(e) => {
                        tracing::warn!("Session backup also unreadable: {}", e);
                    }
                }
            }

            Ok(SessionData::new())
        }
    }
}

/// Read and parse a session file
fn read_session(path: &Path) -> Result<SessionData> {
    let json = fs::read_to_string(path)?;
    let data: SessionData = serde_json::from_str(&json)?;
    Ok(data)
}

/// Clear session data (delete the file)
pub fn clear_session() -> Result<()> {
    let path = get_session_file_path()?;

    if path.exists() {
        fs::remove_file(path)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a fresh temp directory for a test and return the session path
    fn temp_session_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arct-persistence-test-{}-{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("session.json")
    }

    fn sample_session() -> SessionData {
        let mut data = SessionData::new();
        data.command_history.push("ls -la".to_string());
        data.command_history.push("echo hello".to_string());
        data
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let path = temp_session_path("roundtrip");
        let data = sample_session();

        save_session_to(&path, &data).unwrap();
        let loaded = load_session_from(&path).unwrap();

        assert_eq!(loaded.command_history, data.command_history);
        // Temp file must not be left behind
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn test_save_creates_backup_of_previous_file() {
        let path = temp_session_path("backup");
        let data = sample_session();

        save_session_to(&path, &data).unwrap();
        assert!(!backup_path(&path).exists());

        save_session_to(&path, &data).unwrap();
        assert!(backup_path(&path).exists());
    }

    #[test]
    fn test_corrupt_json_recovers_from_backup() {
        let path = temp_session_path("corrupt-recovery");
        let data = sample_session();

        // First save creates session.json; second save creates the .bak
        save_session_to(&path, &data).unwrap();
        save_session_to(&path, &data).unwrap();
        assert!(backup_path(&path).exists());

        // Corrupt the primary file
        fs::write(&path, "{ this is not json").unwrap();

        let loaded = load_session_from(&path).unwrap();
        assert_eq!(loaded.command_history, data.command_history);

        // Corrupt file preserved for post-mortem, primary moved aside
        assert!(corrupt_path(&path).exists());
        assert!(!path.exists());
    }

    #[test]
    fn test_truncated_file_without_backup_falls_back_to_empty() {
        let path = temp_session_path("truncated");
        let data = sample_session();
        let json = serde_json::to_string_pretty(&data).unwrap();

        // Simulate a crash mid-write of the old (non-atomic) code path
        fs::write(&path, &json[..json.len() / 2]).unwrap();

        let loaded = load_session_from(&path).unwrap();
        assert!(loaded.command_history.is_empty());

        // Truncated file preserved for post-mortem
        assert!(corrupt_path(&path).exists());
    }

    #[test]
    fn test_lesson_progress_roundtrip() {
        let path = temp_session_path("lesson-progress");
        let mut data = sample_session();
        data.lesson_progress.insert(
            "nav-basics".to_string(),
            LessonResumeState {
                current_step_index: 3,
                completed_steps: vec![0, 1, 2],
            },
        );

        save_session_to(&path, &data).unwrap();
        let loaded = load_session_from(&path).unwrap();

        let resume = loaded.lesson_progress.get("nav-basics").unwrap();
        assert_eq!(resume.current_step_index, 3);
        assert_eq!(resume.completed_steps, vec![0, 1, 2]);
    }

    #[test]
    fn test_old_session_without_lesson_progress_loads() {
        // Backward compat: a session file written before lesson_progress
        // existed must still load (field defaults to empty)
        let path = temp_session_path("old-format");
        let data = sample_session();
        let mut json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&data).unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("lesson_progress");
        fs::write(&path, serde_json::to_string(&json).unwrap()).unwrap();

        let loaded = load_session_from(&path).unwrap();
        assert!(loaded.lesson_progress.is_empty());
        assert_eq!(loaded.command_history, data.command_history);
    }

    #[test]
    fn test_missing_file_returns_new_session() {
        let path = temp_session_path("missing");
        let loaded = load_session_from(&path).unwrap();
        assert!(loaded.command_history.is_empty());
    }
}
