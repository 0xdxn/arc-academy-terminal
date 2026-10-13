//! Real-filesystem practice playground for lessons.
//!
//! When `lessons.practice_mode = "real"`, lesson commands run against the
//! real shell inside a dedicated directory (by default
//! `~/ArcAcademy/playground`). Each lesson gets its own subdirectory
//! (`<playground>/<lesson-id>/`) where its starter files (the lesson's
//! `setup` list) are materialized.
//!
//! Safety properties:
//! - `change_directory` canonicalizes and refuses to leave the playground.
//! - `reset_lesson` only ever deletes inside the playground root, and
//!   verifies the canonicalized path prefix before removing anything.
//! - Setup file paths are validated to be relative and free of `..`.

use crate::lesson::SetupFile;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// A real practice directory with a tracked working directory per session.
pub struct Playground {
    /// Canonicalized playground root on the real filesystem.
    root: PathBuf,
    /// Active lesson id (subdirectory name under the root), if any.
    lesson_id: Option<String>,
    /// Starter files for the active lesson (kept for `reset_lesson`).
    setup: Vec<SetupFile>,
    /// Tracked working directory (absolute, always inside `root`).
    cwd: PathBuf,
    /// How the root is shown to the learner (e.g. `~/ArcAcademy/playground`).
    display: String,
}

impl Playground {
    /// Open (creating on demand) the playground at `root`.
    pub fn open(root: PathBuf) -> Result<Self> {
        Self::open_with_display(root, crate::guard::PLAYGROUND_DISPLAY)
    }

    /// Open a playground whose paths are shown to the learner under
    /// `display` (e.g. `~/ArcAcademy/home`).
    pub fn open_with_display(root: PathBuf, display: impl Into<String>) -> Result<Self> {
        fs::create_dir_all(&root)
            .with_context(|| format!("Failed to create playground at {}", root.display()))?;
        let root = root
            .canonicalize()
            .context("Failed to canonicalize playground root")?;
        Ok(Self {
            cwd: root.clone(),
            root,
            lesson_id: None,
            setup: Vec::new(),
            display: display.into(),
        })
    }

    /// The canonicalized playground root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The tracked working directory (absolute real path inside the root).
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// Whether a lesson directory is currently active.
    pub fn has_lesson(&self) -> bool {
        self.lesson_id.is_some()
    }

    /// Friendly display path for the tracked cwd, e.g.
    /// `~/ArcAcademy/playground/file-mgmt/practice`.
    pub fn display_cwd(&self) -> String {
        match self.cwd.strip_prefix(&self.root) {
            Ok(rel) if rel.as_os_str().is_empty() => self.display.clone(),
            Ok(rel) => format!("{}/{}", self.display, rel.display()),
            Err(_) => self.cwd.display().to_string(),
        }
    }

    /// Enter a lesson: create `<root>/<lesson-id>/`, materialize its starter
    /// files (without clobbering files the learner already changed), and cd
    /// there. Returns the lesson directory.
    pub fn enter_lesson(&mut self, lesson_id: &str, setup: &[SetupFile]) -> Result<PathBuf> {
        anyhow::ensure!(
            !lesson_id.is_empty()
                && lesson_id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "Invalid lesson id for playground directory: {lesson_id}"
        );

        let dir = self.root.join(lesson_id);
        fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create lesson playground: {}", dir.display()))?;
        Self::materialize(&dir, setup, false)?;

        self.lesson_id = Some(lesson_id.to_string());
        self.setup = setup.to_vec();
        self.cwd = dir
            .canonicalize()
            .context("Failed to canonicalize lesson playground directory")?;
        Ok(self.cwd.clone())
    }

