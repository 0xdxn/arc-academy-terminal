//! User analytics and progress tracking

use anyhow::Result;
use chrono::Local;
use rusqlite::{Connection, params};
use std::path::PathBuf;

/// Analytics database for tracking user progress
pub struct Analytics {
    conn: Connection,
}

impl Analytics {
    /// Create or open the analytics database
    pub fn new() -> Result<Self> {
        let db_path = Self::get_db_path()?;
        let conn = Connection::open(db_path)?;

        // Create tables if they don't exist
        conn.execute(
            "CREATE TABLE IF NOT EXISTS commands (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                command TEXT NOT NULL,
                success INTEGER NOT NULL,
                working_dir TEXT,
                session_id TEXT
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                start_time TEXT NOT NULL,
                end_time TEXT,
                commands_count INTEGER DEFAULT 0
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS daily_stats (
                date TEXT PRIMARY KEY,
                commands_count INTEGER DEFAULT 0,
                unique_commands INTEGER DEFAULT 0,
                session_count INTEGER DEFAULT 0
            )",
            [],
        )?;

        Ok(Self { conn })
    }

    /// Get the database file path
    fn get_db_path() -> Result<PathBuf> {
        let arct_dir = crate::persistence::arct_data_dir()?;
        if !arct_dir.exists() {
            std::fs::create_dir_all(&arct_dir)?;
        }

        Ok(arct_dir.join("analytics.db"))
    }

    /// Record a command execution
    pub fn record_command(
        &self,
        command: &str,
        success: bool,
        working_dir: &str,
        session_id: &str,
    ) -> Result<()> {
        let timestamp = Local::now().to_rfc3339();

        self.conn.execute(
            "INSERT INTO commands (timestamp, command, success, working_dir, session_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![timestamp, command, success as i32, working_dir, session_id],
        )?;

        // Update daily stats
        self.update_daily_stats()?;

        Ok(())
    }

    /// Update daily statistics
    fn update_daily_stats(&self) -> Result<()> {
        let today = Local::now().format("%Y-%m-%d").to_string();

        // Count commands today
        let commands_today: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM commands WHERE DATE(timestamp) = ?1",
            params![today],
            |row| row.get(0),
        )?;

        // Count unique commands today
        let unique_today: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT command) FROM commands WHERE DATE(timestamp) = ?1",
            params![today],
            |row| row.get(0),
        )?;

        // Upsert daily stats
        self.conn.execute(
            "INSERT OR REPLACE INTO daily_stats (date, commands_count, unique_commands)
             VALUES (?1, ?2, ?3)",
            params![today, commands_today, unique_today],
        )?;

        Ok(())
    }

    /// Get total commands executed
    pub fn get_total_commands(&self) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM commands",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Get commands executed today
    pub fn get_commands_today(&self) -> Result<i64> {
        let today = Local::now().format("%Y-%m-%d").to_string();
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM commands WHERE DATE(timestamp) = ?1",
            params![today],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Get unique commands learned (all time)
    pub fn get_unique_commands(&self) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT command) FROM commands",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Get most used commands (top 5)
    pub fn get_top_commands(&self, limit: usize) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT command, COUNT(*) as count
             FROM commands
             GROUP BY command
             ORDER BY count DESC
             LIMIT ?1"
        )?;

        let commands = stmt.query_map(params![limit], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;

        Ok(commands)
    }

    /// Get current streak (consecutive days with activity)
    pub fn get_streak(&self) -> Result<i64> {
        let mut streak = 0i64;
        let mut current_date = Local::now().naive_local().date();

        loop {
            let date_str = current_date.format("%Y-%m-%d").to_string();
            let count: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM commands WHERE DATE(timestamp) = ?1",
                params![date_str],
                |row| row.get(0),
            )?;

            if count > 0 {
                streak += 1;
                current_date = current_date.pred_opt().unwrap_or(current_date);
            } else if streak == 0 {
                // No commands today, check if there were any yesterday
                current_date = current_date.pred_opt().unwrap_or(current_date);
                let date_str = current_date.format("%Y-%m-%d").to_string();
                let yesterday_count: i64 = self.conn.query_row(
                    "SELECT COUNT(*) FROM commands WHERE DATE(timestamp) = ?1",
                    params![date_str],
                    |row| row.get(0),
                )?;

                if yesterday_count > 0 {
                    streak = 1;
                    current_date = current_date.pred_opt().unwrap_or(current_date);
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(streak)
    }

    /// Calculate skill level based on command diversity and usage
    pub fn get_skill_level(&self) -> Result<String> {
        let unique = self.get_unique_commands()?;
        let _total = self.get_total_commands()?; // Reserved for future skill calculation refinement

        let level = if unique < 5 {
            "Beginner"
        } else if unique < 15 {
            "Learning"
        } else if unique < 30 {
            "Intermediate"
        } else if unique < 50 {
            "Advanced"
        } else {
            "Expert"
        };

        Ok(level.to_string())
    }

    /// Get commands executed in the last 7 days
    pub fn get_weekly_activity(&self) -> Result<Vec<(String, i64)>> {
        let mut activity = Vec::new();

        for days_ago in (0..7).rev() {
            let date = Local::now().naive_local().date() - chrono::Duration::days(days_ago);
            let date_str = date.format("%Y-%m-%d").to_string();

            let count: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM commands WHERE DATE(timestamp) = ?1",
                params![date_str],
                |row| row.get(0),
            ).unwrap_or(0);

            let day_name = date.format("%a").to_string();
            activity.push((day_name, count));
        }

        Ok(activity)
    }

    /// Per-day activity flags for the last `days` days (oldest first):
    /// `true` when at least one command was executed that day.
    /// Used by the progress panel's streak calendar strip.
    pub fn get_daily_activity(&self, days: usize) -> Result<Vec<bool>> {
        let mut activity = Vec::with_capacity(days);

        for days_ago in (0..days as i64).rev() {
            let date = Local::now().naive_local().date() - chrono::Duration::days(days_ago);
            let date_str = date.format("%Y-%m-%d").to_string();

            let count: i64 = self
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM commands WHERE DATE(timestamp) = ?1",
                    params![date_str],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            activity.push(count > 0);
        }

        Ok(activity)
    }
}

/// User analytics summary for display
#[derive(Debug, Clone)]
pub struct AnalyticsSummary {
    pub total_commands: i64,
    pub commands_today: i64,
    pub unique_commands: i64,
    pub streak: i64,
    pub skill_level: String,
    pub top_commands: Vec<(String, i64)>,
}

impl Analytics {
    /// Get a summary of all analytics
    pub fn get_summary(&self) -> Result<AnalyticsSummary> {
        Ok(AnalyticsSummary {
            total_commands: self.get_total_commands()?,
            commands_today: self.get_commands_today()?,
            unique_commands: self.get_unique_commands()?,
            streak: self.get_streak()?,
            skill_level: self.get_skill_level()?,
            top_commands: self.get_top_commands(3)?,
        })
    }
}
