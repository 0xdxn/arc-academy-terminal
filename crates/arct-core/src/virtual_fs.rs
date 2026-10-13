//! Virtual filesystem for lesson sandboxing
//!
//! Provides a safe, isolated filesystem for lessons so users can practice
//! without affecting their real system.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Virtual filesystem manager for lessons
pub struct VirtualFileSystem {
    /// Root directory of the virtual filesystem
    root: PathBuf,
    /// Current working directory (relative to root)
    current_dir: PathBuf,
    /// Session ID for cleanup and debugging
    #[allow(dead_code)]
    session_id: String,
}

impl VirtualFileSystem {
    /// Create a new virtual filesystem for a lesson
    pub fn new(lesson_id: &str, session_id: &str) -> Result<Self> {
        let root = Self::create_temp_root(lesson_id, session_id)?;
        let current_dir = PathBuf::from("/lesson-home");

        let mut vfs = Self {
            root,
            current_dir,
            session_id: session_id.to_string(),
        };

        // Initialize the filesystem structure
        vfs.initialize_structure()?;

        Ok(vfs)
    }

    /// Create temporary root directory
    fn create_temp_root(lesson_id: &str, session_id: &str) -> Result<PathBuf> {
        let temp_dir = std::env::temp_dir();
        let root = temp_dir.join(format!("arct-lesson-{}-{}", lesson_id, session_id));

        fs::create_dir_all(&root)
            .context("Failed to create virtual filesystem root")?;

        Ok(root)
    }

    /// Initialize the virtual filesystem structure
    fn initialize_structure(&mut self) -> Result<()> {
        let home = self.root.join("lesson-home");
        fs::create_dir_all(&home)?;

        // Create standard directories
        let dirs = [
            "Documents",
            "Documents/homework",
            "Downloads",
            "Pictures",
            "Pictures/family",
            "Music",
            "Videos",
            "projects",
            "projects/website",
        ];

        for dir in &dirs {
            fs::create_dir_all(home.join(dir))?;
        }

        // Create sample files
        self.create_file("Documents/report.txt", "# Quarterly Report\n\nThis is a sample document.\n")?;
        self.create_file("Documents/notes.md", "# Notes\n\n- Learn Linux commands\n- Practice navigation\n")?;
        self.create_file("Downloads/software.zip", "[Binary data - sample file]\n")?;
        self.create_file("Pictures/vacation.jpg", "[Image data - sample file]\n")?;
        self.create_file("Pictures/family/photo.png", "[Image data - sample file]\n")?;
        self.create_file("projects/website/index.html", "<!DOCTYPE html>\n<html>\n<head><title>My Site</title></head>\n<body><h1>Hello World</h1></body>\n</html>\n")?;
        self.create_file("projects/website/style.css", "body { font-family: Arial; }\n")?;
        self.create_file(".bashrc", "# Bash configuration\nexport PS1='$ '\n")?;

        Ok(())
    }

    /// Create a file with content
    fn create_file(&self, path: &str, content: &str) -> Result<()> {
        let full_path = self.root.join("lesson-home").join(path);
        fs::write(full_path, content)?;
        Ok(())
    }

    /// Build the absolute *virtual* path (rooted at /lesson-home) for user input,
    /// without resolving `.` or `..` components yet.
    fn virtual_target(&self, virtual_path: &str) -> PathBuf {
        if virtual_path == "~" || virtual_path.is_empty() {
            return PathBuf::from("/lesson-home");
        }

        let path = Path::new(virtual_path);

        // If absolute path starting with /lesson-home
        if path.starts_with("/lesson-home") {
            return path.to_path_buf();
        }

        // If absolute path starting with /
        if path.is_absolute() {
            return PathBuf::from("/lesson-home").join(path.strip_prefix("/").unwrap_or(path));
        }

        // Relative path - join with current dir
        self.current_dir.join(path)
    }

