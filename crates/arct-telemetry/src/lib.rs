//! Privacy-first telemetry for Arc Academy Terminal
//!
//! This module provides opt-in, local telemetry that helps users understand
//! their learning patterns while respecting privacy.
//!
//! Privacy guarantees:
//! - Opt-in only (disabled by default)
//! - All data stored locally (never sent anywhere)
//! - No sensitive information collected (no command arguments, no file paths)
//! - Users can export and delete their data at any time
//! - Anonymous user IDs

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Telemetry event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TelemetryEvent {
    /// Application session started
    SessionStarted {
        session_id: String,
        version: String,
        os: String,
        arch: String,
    },

    /// Application session ended
    SessionEnded {
        session_id: String,
        duration_ms: u64,
    },

    /// Command executed (command name only, no arguments)
    CommandExecuted {
        command: String,
        success: bool,
        duration_ms: u64,
    },

    /// Feature used
    FeatureUsed {
        feature: String,
        context: Option<String>,
    },

    /// Error occurred
    ErrorOccurred {
        error_type: String,
        context: String,
    },

    /// Theme changed
    ThemeChanged {
        from: String,
        to: String,
    },

    /// Configuration changed
    ConfigChanged {
        key: String,
    },
}

/// Telemetry service
pub struct Telemetry {
    enabled: bool,
    user_id: String,
    db: Connection,
}

impl Telemetry {
    /// Create a new telemetry instance
    pub fn new(enabled: bool) -> Result<Self> {
        let db_path = get_telemetry_db_path()?;

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let db = Connection::open(&db_path)?;

        // Initialize database schema
        Self::init_db(&db)?;

        // Load or create user ID
        let user_id = Self::load_or_create_user_id(&db)?;

        Ok(Self {
            enabled,
            user_id,
            db,
        })
    }

    /// Initialize database schema
    fn init_db(db: &Connection) -> Result<()> {
        db.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS metadata (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                user_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                event_data TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
            CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
            CREATE INDEX IF NOT EXISTS idx_events_user ON events(user_id);
            "#,
        )?;

        Ok(())
    }

    /// Load or create anonymous user ID
    fn load_or_create_user_id(db: &Connection) -> Result<String> {
        // Try to load existing user ID
        let existing: Option<String> = db
            .query_row(
                "SELECT value FROM metadata WHERE key = 'user_id'",
                [],
                |row| row.get(0),
            )
            .ok();

        if let Some(user_id) = existing {
            return Ok(user_id);
        }

        // Create new anonymous user ID
        let user_id = Uuid::new_v4().to_string();
        db.execute(
            "INSERT INTO metadata (key, value) VALUES ('user_id', ?1)",
            params![&user_id],
        )?;

        Ok(user_id)
    }

    /// Record a telemetry event
    pub fn record(&self, event: TelemetryEvent) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let timestamp = Utc::now().to_rfc3339();
        let event_type = match &event {
            TelemetryEvent::SessionStarted { .. } => "session_started",
            TelemetryEvent::SessionEnded { .. } => "session_ended",
            TelemetryEvent::CommandExecuted { .. } => "command_executed",
            TelemetryEvent::FeatureUsed { .. } => "feature_used",
            TelemetryEvent::ErrorOccurred { .. } => "error_occurred",
            TelemetryEvent::ThemeChanged { .. } => "theme_changed",
            TelemetryEvent::ConfigChanged { .. } => "config_changed",
        };

        let event_data = serde_json::to_string(&event)?;

        self.db.execute(
            "INSERT INTO events (timestamp, user_id, event_type, event_data) VALUES (?1, ?2, ?3, ?4)",
            params![timestamp, &self.user_id, event_type, event_data],
        )?;