    /// Wipe and re-materialize the active lesson's playground directory.
    ///
    /// rm safety: only ever deletes inside the playground root — the target
    /// is canonicalized and its prefix verified before removal.
    pub fn reset_lesson(&mut self) -> Result<PathBuf> {
        let lesson_id = self
            .lesson_id
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No active lesson to reset"))?;

        let dir = self.root.join(&lesson_id);
        if dir.exists() {
            let canonical = dir
                .canonicalize()
                .context("Failed to canonicalize lesson directory before reset")?;
            // Never delete the root itself, and never delete anything whose
            // canonical path is not strictly inside the playground root.
            anyhow::ensure!(
                canonical != self.root && canonical.starts_with(&self.root),
                "Refusing to reset: {} is not inside the playground",
                canonical.display()
            );
            fs::remove_dir_all(&canonical).with_context(|| {
                format!("Failed to remove lesson playground: {}", canonical.display())
            })?;
        }

        fs::create_dir_all(&dir)?;
        let setup = self.setup.clone();
        Self::materialize(&dir, &setup, true)?;
        self.cwd = dir
            .canonicalize()
            .context("Failed to canonicalize lesson playground directory")?;
        Ok(self.cwd.clone())
    }

    /// Change the tracked working directory (the `cd` builtin for real
    /// practice mode). The target is canonicalized and must remain inside
    /// the playground. Returns the new display path.
    pub fn change_directory(&mut self, target: &str) -> Result<String> {
        let target = target.trim();
        let candidate = if target.is_empty() || target == "~" {
            // "cd" with no argument goes to the lesson directory (or root)
            match &self.lesson_id {
                Some(id) => self.root.join(id),
                None => self.root.clone(),
            }
        } else if target.starts_with('/') {
            PathBuf::from(target)
        } else {
            self.cwd.join(target)
        };

        let canonical = candidate
            .canonicalize()
            .map_err(|_| anyhow::anyhow!("No such directory: {target}"))?;

        anyhow::ensure!(
            canonical.starts_with(&self.root),
            "'{target}' leaves {} — commands here stay inside it",
            self.display
        );
        anyhow::ensure!(canonical.is_dir(), "Not a directory: {target}");

        self.cwd = canonical;
        Ok(self.display_cwd())
    }