    /// Lexically resolve `.` and `..` components of an absolute virtual path.
    ///
    /// This is deliberately lexical (no `fs::canonicalize`) so it works for
    /// paths that don't exist yet (touch, mkdir). Returns an error if the
    /// path would escape the `/lesson-home` sandbox.
    fn normalize_virtual(path: &Path) -> Result<PathBuf> {
        use std::path::Component;

        let mut normalized = PathBuf::from("/");
        for component in path.components() {
            match component {
                Component::RootDir | Component::Prefix(_) | Component::CurDir => {}
                Component::ParentDir => {
                    if !normalized.pop() {
                        anyhow::bail!("path escapes lesson sandbox");
                    }
                }
                Component::Normal(part) => normalized.push(part),
            }
        }

        if !normalized.starts_with("/lesson-home") {
            anyhow::bail!("path escapes lesson sandbox");
        }

        Ok(normalized)
    }

    /// Get the real filesystem path for a virtual path.
    ///
    /// All `.`/`..` components are resolved lexically and the result is
    /// guaranteed to be contained within the sandbox root; paths that would
    /// escape it (e.g. `../../../../etc/hostname`) return an error.
    pub fn resolve_path(&self, virtual_path: &str) -> Result<PathBuf> {
        let virtual_abs = Self::normalize_virtual(&self.virtual_target(virtual_path))?;
        let relative = virtual_abs.strip_prefix("/").unwrap_or(&virtual_abs);
        let real = self.root.join(relative);

        // Defense in depth: never hand out a path outside the sandbox root.
        if !real.starts_with(&self.root) {
            anyhow::bail!("path escapes lesson sandbox");
        }

        Ok(real)
    }

    /// Get current directory (virtual path)
    pub fn get_current_dir(&self) -> &Path {
        &self.current_dir
    }

    /// Get current directory (real filesystem path)
    pub fn get_current_dir_real(&self) -> PathBuf {
        if self.current_dir == PathBuf::from("/lesson-home") {
            self.root.join("lesson-home")
        } else {
            let relative = self.current_dir.strip_prefix("/lesson-home").unwrap_or(&self.current_dir);
            self.root.join("lesson-home").join(relative)
        }
    }

    /// Change directory (returns new virtual path)
    pub fn change_directory(&mut self, path: &str) -> Result<String> {
        let target = if path == ".." && self.current_dir == Path::new("/lesson-home") {
            // 'cd ..' at the sandbox root clamps to the root instead of escaping
            PathBuf::from("/lesson-home")
        } else {
            // Resolve '.'/'..' lexically; errors if the path escapes the sandbox
            Self::normalize_virtual(&self.virtual_target(path))?
        };

        // Verify the directory exists
        let relative = target.strip_prefix("/").unwrap_or(&target);
        let real_path = self.root.join(relative);
        if !real_path.exists() {
            anyhow::bail!("Directory not found: {}", path);
        }
        if !real_path.is_dir() {
            anyhow::bail!("Not a directory: {}", path);
        }

        self.current_dir = target.clone();
        Ok(target.to_string_lossy().to_string())
    }

    /// List directory contents
    pub fn list_directory(&self, path: Option<&str>) -> Result<Vec<DirEntry>> {
        let real_path = if let Some(p) = path {
            self.resolve_path(p)?
        } else {
            self.get_current_dir_real()
        };

        let mut entries = Vec::new();

        for entry in fs::read_dir(&real_path)
            .context("Failed to read directory")?
        {
            let entry = entry?;
            let metadata = entry.metadata()?;

            entries.push(DirEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                is_dir: metadata.is_dir(),
                size: metadata.len(),
            });
        }

        // Sort: directories first, then alphabetically
        entries.sort_by(|a, b| {
            match (a.is_dir, b.is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });

        Ok(entries)
    }

    /// Get directory tree structure
    pub fn get_tree(&self, max_depth: usize) -> Vec<TreeNode> {
        let root = self.root.join("lesson-home");
        self.build_tree(&root, 0, max_depth)
    }