        Ok(())
    }

    /// Get telemetry statistics
    pub fn get_stats(&self) -> Result<TelemetryStats> {
        // If telemetry is disabled, return empty stats
        if !self.enabled {
            return Ok(TelemetryStats {
                total_commands: 0,
                total_sessions: 0,
                total_errors: 0,
                top_commands: vec![],
                features_used: vec![],
            });
        }

        let total_commands: i64 = self.db.query_row(
            "SELECT COUNT(*) FROM events WHERE event_type = 'command_executed'",
            [],
            |row| row.get(0),
        )?;

        let total_sessions: i64 = self.db.query_row(
            "SELECT COUNT(*) FROM events WHERE event_type = 'session_started'",
            [],
            |row| row.get(0),
        )?;

        let total_errors: i64 = self.db.query_row(
            "SELECT COUNT(*) FROM events WHERE event_type = 'error_occurred'",
            [],
            |row| row.get(0),
        )?;

        // Most used commands
        let mut stmt = self.db.prepare(
            "SELECT json_extract(event_data, '$.command') as cmd, COUNT(*) as count
             FROM events
             WHERE event_type = 'command_executed'
             GROUP BY cmd
             ORDER BY count DESC
             LIMIT 10"
        )?;

        let top_commands: Vec<(String, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(Result::ok)
            .collect();

        // Features used
        let mut stmt = self.db.prepare(
            "SELECT json_extract(event_data, '$.feature') as feature, COUNT(*) as count
             FROM events
             WHERE event_type = 'feature_used'
             GROUP BY feature
             ORDER BY count DESC
             LIMIT 10"
        )?;

        let features_used: Vec<(String, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(Result::ok)
            .collect();

        Ok(TelemetryStats {
            total_commands: total_commands as usize,
            total_sessions: total_sessions as usize,
            total_errors: total_errors as usize,
            top_commands,
            features_used,
        })
    }

    /// Export all telemetry data as JSON
    pub fn export_data(&self) -> Result<String> {
        let mut stmt = self.db.prepare(
            "SELECT timestamp, event_type, event_data FROM events ORDER BY timestamp"
        )?;

        let events: Vec<ExportedEvent> = stmt
            .query_map([], |row| {
                Ok(ExportedEvent {
                    timestamp: row.get(0)?,
                    event_type: row.get(1)?,
                    event_data: row.get(2)?,
                })
            })?
            .filter_map(Result::ok)
            .collect();

        let export = TelemetryExport {
            user_id: self.user_id.clone(),
            exported_at: Utc::now().to_rfc3339(),
            events,
        };

        Ok(serde_json::to_string_pretty(&export)?)
    }

    /// Delete all telemetry data
    pub fn delete_all_data(&self) -> Result<()> {
        self.db.execute("DELETE FROM events", [])?;
        Ok(())
    }

    /// Get database path
    pub fn db_path() -> Result<PathBuf> {
        get_telemetry_db_path()
    }
}

/// Telemetry statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryStats {
    pub total_commands: usize,
    pub total_sessions: usize,
    pub total_errors: usize,
    pub top_commands: Vec<(String, i64)>,
    pub features_used: Vec<(String, i64)>,
}

/// Exported event
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExportedEvent {
    timestamp: String,
    event_type: String,
    event_data: String,
}

/// Telemetry export
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TelemetryExport {
    user_id: String,
    exported_at: String,
    events: Vec<ExportedEvent>,
}

/// Get the telemetry database path
pub fn get_telemetry_db_path() -> Result<PathBuf> {
    // $ARCT_DATA_DIR overrides the location (portable installs, tests)
    let arct_dir = match std::env::var_os("ARCT_DATA_DIR").filter(|d| !d.is_empty()) {
        Some(dir) => std::path::PathBuf::from(dir),
        None => dirs::data_local_dir()
            .context("Could not find local data directory")?
            .join("arct"),
    };

    if !arct_dir.exists() {
        std::fs::create_dir_all(&arct_dir)
            .with_context(|| format!("Failed to create data directory: {}", arct_dir.display()))?;
    }

    Ok(arct_dir.join("telemetry.db"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_disabled() {
        let telemetry = Telemetry::new(false).unwrap();

        // Recording events should succeed but do nothing
        telemetry.record(TelemetryEvent::CommandExecuted {
            command: "ls".to_string(),
            success: true,
            duration_ms: 100,
        }).unwrap();

        let stats = telemetry.get_stats().unwrap();
        assert_eq!(stats.total_commands, 0);
    }

    #[test]
    fn test_telemetry_enabled() {
        let telemetry = Telemetry::new(true).unwrap();

        telemetry.record(TelemetryEvent::CommandExecuted {
            command: "ls".to_string(),
            success: true,
            duration_ms: 100,
        }).unwrap();

        let stats = telemetry.get_stats().unwrap();
        assert!(stats.total_commands > 0);
    }
}