    /// Write starter files under `dir`. Paths must be relative and must not
    /// contain `..`. With `overwrite = false`, existing files are preserved
    /// (so re-entering a lesson doesn't clobber the learner's work).
    fn materialize(dir: &Path, files: &[SetupFile], overwrite: bool) -> Result<()> {
        for file in files {
            let rel = Path::new(&file.path);
            let safe = rel.components().all(|c| matches!(c, Component::Normal(_)))
                && !rel.as_os_str().is_empty();
            anyhow::ensure!(safe, "Unsafe setup file path in lesson: {}", file.path);

            let dest = dir.join(rel);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            if overwrite || !dest.exists() {
                fs::write(&dest, &file.contents)
                    .with_context(|| format!("Failed to write setup file: {}", dest.display()))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arct-playground-test-{}-{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn sample_setup() -> Vec<SetupFile> {
        vec![
            SetupFile {
                path: "notes.txt".to_string(),
                contents: "starter\n".to_string(),
            },
            SetupFile {
                path: "logs/server.log".to_string(),
                contents: "line1\nline2\n".to_string(),
            },
        ]
    }

    #[test]
    fn test_open_creates_root_and_enter_materializes_setup() {
        let root = temp_root("enter");
        let mut pg = Playground::open(root.clone()).unwrap();
        assert!(pg.root().exists());
        assert_eq!(pg.cwd(), pg.root());

        let dir = pg.enter_lesson("file-mgmt", &sample_setup()).unwrap();
        assert!(dir.ends_with("file-mgmt"));
        assert_eq!(pg.cwd(), dir.as_path());
        assert_eq!(fs::read_to_string(dir.join("notes.txt")).unwrap(), "starter\n");
        assert_eq!(
            fs::read_to_string(dir.join("logs/server.log")).unwrap(),
            "line1\nline2\n"
        );
        assert_eq!(pg.display_cwd(), "~/ArcAcademy/playground/file-mgmt");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_reenter_preserves_learner_changes_but_reset_wipes() {
        let root = temp_root("reset");
        let mut pg = Playground::open(root.clone()).unwrap();
        let dir = pg.enter_lesson("file-viewing", &sample_setup()).unwrap();

        // Learner edits a starter file and adds a new one
        fs::write(dir.join("notes.txt"), "my edits\n").unwrap();
        fs::write(dir.join("extra.txt"), "keep?\n").unwrap();

        // Re-entering must NOT clobber the learner's work
        pg.enter_lesson("file-viewing", &sample_setup()).unwrap();
        assert_eq!(fs::read_to_string(dir.join("notes.txt")).unwrap(), "my edits\n");
        assert!(dir.join("extra.txt").exists());

        // Reset wipes and re-materializes pristine starter files
        pg.reset_lesson().unwrap();
        assert_eq!(fs::read_to_string(dir.join("notes.txt")).unwrap(), "starter\n");
        assert!(!dir.join("extra.txt").exists());
        assert!(dir.join("logs/server.log").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_reset_without_lesson_errors() {
        let root = temp_root("reset-none");
        let mut pg = Playground::open(root.clone()).unwrap();
        assert!(pg.reset_lesson().is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_change_directory_stays_contained() {
        let root = temp_root("cd");
        let mut pg = Playground::open(root.clone()).unwrap();
        pg.enter_lesson("nav", &sample_setup()).unwrap();

        // Into a subdirectory and back up
        pg.change_directory("logs").unwrap();
        assert!(pg.display_cwd().ends_with("nav/logs"));
        pg.change_directory("..").unwrap();
        assert!(pg.display_cwd().ends_with("nav"));

        // Up to the playground root is allowed
        pg.change_directory("..").unwrap();
        assert_eq!(pg.cwd(), pg.root());

        // Escaping above the root is refused, cwd unchanged
        assert!(pg.change_directory("..").is_err());
        assert_eq!(pg.cwd(), pg.root());
        assert!(pg.change_directory("/etc").is_err());
        assert!(pg.change_directory("../..").is_err());

        // Nonexistent directory errors
        assert!(pg.change_directory("nope").is_err());

        // Bare `cd` returns to the lesson directory
        pg.change_directory("").unwrap();
        assert!(pg.display_cwd().ends_with("nav"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_unsafe_setup_paths_rejected() {
        let root = temp_root("unsafe");
        let mut pg = Playground::open(root.clone()).unwrap();

        let evil = vec![SetupFile {
            path: "../escape.txt".to_string(),
            contents: "nope".to_string(),
        }];
        assert!(pg.enter_lesson("evil", &evil).is_err());
        assert!(!root.join("escape.txt").exists());

        let abs = vec![SetupFile {
            path: "/tmp/abs-escape.txt".to_string(),
            contents: "nope".to_string(),
        }];
        assert!(pg.enter_lesson("evil2", &abs).is_err());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_invalid_lesson_id_rejected() {
        let root = temp_root("badid");
        let mut pg = Playground::open(root.clone()).unwrap();
        assert!(pg.enter_lesson("../outside", &[]).is_err());
        assert!(pg.enter_lesson("", &[]).is_err());
        assert!(pg.enter_lesson("a/b", &[]).is_err());
        let _ = fs::remove_dir_all(&root);
    }
}

impl crate::lesson::PracticeWorld for Playground {
    fn locate(&self, lesson_path: &str) -> Option<PathBuf> {
        let base = match &self.lesson_id {
            Some(id) => self.root.join(id),
            None => self.root.clone(),
        };
        let rel = Path::new(lesson_path.trim_start_matches('/'));
        if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
            return None;
        }
        Some(base.join(rel))
    }

    fn cwd_relative(&self) -> String {
        let base = match &self.lesson_id {
            Some(id) => self.root.join(id),
            None => self.root.clone(),
        };
        self.cwd
            .strip_prefix(&base)
            .map(|rel| rel.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}