    /// Recursively build tree structure
    fn build_tree(&self, path: &Path, depth: usize, max_depth: usize) -> Vec<TreeNode> {
        if depth >= max_depth {
            return vec![];
        }

        let mut nodes = Vec::new();

        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let metadata = entry.metadata().ok();
                let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                let name = entry.file_name().to_string_lossy().to_string();

                let children = if is_dir {
                    self.build_tree(&entry.path(), depth + 1, max_depth)
                } else {
                    vec![]
                };

                nodes.push(TreeNode {
                    name,
                    is_dir,
                    children,
                    is_current: self.get_current_dir_real() == entry.path(),
                });
            }
        }

        // Sort: directories first
        nodes.sort_by(|a, b| {
            match (a.is_dir, b.is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });

        nodes
    }

    /// Read file contents (cat command)
    pub fn read_file(&self, path: &str) -> Result<String> {
        let real_path = self.resolve_path(path)?;

        if !real_path.exists() {
            anyhow::bail!("No such file or directory: {}", path);
        }

        if real_path.is_dir() {
            anyhow::bail!("Is a directory: {}", path);
        }

        fs::read_to_string(&real_path)
            .context(format!("Failed to read file: {}", path))
    }

    /// Create directory (mkdir command)
    pub fn create_directory(&self, path: &str, parents: bool) -> Result<()> {
        let real_path = self.resolve_path(path)?;

        // mkdir -p is fine with a folder that's already there
        if parents && real_path.is_dir() {
            return Ok(());
        }
        if real_path.exists() {
            anyhow::bail!("File or directory already exists: {}", path);
        }

        if parents {
            fs::create_dir_all(&real_path)
        } else {
            fs::create_dir(&real_path)
        }
        .context(format!("Failed to create directory: {}", path))
    }

    /// Create or update file (touch command)
    pub fn touch_file(&self, path: &str) -> Result<()> {
        let real_path = self.resolve_path(path)?;

        if real_path.exists() {
            // Update modification time
            let metadata = fs::metadata(&real_path)?;
            let permissions = metadata.permissions();
            fs::set_permissions(&real_path, permissions)?;
        } else {
            // Create empty file
            fs::write(&real_path, "")?;
        }

        Ok(())
    }

    /// Remove file or directory (rm command)
    pub fn remove(&self, path: &str, recursive: bool, force: bool) -> Result<()> {
        let real_path = self.resolve_path(path)?;

        if !real_path.exists() {
            if force {
                return Ok(()); // -f flag ignores non-existent files
            }
            anyhow::bail!("No such file or directory: {}", path);
        }

        if real_path.is_dir() {
            if !recursive {
                anyhow::bail!("Is a directory (use -r to remove): {}", path);
            }
            fs::remove_dir_all(&real_path)
        } else {
            fs::remove_file(&real_path)
        }
        .context(format!("Failed to remove: {}", path))
    }

    /// Move or rename file/directory (mv command)
    pub fn move_item(&self, source: &str, destination: &str) -> Result<()> {
        let source_path = self.resolve_path(source)?;
        let dest_path = self.resolve_path(destination)?;

        if !source_path.exists() {
            anyhow::bail!("No such file or directory: {}", source);
        }

        // If destination is a directory, move into it with same name
        let final_dest = if dest_path.exists() && dest_path.is_dir() {
            dest_path.join(source_path.file_name().unwrap_or_default())
        } else {
            dest_path
        };

        fs::rename(&source_path, &final_dest)
            .context(format!("Failed to move {} to {}", source, destination))
    }

    /// Copy file or directory (cp command)
    pub fn copy(&self, source: &str, destination: &str, recursive: bool) -> Result<()> {
        let source_path = self.resolve_path(source)?;
        let dest_path = self.resolve_path(destination)?;

        if !source_path.exists() {
            anyhow::bail!("No such file or directory: {}", source);
        }

        if source_path.is_dir() {
            if !recursive {
                anyhow::bail!("Is a directory (use -r to copy): {}", source);
            }

            // Recursive directory copy
            self.copy_dir_recursive(&source_path, &dest_path)?;
        } else {
            // File copy
            let final_dest = if dest_path.exists() && dest_path.is_dir() {
                dest_path.join(source_path.file_name().unwrap_or_default())
            } else {
                dest_path
            };

            fs::copy(&source_path, &final_dest)
                .context(format!("Failed to copy {} to {}", source, destination))?;
        }

        Ok(())
    }

    /// Helper: Recursively copy directory
    fn copy_dir_recursive(&self, source: &Path, destination: &Path) -> Result<()> {
        fs::create_dir_all(destination)?;

        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let dest_path = destination.join(entry.file_name());

            if file_type.is_dir() {
                self.copy_dir_recursive(&entry.path(), &dest_path)?;
            } else {
                fs::copy(entry.path(), dest_path)?;
            }
        }

        Ok(())
    }

    /// Search a file for lines containing `pattern` (grep command).
    ///
    /// Returns the matching lines as `(line_number, line)` pairs (line
    /// numbers are 1-based). Matching is plain substring search; pass
    /// `case_insensitive` for `-i` behavior.
    pub fn grep_file(
        &self,
        pattern: &str,
        path: &str,
        case_insensitive: bool,
    ) -> Result<Vec<(usize, String)>> {
        let content = self.read_file(path)?;
        let needle = if case_insensitive {
            pattern.to_lowercase()
        } else {
            pattern.to_string()
        };

        let mut matches = Vec::new();
        for (idx, line) in content.lines().enumerate() {
            let haystack = if case_insensitive {
                line.to_lowercase()
            } else {
                line.to_string()
            };
            if haystack.contains(&needle) {
                matches.push((idx + 1, line.to_string()));
            }
        }
        Ok(matches)
    }

    /// Return the first `n` lines of a file (head command)
    pub fn head_file(&self, path: &str, n: usize) -> Result<String> {
        let content = self.read_file(path)?;
        let mut out = String::new();
        for line in content.lines().take(n) {
            out.push_str(line);
            out.push('\n');
        }
        Ok(out)
    }

    /// Return the last `n` lines of a file (tail command)
    pub fn tail_file(&self, path: &str, n: usize) -> Result<String> {
        let content = self.read_file(path)?;
        let lines: Vec<&str> = content.lines().collect();
        let start = lines.len().saturating_sub(n);
        let mut out = String::new();
        for line in &lines[start..] {
            out.push_str(line);
            out.push('\n');
        }
        Ok(out)
    }

    /// Count lines, words and bytes of a file (wc command).
    ///
    /// Returns `(lines, words, bytes)`.
    pub fn wc_file(&self, path: &str) -> Result<(usize, usize, usize)> {
        let content = self.read_file(path)?;
        let lines = content.lines().count();
        let words = content.split_whitespace().count();
        let bytes = content.len();
        Ok((lines, words, bytes))
    }

    /// Write content to file (for echo redirection, etc.)
    pub fn write_file(&self, path: &str, content: &str, append: bool) -> Result<()> {
        let real_path = self.resolve_path(path)?;

        if append {
            use std::io::Write;
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&real_path)?;
            file.write_all(content.as_bytes())?;
        } else {
            fs::write(&real_path, content)?;
        }

        Ok(())
    }

    /// Seed lesson starter files (the lesson's `setup` list) into the
    /// sandbox, creating parent directories as needed. Paths are resolved
    /// through the same containment checks as every other operation, so a
    /// malicious setup path cannot escape the sandbox.
    pub fn seed_setup(&self, files: &[crate::lesson::SetupFile]) -> Result<()> {
        for file in files {
            let real = self.resolve_path(&file.path)?;
            if let Some(parent) = real.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&real, &file.contents)
                .with_context(|| format!("Failed to seed lesson file: {}", file.path))?;
        }
        Ok(())
    }

    /// Clean up virtual filesystem
    pub fn cleanup(&self) -> Result<()> {
        if self.root.exists() {
            fs::remove_dir_all(&self.root)
                .context("Failed to cleanup virtual filesystem")?;
        }
        Ok(())
    }
}

impl Drop for VirtualFileSystem {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

/// Directory entry
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// Tree node for visualization
#[derive(Debug, Clone)]
pub struct TreeNode {
    pub name: String,
    pub is_dir: bool,
    pub children: Vec<TreeNode>,
    pub is_current: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_vfs() {
        let vfs = VirtualFileSystem::new("test", "session-123").unwrap();
        assert!(vfs.root.exists());
        assert_eq!(vfs.get_current_dir(), Path::new("/lesson-home"));
    }

    #[test]
    fn test_change_directory() {
        let mut vfs = VirtualFileSystem::new("test", "session-456").unwrap();

        // Change to Documents
        let new_path = vfs.change_directory("Documents").unwrap();
        assert_eq!(new_path, "/lesson-home/Documents");

        // Go back up
        let new_path = vfs.change_directory("..").unwrap();
        assert_eq!(new_path, "/lesson-home");
    }

    #[test]
    fn test_list_directory() {
        let vfs = VirtualFileSystem::new("test", "session-789").unwrap();
        let entries = vfs.list_directory(None).unwrap();

        // Should have Documents, Downloads, Pictures, Music, Videos, projects, .bashrc
        assert!(entries.len() >= 7);

        // Directories should be first
        assert!(entries[0].is_dir || entries[0].name == ".bashrc");
    }

    fn assert_escape_err<T: std::fmt::Debug>(result: Result<T>) {
        let err = result.expect_err("operation should have been rejected");
        assert!(
            err.to_string().contains("escapes lesson sandbox"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_cat_traversal_escape_rejected() {
        let vfs = VirtualFileSystem::new("test", "session-cat-escape").unwrap();
        assert_escape_err(vfs.read_file("../../../../etc/hostname"));
        assert_escape_err(vfs.read_file("/lesson-home/../../etc/hostname"));
    }

    #[test]
    fn test_rm_traversal_escape_rejected() {
        let vfs = VirtualFileSystem::new("test", "session-rm-escape").unwrap();
        // Even with recursive + force, escaping paths must error, not silently pass
        assert_escape_err(vfs.remove("../../..", true, true));
        assert_escape_err(vfs.remove("../outside.txt", false, false));
    }

    #[test]
    fn test_cd_traversal_escape_rejected() {
        let mut vfs = VirtualFileSystem::new("test", "session-cd-escape").unwrap();
        assert_escape_err(vfs.change_directory("../.."));
        assert_escape_err(vfs.change_directory("Documents/../../.."));

        // Current directory must be unchanged after rejected attempts
        assert_eq!(vfs.get_current_dir(), Path::new("/lesson-home"));
    }

    #[test]
    fn test_mv_traversal_escape_rejected() {
        let vfs = VirtualFileSystem::new("test", "session-mv-escape").unwrap();
        assert_escape_err(vfs.move_item("Documents/report.txt", "../../stolen.txt"));
        assert_escape_err(vfs.move_item("../../../etc/hostname", "here.txt"));
    }

    #[test]
    fn test_cp_traversal_escape_rejected() {
        let vfs = VirtualFileSystem::new("test", "session-cp-escape").unwrap();
        assert_escape_err(vfs.copy("Documents/report.txt", "../../stolen.txt", false));
        assert_escape_err(vfs.copy("../../../etc/hostname", "here.txt", false));
    }

    #[test]
    fn test_mkdir_touch_write_traversal_escape_rejected() {
        let vfs = VirtualFileSystem::new("test", "session-mk-escape").unwrap();
        assert_escape_err(vfs.create_directory("../evil-dir", true));
        assert_escape_err(vfs.touch_file("../evil.txt"));
        assert_escape_err(vfs.write_file("../evil.txt", "payload", false));
        assert_escape_err(vfs.list_directory(Some("../..")));
    }

    #[test]
    fn test_cd_dotdot_at_root_clamps() {
        let mut vfs = VirtualFileSystem::new("test", "session-cd-root").unwrap();

        // 'cd ..' at the sandbox root stays at the root (does not escape)
        let path = vfs.change_directory("..").unwrap();
        assert_eq!(path, "/lesson-home");
        assert_eq!(vfs.get_current_dir(), Path::new("/lesson-home"));
    }

    #[test]
    fn test_grep_file() {
        let vfs = VirtualFileSystem::new("test", "session-grep").unwrap();
        vfs.write_file("log.txt", "ok line\nERROR one\nfine\nerror two\n", false)
            .unwrap();

        // Case-sensitive: only the lowercase match
        let matches = vfs.grep_file("error", "log.txt", false).unwrap();
        assert_eq!(matches, vec![(4, "error two".to_string())]);

        // Case-insensitive (-i): both matches, with 1-based line numbers
        let matches = vfs.grep_file("error", "log.txt", true).unwrap();
        assert_eq!(
            matches,
            vec![(2, "ERROR one".to_string()), (4, "error two".to_string())]
        );

        // grep must not escape the sandbox
        assert_escape_err(vfs.grep_file("root", "../../../../etc/passwd", false));
    }

    #[test]
    fn test_head_and_tail_file() {
        let vfs = VirtualFileSystem::new("test", "session-headtail").unwrap();
        let body = (1..=20).map(|i| format!("line{}\n", i)).collect::<String>();
        vfs.write_file("nums.txt", &body, false).unwrap();

        let head = vfs.head_file("nums.txt", 3).unwrap();
        assert_eq!(head, "line1\nline2\nline3\n");

        let tail = vfs.tail_file("nums.txt", 2).unwrap();
        assert_eq!(tail, "line19\nline20\n");

        // Requesting more lines than exist returns the whole file
        let all = vfs.head_file("nums.txt", 100).unwrap();
        assert_eq!(all.lines().count(), 20);
        let all = vfs.tail_file("nums.txt", 100).unwrap();
        assert_eq!(all.lines().count(), 20);
    }

    #[test]
    fn test_wc_file() {
        let vfs = VirtualFileSystem::new("test", "session-wc").unwrap();
        vfs.write_file("data.txt", "one two\nthree\n", false).unwrap();

        let (lines, words, bytes) = vfs.wc_file("data.txt").unwrap();
        assert_eq!(lines, 2);
        assert_eq!(words, 3);
        assert_eq!(bytes, 14);
    }

    #[test]
    fn test_legitimate_relative_paths_still_work() {
        let mut vfs = VirtualFileSystem::new("test", "session-legit").unwrap();

        // Dotted paths that stay inside the sandbox are fine
        let content = vfs.read_file("Documents/../Documents/report.txt").unwrap();
        assert!(content.contains("Quarterly Report"));

        // cd into a nested dir, then navigate with ..
        vfs.change_directory("Pictures/family").unwrap();
        let path = vfs.change_directory("..").unwrap();
        assert_eq!(path, "/lesson-home/Pictures");

        // Relative operations from a subdirectory work and stay contained
        vfs.create_directory("holiday", false).unwrap();
        vfs.touch_file("holiday/list.txt").unwrap();
        vfs.copy("family/../vacation.jpg", "holiday/copy.jpg", false).unwrap();
        vfs.move_item("holiday/copy.jpg", "holiday/moved.jpg").unwrap();
        assert!(vfs.resolve_path("holiday/moved.jpg").unwrap().exists());

        // Absolute virtual paths resolve inside the sandbox root
        let resolved = vfs.resolve_path("/lesson-home/Documents").unwrap();
        assert!(resolved.starts_with(&vfs.root));
    }
}

impl crate::lesson::PracticeWorld for VirtualFileSystem {
    fn locate(&self, lesson_path: &str) -> Option<PathBuf> {
        let path = lesson_path.trim_start_matches('/');
        self.resolve_path(&format!("/lesson-home/{path}")).ok()
    }

    fn cwd_relative(&self) -> String {
        self.current_dir
            .strip_prefix("/lesson-home")
            .map(|rel| rel.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}
