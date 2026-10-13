//! Virtual filesystem for lesson sandboxing
//!
//! Provides a safe, isolated filesystem for lessons so users can practice
//! without affecting their real system.
//!
//! The practice files live in one of two places: a private temp directory
//! on disk (the terminal app), or a tree held in memory (the browser, which
//! has no filesystem). Every path goes through the same lexical containment
//! check first, and every operation below is written once on top of a few
//! primitives (stat, read, write, list, ...), so the two behave the same.

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The virtual path of the lesson home, where every sandbox starts
const HOME: &str = "/lesson-home";

/// Virtual filesystem manager for lessons
pub struct VirtualFileSystem {
    /// Where the files are kept
    store: Store,
    /// Current working directory (relative to root)
    current_dir: PathBuf,
    /// Session ID for cleanup and debugging
    #[allow(dead_code)]
    session_id: String,
}

/// Where a sandbox keeps its files
enum Store {
    /// Under a private temp directory, removed when the sandbox is dropped
    Disk { root: PathBuf },
    /// In memory, keyed by virtual path (`/lesson-home/...`)
    Memory(Mutex<MemTree>),
}

/// What is at a sandbox path
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Meta {
    pub is_dir: bool,
    /// Size in bytes. For folders, whatever the backend reports (the disk
    /// reports its own; the memory backend says 4096, as ext4 does).
    pub len: u64,
    /// Permission bits (`0o755`), where the platform keeps them
    pub mode: Option<u32>,
}

/// One entry of a folder, as the primitives list it
pub(crate) struct Child {
    name: String,
    is_dir: bool,
    is_symlink: bool,
    len: u64,
}

impl VirtualFileSystem {
    /// Create a new virtual filesystem for a lesson, on disk, holding the
    /// generic practice home (see [`VirtualFileSystem::seed_generic`])
    pub fn new(lesson_id: &str, session_id: &str) -> Result<Self> {
        let vfs = Self::empty(lesson_id, session_id)?;

        // Initialize the filesystem structure
        vfs.seed_generic()?;

        Ok(vfs)
    }

    /// Create an empty virtual filesystem on disk: just the lesson home
    pub fn empty(lesson_id: &str, session_id: &str) -> Result<Self> {
        let root = Self::create_temp_root(lesson_id, session_id)?;
        fs::create_dir_all(root.join("lesson-home"))?;

        Ok(Self {
            store: Store::Disk { root },
            current_dir: PathBuf::from(HOME),
            session_id: session_id.to_string(),
        })
    }

    /// Create an empty virtual filesystem held in memory: just the lesson
    /// home. Nothing is ever written to disk.
    pub fn in_memory() -> Self {
        let mut tree = MemTree::default();
        tree.nodes.insert(HOME.to_string(), Node::Dir { mode: DIR_MODE });
        Self {
            store: Store::Memory(Mutex::new(tree)),
            current_dir: PathBuf::from(HOME),
            session_id: String::new(),
        }
    }

    /// A sandbox on disk for `lesson`: its own setup files, plus the
    /// generic practice home for Operator lessons. Explorer lessons see
    /// only the files they set up. A setup file that can't be written is
    /// logged and skipped.
    pub fn for_lesson(lesson: &crate::lesson::Lesson, session_id: &str) -> Result<Self> {
        let vfs = Self::empty(&lesson.id, session_id)?;
        vfs.prepare_for(lesson)?;
        Ok(vfs)
    }

    /// [`VirtualFileSystem::for_lesson`], held in memory
    pub fn for_lesson_in_memory(lesson: &crate::lesson::Lesson) -> Result<Self> {
        let vfs = Self::in_memory();
        vfs.prepare_for(lesson)?;
        Ok(vfs)
    }

    fn prepare_for(&self, lesson: &crate::lesson::Lesson) -> Result<()> {
        if lesson.track == crate::lesson::Track::Operator {
            self.seed_generic()?;
        }
        if let Err(e) = self.seed_setup(&lesson.setup) {
            tracing::warn!("Failed to seed lesson setup files: {}", e);
        }
        Ok(())
    }

    /// Whether this sandbox is held in memory (rather than on disk)
    pub fn is_in_memory(&self) -> bool {
        matches!(self.store, Store::Memory(_))
    }

    /// Create temporary root directory
    fn create_temp_root(lesson_id: &str, session_id: &str) -> Result<PathBuf> {
        let temp_dir = std::env::temp_dir();
        let root = temp_dir.join(format!("arct-lesson-{}-{}", lesson_id, session_id));

        fs::create_dir_all(&root)
            .context("Failed to create virtual filesystem root")?;

        Ok(root)
    }

    /// Seed the generic practice home: Documents, Downloads, Pictures,
    /// Music, Videos, projects and a `.bashrc`, with a few sample files
    pub fn seed_generic(&self) -> Result<()> {
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
            self.prim_create_dir_all(&self.home_path(dir))?;
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

    /// Virtual path of a path under the lesson home
    fn home_path(&self, path: &str) -> PathBuf {
        Path::new(HOME).join(path)
    }

    /// Create a file with content
    fn create_file(&self, path: &str, content: &str) -> Result<()> {
        self.prim_write(&self.home_path(path), content.as_bytes(), false)?;
        Ok(())
    }

    /// Build the absolute *virtual* path (rooted at /lesson-home) for user input,
    /// without resolving `.` or `..` components yet.
    fn virtual_target(&self, virtual_path: &str) -> PathBuf {
        if virtual_path == "~" || virtual_path.is_empty() {
            return PathBuf::from(HOME);
        }

        let path = Path::new(virtual_path);

        // If absolute path starting with /lesson-home
        if path.starts_with(HOME) {
            return path.to_path_buf();
        }

        // If absolute path starting with /
        if path.is_absolute() {
            return PathBuf::from(HOME).join(path.strip_prefix("/").unwrap_or(path));
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

        if !normalized.starts_with(HOME) {
            anyhow::bail!("path escapes lesson sandbox");
        }

        Ok(normalized)
    }

    /// The normalized virtual path for user input, or an error if it would
    /// leave the sandbox. Every operation starts here.
    pub(crate) fn virtual_path(&self, path: &str) -> Result<PathBuf> {
        Self::normalize_virtual(&self.virtual_target(path))
    }

    /// Where a normalized virtual path lives: a real path for the disk
    /// backend, the virtual path itself for the memory backend
    fn real_of(&self, virtual_abs: &Path) -> Result<PathBuf> {
        match &self.store {
            Store::Disk { root } => {
                let relative = virtual_abs.strip_prefix("/").unwrap_or(virtual_abs);
                let real = root.join(relative);

                // Defense in depth: never hand out a path outside the sandbox root.
                if !real.starts_with(root) {
                    anyhow::bail!("path escapes lesson sandbox");
                }

                Ok(real)
            }
            Store::Memory(_) => Ok(virtual_abs.to_path_buf()),
        }
    }

    /// Get the real filesystem path for a virtual path.
    ///
    /// All `.`/`..` components are resolved lexically and the result is
    /// guaranteed to be contained within the sandbox root; paths that would
    /// escape it (e.g. `../../../../etc/hostname`) return an error. A
    /// sandbox held in memory has no real paths: it returns the normalized
    /// virtual path (`/lesson-home/...`), so use [`VirtualFileSystem::metadata`]
    /// and friends rather than `std::fs` on the result.
    pub fn resolve_path(&self, virtual_path: &str) -> Result<PathBuf> {
        let virtual_abs = self.virtual_path(virtual_path)?;
        self.real_of(&virtual_abs)
    }

    /// Get current directory (virtual path)
    pub fn get_current_dir(&self) -> &Path {
        &self.current_dir
    }

    /// Get current directory (real filesystem path; the virtual path for a
    /// sandbox held in memory)
    pub fn get_current_dir_real(&self) -> PathBuf {
        self.real_of(&self.current_dir)
            .unwrap_or_else(|_| self.current_dir.clone())
    }

    /// What is at a path, or `None` when nothing is (or the path would
    /// leave the sandbox)
    pub fn metadata(&self, path: &str) -> Option<Meta> {
        self.prim_stat(&self.virtual_path(path).ok()?)
    }

    /// Something exists at this path
    pub fn exists(&self, path: &str) -> bool {
        self.metadata(path).is_some()
    }

    /// A folder exists at this path
    pub fn is_dir(&self, path: &str) -> bool {
        self.metadata(path).is_some_and(|m| m.is_dir)
    }

    /// A file exists at this path
    pub fn is_file(&self, path: &str) -> bool {
        self.metadata(path).is_some_and(|m| !m.is_dir)
    }

    /// Change directory (returns new virtual path)
    pub fn change_directory(&mut self, path: &str) -> Result<String> {
        let target = if path == ".." && self.current_dir == Path::new(HOME) {
            // 'cd ..' at the sandbox root clamps to the root instead of escaping
            PathBuf::from(HOME)
        } else {
            // Resolve '.'/'..' lexically; errors if the path escapes the sandbox
            Self::normalize_virtual(&self.virtual_target(path))?
        };

        // Verify the directory exists
        match self.prim_stat(&target) {
            None => anyhow::bail!("Directory not found: {}", path),
            Some(meta) if !meta.is_dir => anyhow::bail!("Not a directory: {}", path),
            Some(_) => {}
        }

        self.current_dir = target.clone();
        Ok(target.to_string_lossy().to_string())
    }

    /// List directory contents
    pub fn list_directory(&self, path: Option<&str>) -> Result<Vec<DirEntry>> {
        let virtual_abs = match path {
            Some(p) => self.virtual_path(p)?,
            None => self.current_dir.clone(),
        };

        let mut entries: Vec<DirEntry> = self
            .prim_list(&virtual_abs)
            .context("Failed to read directory")?
            .into_iter()
            .map(|child| DirEntry {
                name: child.name,
                is_dir: child.is_dir,
                size: child.len,
            })
            .collect();

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
        self.build_tree(Path::new(HOME), 0, max_depth)
    }

    /// Recursively build tree structure
    fn build_tree(&self, path: &Path, depth: usize, max_depth: usize) -> Vec<TreeNode> {
        if depth >= max_depth {
            return vec![];
        }

        let mut nodes = Vec::new();

        if let Ok(entries) = self.prim_list(path) {
            for entry in entries {
                let child = path.join(&entry.name);
                let children = if entry.is_dir {
                    self.build_tree(&child, depth + 1, max_depth)
                } else {
                    vec![]
                };

                nodes.push(TreeNode {
                    name: entry.name,
                    is_dir: entry.is_dir,
                    children,
                    is_current: self.current_dir == child,
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
        let virtual_abs = self.virtual_path(path)?;

        match self.prim_stat(&virtual_abs) {
            None => anyhow::bail!("No such file or directory: {}", path),
            Some(meta) if meta.is_dir => anyhow::bail!("Is a directory: {}", path),
            Some(_) => {}
        }

        self.prim_read(&virtual_abs)
            .map_err(anyhow::Error::from)
            .and_then(|bytes| String::from_utf8(bytes).map_err(anyhow::Error::from))
            .context(format!("Failed to read file: {}", path))
    }

    /// Read a file's raw bytes
    pub fn read_bytes(&self, path: &str) -> Result<Vec<u8>> {
        Ok(self.prim_read(&self.virtual_path(path)?)?)
    }

    /// Write raw bytes to a file, replacing what it held
    pub fn write_bytes(&self, path: &str, bytes: &[u8]) -> Result<()> {
        Ok(self.prim_write(&self.virtual_path(path)?, bytes, false)?)
    }

    /// Set a path's permission bits. On a platform without them (a disk
    /// sandbox outside Unix) this does nothing.
    pub fn set_mode(&self, path: &str, mode: u32) -> Result<()> {
        Ok(self.prim_set_mode(&self.virtual_path(path)?, mode)?)
    }

    /// Create directory (mkdir command)
    pub fn create_directory(&self, path: &str, parents: bool) -> Result<()> {
        let virtual_abs = self.virtual_path(path)?;
        let existing = self.prim_stat(&virtual_abs);

        // mkdir -p is fine with a folder that's already there
        if parents && existing.is_some_and(|m| m.is_dir) {
            return Ok(());
        }
        if existing.is_some() {
            anyhow::bail!("File or directory already exists: {}", path);
        }

        if parents {
            self.prim_create_dir_all(&virtual_abs)
        } else {
            self.prim_create_dir(&virtual_abs)
        }
        .context(format!("Failed to create directory: {}", path))
    }

    /// Create or update file (touch command)
    pub fn touch_file(&self, path: &str) -> Result<()> {
        let virtual_abs = self.virtual_path(path)?;

        // An existing file is left as it is (the sandbox keeps no times);
        // a new one is created empty
        if self.prim_stat(&virtual_abs).is_none() {
            self.prim_write(&virtual_abs, b"", false)?;
        }

        Ok(())
    }

    /// Remove file or directory (rm command)
    pub fn remove(&self, path: &str, recursive: bool, force: bool) -> Result<()> {
        let virtual_abs = self.virtual_path(path)?;

        let Some(meta) = self.prim_stat(&virtual_abs) else {
            if force {
                return Ok(()); // -f flag ignores non-existent files
            }
            anyhow::bail!("No such file or directory: {}", path);
        };

        if meta.is_dir {
            if !recursive {
                anyhow::bail!("Is a directory (use -r to remove): {}", path);
            }
            self.prim_remove_dir_all(&virtual_abs)
        } else {
            self.prim_remove_file(&virtual_abs)
        }
        .context(format!("Failed to remove: {}", path))
    }

    /// Move or rename file/directory (mv command)
    pub fn move_item(&self, source: &str, destination: &str) -> Result<()> {
        let source_path = self.virtual_path(source)?;
        let dest_path = self.virtual_path(destination)?;

        if self.prim_stat(&source_path).is_none() {
            anyhow::bail!("No such file or directory: {}", source);
        }

        // If destination is a directory, move into it with same name
        let final_dest = if self.prim_stat(&dest_path).is_some_and(|m| m.is_dir) {
            dest_path.join(source_path.file_name().unwrap_or_default())
        } else {
            dest_path
        };

        self.prim_rename(&source_path, &final_dest)
            .context(format!("Failed to move {} to {}", source, destination))
    }

    /// Copy file or directory (cp command)
    pub fn copy(&self, source: &str, destination: &str, recursive: bool) -> Result<()> {
        let source_path = self.virtual_path(source)?;
        let dest_path = self.virtual_path(destination)?;

        let Some(meta) = self.prim_stat(&source_path) else {
            anyhow::bail!("No such file or directory: {}", source);
        };

        if meta.is_dir {
            if !recursive {
                anyhow::bail!("Is a directory (use -r to copy): {}", source);
            }
            // A folder copied into itself would never finish
            if dest_path.starts_with(&source_path) {
                anyhow::bail!(
                    "cannot copy a directory, '{}', into itself, '{}'",
                    source,
                    destination
                );
            }

            // Recursive directory copy
            self.copy_dir_recursive(&source_path, &dest_path)?;
        } else {
            // File copy
            let final_dest = if self.prim_stat(&dest_path).is_some_and(|m| m.is_dir) {
                dest_path.join(source_path.file_name().unwrap_or_default())
            } else {
                dest_path
            };

            self.prim_copy_file(&source_path, &final_dest)
                .context(format!("Failed to copy {} to {}", source, destination))?;
        }

        Ok(())
    }

    /// Helper: Recursively copy directory
    fn copy_dir_recursive(&self, source: &Path, destination: &Path) -> Result<()> {
        self.prim_create_dir_all(destination)?;

        for entry in self.prim_list(source)? {
            let from = source.join(&entry.name);
            let to = destination.join(&entry.name);

            if entry.is_dir {
                self.copy_dir_recursive(&from, &to)?;
            } else {
                self.prim_copy_file(&from, &to)?;
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
        let virtual_abs = self.virtual_path(path)?;
        self.prim_write(&virtual_abs, content.as_bytes(), append)?;
        Ok(())
    }

    /// Seed lesson starter files (the lesson's `setup` list) into the
    /// sandbox, creating parent directories as needed. Paths are resolved
    /// through the same containment checks as every other operation, so a
    /// malicious setup path cannot escape the sandbox.
    pub fn seed_setup(&self, files: &[crate::lesson::SetupFile]) -> Result<()> {
        for file in files {
            let virtual_abs = self.virtual_path(&file.path)?;
            if let Some(parent) = virtual_abs.parent() {
                self.prim_create_dir_all(parent)?;
            }
            self.prim_write(&virtual_abs, file.contents.as_bytes(), false)
                .with_context(|| format!("Failed to seed lesson file: {}", file.path))?;
        }
        Ok(())
    }

    /// Clean up virtual filesystem
    pub fn cleanup(&self) -> Result<()> {
        match &self.store {
            Store::Disk { root } => {
                if root.exists() {
                    fs::remove_dir_all(root)
                        .context("Failed to cleanup virtual filesystem")?;
                }
            }
            Store::Memory(tree) => tree_lock(tree).nodes.clear(),
        }
        Ok(())
    }
}

// ----------------------------------------------------------------------------
// Primitives: the only code that knows where the files are. Paths here are
// normalized virtual paths (`/lesson-home/...`), already checked.
// ----------------------------------------------------------------------------

/// Mode of a new file and a new folder (as with umask 022)
const FILE_MODE: u32 = 0o644;
const DIR_MODE: u32 = 0o755;
/// What the memory backend reports as a folder's size
const DIR_SIZE: u64 = 4096;

fn tree_lock(tree: &Mutex<MemTree>) -> std::sync::MutexGuard<'_, MemTree> {
    tree.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl VirtualFileSystem {
    pub(crate) fn prim_stat(&self, path: &Path) -> Option<Meta> {
        match &self.store {
            Store::Disk { .. } => {
                let meta = fs::metadata(self.real_of(path).ok()?).ok()?;
                #[cfg(unix)]
                let mode = {
                    use std::os::unix::fs::PermissionsExt;
                    Some(meta.permissions().mode())
                };
                #[cfg(not(unix))]
                let mode = None;
                Some(Meta {
                    is_dir: meta.is_dir(),
                    len: meta.len(),
                    mode,
                })
            }
            Store::Memory(tree) => tree_lock(tree).stat(&key(path)),
        }
    }

    /// Like prim_stat, but a symbolic link on disk is described as itself,
    /// never followed (find, du and tar start here)
    pub(crate) fn prim_lstat(&self, path: &Path) -> Option<Meta> {
        match &self.store {
            Store::Disk { .. } => {
                let meta = fs::symlink_metadata(self.real_of(path).ok()?).ok()?;
                #[cfg(unix)]
                let mode = {
                    use std::os::unix::fs::PermissionsExt;
                    Some(meta.permissions().mode())
                };
                #[cfg(not(unix))]
                let mode = None;
                Some(Meta {
                    is_dir: meta.is_dir(),
                    len: meta.len(),
                    mode,
                })
            }
            Store::Memory(tree) => tree_lock(tree).stat(&key(path)),
        }
    }

    pub(crate) fn prim_read(&self, path: &Path) -> io::Result<Vec<u8>> {
        match &self.store {
            Store::Disk { .. } => fs::read(self.real_io(path)?),
            Store::Memory(tree) => tree_lock(tree).read(&key(path)),
        }
    }

    pub(crate) fn prim_write(&self, path: &Path, bytes: &[u8], append: bool) -> io::Result<()> {
        match &self.store {
            Store::Disk { .. } => {
                let real = self.real_io(path)?;
                if append {
                    use std::io::Write;
                    let mut file = fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&real)?;
                    file.write_all(bytes)
                } else {
                    fs::write(&real, bytes)
                }
            }
            Store::Memory(tree) => tree_lock(tree).write(&key(path), bytes, append),
        }
    }

    pub(crate) fn prim_list(&self, path: &Path) -> io::Result<Vec<Child>> {
        match &self.store {
            Store::Disk { .. } => {
                let mut out = Vec::new();
                for entry in fs::read_dir(self.real_io(path)?)? {
                    let entry = entry?;
                    // Not followed: a link is listed as itself
                    let meta = entry.metadata()?;
                    out.push(Child {
                        name: entry.file_name().to_string_lossy().to_string(),
                        is_dir: meta.is_dir(),
                        is_symlink: meta.file_type().is_symlink(),
                        len: meta.len(),
                    });
                }
                Ok(out)
            }
            Store::Memory(tree) => tree_lock(tree).list(&key(path)),
        }
    }

    pub(crate) fn prim_create_dir(&self, path: &Path) -> io::Result<()> {
        match &self.store {
            Store::Disk { .. } => fs::create_dir(self.real_io(path)?),
            Store::Memory(tree) => tree_lock(tree).create_dir(&key(path)),
        }
    }

    pub(crate) fn prim_create_dir_all(&self, path: &Path) -> io::Result<()> {
        match &self.store {
            Store::Disk { .. } => fs::create_dir_all(self.real_io(path)?),
            Store::Memory(tree) => tree_lock(tree).create_dir_all(&key(path)),
        }
    }

    pub(crate) fn prim_remove_file(&self, path: &Path) -> io::Result<()> {
        match &self.store {
            Store::Disk { .. } => fs::remove_file(self.real_io(path)?),
            Store::Memory(tree) => tree_lock(tree).remove_file(&key(path)),
        }
    }

    pub(crate) fn prim_remove_dir_all(&self, path: &Path) -> io::Result<()> {
        match &self.store {
            Store::Disk { .. } => fs::remove_dir_all(self.real_io(path)?),
            Store::Memory(tree) => tree_lock(tree).remove_dir_all(&key(path)),
        }
    }

    pub(crate) fn prim_rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        match &self.store {
            Store::Disk { .. } => fs::rename(self.real_io(from)?, self.real_io(to)?),
            Store::Memory(tree) => tree_lock(tree).rename(&key(from), &key(to)),
        }
    }

    /// Copy a file's contents and permission bits, as `std::fs::copy` does
    pub(crate) fn prim_copy_file(&self, from: &Path, to: &Path) -> io::Result<()> {
        match &self.store {
            Store::Disk { .. } => fs::copy(self.real_io(from)?, self.real_io(to)?).map(|_| ()),
            Store::Memory(tree) => tree_lock(tree).copy_file(&key(from), &key(to)),
        }
    }

    pub(crate) fn prim_set_mode(&self, path: &Path, mode: u32) -> io::Result<()> {
        match &self.store {
            Store::Disk { .. } => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(self.real_io(path)?, fs::Permissions::from_mode(mode))
                }
                #[cfg(not(unix))]
                {
                    let _ = (path, mode);
                    Ok(())
                }
            }
            Store::Memory(tree) => tree_lock(tree).set_mode(&key(path), mode),
        }
    }

    /// The disk path for a virtual path, as an io error when it can't be had
    fn real_io(&self, path: &Path) -> io::Result<PathBuf> {
        self.real_of(path)
            .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e.to_string()))
    }

    /// A folder's children for walking (find, du, tar): sorted by name,
    /// links left out (never followed)
    pub(crate) fn walk_children(&self, dir: &Path) -> Vec<(String, PathBuf, bool)> {
        let mut out: Vec<(String, PathBuf, bool)> = self
            .prim_list(dir)
            .unwrap_or_default()
            .into_iter()
            .filter(|child| !child.is_symlink)
            .map(|child| {
                let path = dir.join(&child.name);
                (child.name, path, child.is_dir)
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// Write a tar entry for a file. On disk the header comes from the
    /// file itself; in memory from what the tree keeps (no times or owners).
    pub(crate) fn tar_append_file<W: io::Write>(
        &self,
        builder: &mut tar::Builder<W>,
        path: &Path,
        name: &Path,
    ) -> Result<()> {
        match &self.store {
            Store::Disk { .. } => builder.append_path_with_name(self.real_of(path)?, name)?,
            Store::Memory(_) => {
                let data = self.prim_read(path)?;
                let mode = self.prim_stat(path).and_then(|m| m.mode).unwrap_or(FILE_MODE);
                let mut header = tar::Header::new_gnu();
                header.set_entry_type(tar::EntryType::Regular);
                header.set_size(data.len() as u64);
                header.set_mode(mode);
                header.set_mtime(0);
                builder.append_data(&mut header, name, &data[..])?;
            }
        }
        Ok(())
    }

    /// Write a tar entry for a folder (its contents are added separately)
    pub(crate) fn tar_append_dir<W: io::Write>(
        &self,
        builder: &mut tar::Builder<W>,
        path: &Path,
        name: &Path,
    ) -> Result<()> {
        match &self.store {
            Store::Disk { .. } => builder.append_dir(name, self.real_of(path)?)?,
            Store::Memory(_) => {
                let mode = self.prim_stat(path).and_then(|m| m.mode).unwrap_or(DIR_MODE);
                let mut header = tar::Header::new_gnu();
                header.set_entry_type(tar::EntryType::Directory);
                header.set_size(0);
                header.set_mode(mode);
                header.set_mtime(0);
                builder.append_data(&mut header, name, io::empty())?;
            }
        }
        Ok(())
    }
}

// ----------------------------------------------------------------------------
// The in-memory tree
// ----------------------------------------------------------------------------

/// A file or folder held in memory
#[derive(Debug, Clone)]
enum Node {
    Dir { mode: u32 },
    File { data: Vec<u8>, mode: u32 },
}

/// Every path in a sandbox held in memory, by virtual path. Errors read
/// like the operating system's (without its error numbers).
#[derive(Debug, Default)]
struct MemTree {
    nodes: BTreeMap<String, Node>,
}

/// Largest file the in-memory world holds, and all files together: a
/// browser tab has to stay usable whatever a learner types
const MEM_MAX_FILE: usize = 4 * 1024 * 1024;
const MEM_MAX_TOTAL: usize = 32 * 1024 * 1024;

fn no_space() -> io::Error {
    os_error("No space left on device")
}

fn key(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn not_found() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "No such file or directory")
}

fn os_error(message: &str) -> io::Error {
    io::Error::other(message.to_string())
}

/// The folder a key is in ("/" for a top-level key)
fn parent_key(key: &str) -> &str {
    match key.rsplit_once('/') {
        Some(("", _)) | None => "/",
        Some((parent, _)) => parent,
    }
}

impl MemTree {
    fn stat(&self, key: &str) -> Option<Meta> {
        Some(match self.nodes.get(key)? {
            Node::Dir { mode } => Meta { is_dir: true, len: DIR_SIZE, mode: Some(*mode) },
            Node::File { data, mode } => Meta {
                is_dir: false,
                len: data.len() as u64,
                mode: Some(*mode),
            },
        })
    }

    /// The parent folder must exist for something to be made in it
    fn check_parent(&self, key: &str) -> io::Result<()> {
        let parent = parent_key(key);
        if parent == "/" {
            return Ok(());
        }
        match self.nodes.get(parent) {
            Some(Node::Dir { .. }) => Ok(()),
            Some(Node::File { .. }) => Err(os_error("Not a directory")),
            None => Err(not_found()),
        }
    }

    /// Keys of everything inside a folder, at any depth
    fn descendants(&self, key: &str) -> Vec<String> {
        let prefix = format!("{key}/");
        self.nodes
            .range(prefix.clone()..)
            .take_while(|(k, _)| k.starts_with(&prefix))
            .map(|(k, _)| k.clone())
            .collect()
    }

    fn read(&self, key: &str) -> io::Result<Vec<u8>> {
        match self.nodes.get(key) {
            Some(Node::File { data, .. }) => Ok(data.clone()),
            Some(Node::Dir { .. }) => Err(os_error("Is a directory")),
            None => Err(not_found()),
        }
    }

    fn write(&mut self, key: &str, bytes: &[u8], append: bool) -> io::Result<()> {
        let current = match self.nodes.get(key) {
            Some(Node::File { data, .. }) => data.len(),
            _ => 0,
        };
        let new_len = if append { current.saturating_add(bytes.len()) } else { bytes.len() };
        let total: usize = self
            .nodes
            .values()
            .map(|n| match n {
                Node::File { data, .. } => data.len(),
                _ => 0,
            })
            .sum();
        if new_len > MEM_MAX_FILE || total - current + new_len > MEM_MAX_TOTAL {
            return Err(no_space());
        }
        match self.nodes.get_mut(key) {
            Some(Node::File { data, .. }) => {
                if !append {
                    data.clear();
                }
                data.extend_from_slice(bytes);
                Ok(())
            }
            Some(Node::Dir { .. }) => Err(os_error("Is a directory")),
            None => {
                self.check_parent(key)?;
                self.nodes.insert(
                    key.to_string(),
                    Node::File { data: bytes.to_vec(), mode: FILE_MODE },
                );
                Ok(())
            }
        }
    }

    fn list(&self, key: &str) -> io::Result<Vec<Child>> {
        match self.nodes.get(key) {
            Some(Node::Dir { .. }) => {}
            Some(Node::File { .. }) => return Err(os_error("Not a directory")),
            None => return Err(not_found()),
        }
        let prefix = format!("{key}/");
        Ok(self
            .nodes
            .range(prefix.clone()..)
            .take_while(|(k, _)| k.starts_with(&prefix))
            .filter(|(k, _)| !k[prefix.len()..].contains('/'))
            .map(|(k, node)| Child {
                name: k[prefix.len()..].to_string(),
                is_dir: matches!(node, Node::Dir { .. }),
                is_symlink: false,
                len: match node {
                    Node::Dir { .. } => DIR_SIZE,
                    Node::File { data, .. } => data.len() as u64,
                },
            })
            .collect())
    }

    fn create_dir(&mut self, key: &str) -> io::Result<()> {
        if self.nodes.contains_key(key) {
            return Err(os_error("File exists"));
        }
        self.check_parent(key)?;
        self.nodes.insert(key.to_string(), Node::Dir { mode: DIR_MODE });
        Ok(())
    }

    fn create_dir_all(&mut self, key: &str) -> io::Result<()> {
        if key == "/" || key.is_empty() {
            return Ok(());
        }
        match self.nodes.get(key) {
            Some(Node::Dir { .. }) => Ok(()),
            Some(Node::File { .. }) => Err(os_error("File exists")),
            None => {
                self.create_dir_all(parent_key(key))?;
                self.nodes.insert(key.to_string(), Node::Dir { mode: DIR_MODE });
                Ok(())
            }
        }
    }

    fn remove_file(&mut self, key: &str) -> io::Result<()> {
        match self.nodes.get(key) {
            Some(Node::File { .. }) => {
                self.nodes.remove(key);
                Ok(())
            }
            Some(Node::Dir { .. }) => Err(os_error("Is a directory")),
            None => Err(not_found()),
        }
    }

    fn remove_dir_all(&mut self, key: &str) -> io::Result<()> {
        match self.nodes.get(key) {
            Some(Node::Dir { .. }) => {
                for inner in self.descendants(key) {
                    self.nodes.remove(&inner);
                }
                self.nodes.remove(key);
                Ok(())
            }
            Some(Node::File { .. }) => Err(os_error("Not a directory")),
            None => Err(not_found()),
        }
    }

    /// `rename(2)`: replaces a file, or an empty folder with a folder
    fn rename(&mut self, from: &str, to: &str) -> io::Result<()> {
        let Some(moving) = self.nodes.get(from).cloned() else {
            return Err(not_found());
        };
        if from == to {
            return Ok(());
        }
        if to.starts_with(&format!("{from}/")) {
            return Err(os_error("Invalid argument"));
        }
        self.check_parent(to)?;
        match (&moving, self.nodes.get(to)) {
            (Node::File { .. }, Some(Node::Dir { .. })) => return Err(os_error("Is a directory")),
            (Node::Dir { .. }, Some(Node::File { .. })) => return Err(os_error("Not a directory")),
            (Node::Dir { .. }, Some(Node::Dir { .. })) if !self.descendants(to).is_empty() => {
                return Err(os_error("Directory not empty"))
            }
            _ => {}
        }
        let inner = self.descendants(from);
        self.nodes.remove(from);
        self.nodes.insert(to.to_string(), moving);
        for old in inner {
            if let Some(node) = self.nodes.remove(&old) {
                self.nodes.insert(format!("{to}{}", &old[from.len()..]), node);
            }
        }
        Ok(())
    }

    fn copy_file(&mut self, from: &str, to: &str) -> io::Result<()> {
        let (data, mode) = match self.nodes.get(from) {
            Some(Node::File { data, mode }) => (data.clone(), *mode),
            Some(Node::Dir { .. }) => return Err(os_error("Is a directory")),
            None => return Err(not_found()),
        };
        match self.nodes.get(to) {
            Some(Node::Dir { .. }) => return Err(os_error("Is a directory")),
            Some(Node::File { .. }) => {}
            None => self.check_parent(to)?,
        }
        self.nodes.insert(to.to_string(), Node::File { data, mode });
        Ok(())
    }

    fn set_mode(&mut self, key: &str, new_mode: u32) -> io::Result<()> {
        match self.nodes.get_mut(key) {
            Some(Node::Dir { mode }) | Some(Node::File { mode, .. }) => {
                *mode = new_mode & 0o7777;
                Ok(())
            }
            None => Err(not_found()),
        }
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
    fn the_in_memory_world_has_a_size_limit() {
        let vfs = VirtualFileSystem::in_memory();
        let mb = "x".repeat(1024 * 1024);
        // one file stops growing at the per-file limit
        let mut err = None;
        for _ in 0..(MEM_MAX_FILE / mb.len() + 2) {
            if let Err(e) = vfs.write_file("big.txt", &mb, true) {
                err = Some(e.to_string());
                break;
            }
        }
        assert!(err.unwrap().contains("No space left on device"));
        // many files stop at the total limit
        let mut written = 0;
        for i in 0..(MEM_MAX_TOTAL / mb.len() + 2) {
            if vfs.write_file(&format!("f{i}.txt"), &mb, false).is_err() {
                break;
            }
            written += 1;
        }
        assert!(written * mb.len() <= MEM_MAX_TOTAL);
        // rewriting a file in place still works at the limit
        assert!(vfs.write_file("f0.txt", "small", false).is_ok());
    }

    use super::*;

    /// The same sandbox on disk and in memory, both with the generic home
    fn both(session: &str) -> [VirtualFileSystem; 2] {
        let memory = VirtualFileSystem::in_memory();
        memory.seed_generic().unwrap();
        [VirtualFileSystem::new("test", session).unwrap(), memory]
    }

    #[test]
    fn test_create_vfs() {
        let vfs = VirtualFileSystem::new("test", "session-123").unwrap();
        assert!(vfs.resolve_path("~").unwrap().exists());
        assert_eq!(vfs.get_current_dir(), Path::new("/lesson-home"));
        let vfs = VirtualFileSystem::in_memory();
        assert!(vfs.is_dir("~"));
        assert_eq!(vfs.get_current_dir(), Path::new("/lesson-home"));
    }

    #[test]
    fn test_change_directory() {
        for mut vfs in both("session-456") {
            // Change to Documents
            let new_path = vfs.change_directory("Documents").unwrap();
            assert_eq!(new_path, "/lesson-home/Documents");

            // Go back up
            let new_path = vfs.change_directory("..").unwrap();
            assert_eq!(new_path, "/lesson-home");
        }
    }

    #[test]
    fn test_list_directory() {
        for vfs in both("session-789") {
            let entries = vfs.list_directory(None).unwrap();

            // Should have Documents, Downloads, Pictures, Music, Videos, projects, .bashrc
            assert!(entries.len() >= 7);

            // Directories should be first
            assert!(entries[0].is_dir || entries[0].name == ".bashrc");
        }
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
        for vfs in both("session-cat-escape") {
            assert_escape_err(vfs.read_file("../../../../etc/hostname"));
            assert_escape_err(vfs.read_file("/lesson-home/../../etc/hostname"));
        }
    }

    #[test]
    fn test_rm_traversal_escape_rejected() {
        for vfs in both("session-rm-escape") {
            // Even with recursive + force, escaping paths must error, not silently pass
            assert_escape_err(vfs.remove("../../..", true, true));
            assert_escape_err(vfs.remove("../outside.txt", false, false));
        }
    }

    #[test]
    fn test_cd_traversal_escape_rejected() {
        for mut vfs in both("session-cd-escape") {
            assert_escape_err(vfs.change_directory("../.."));
            assert_escape_err(vfs.change_directory("Documents/../../.."));

            // Current directory must be unchanged after rejected attempts
            assert_eq!(vfs.get_current_dir(), Path::new("/lesson-home"));
        }
    }

    #[test]
    fn test_mv_traversal_escape_rejected() {
        for vfs in both("session-mv-escape") {
            assert_escape_err(vfs.move_item("Documents/report.txt", "../../stolen.txt"));
            assert_escape_err(vfs.move_item("../../../etc/hostname", "here.txt"));
        }
    }

    #[test]
    fn test_cp_traversal_escape_rejected() {
        for vfs in both("session-cp-escape") {
            assert_escape_err(vfs.copy("Documents/report.txt", "../../stolen.txt", false));
            assert_escape_err(vfs.copy("../../../etc/hostname", "here.txt", false));
        }
    }

    #[test]
    fn test_mkdir_touch_write_traversal_escape_rejected() {
        for vfs in both("session-mk-escape") {
            assert_escape_err(vfs.create_directory("../evil-dir", true));
            assert_escape_err(vfs.touch_file("../evil.txt"));
            assert_escape_err(vfs.write_file("../evil.txt", "payload", false));
            assert_escape_err(vfs.list_directory(Some("../..")));
        }
    }

    #[test]
    fn test_cd_dotdot_at_root_clamps() {
        for mut vfs in both("session-cd-root") {
            // 'cd ..' at the sandbox root stays at the root (does not escape)
            let path = vfs.change_directory("..").unwrap();
            assert_eq!(path, "/lesson-home");
            assert_eq!(vfs.get_current_dir(), Path::new("/lesson-home"));
        }
    }

    #[test]
    fn test_grep_file() {
        for vfs in both("session-grep") {
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
    }

    #[test]
    fn test_head_and_tail_file() {
        for vfs in both("session-headtail") {
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
    }

    #[test]
    fn test_wc_file() {
        for vfs in both("session-wc") {
            vfs.write_file("data.txt", "one two\nthree\n", false).unwrap();

            let (lines, words, bytes) = vfs.wc_file("data.txt").unwrap();
            assert_eq!(lines, 2);
            assert_eq!(words, 3);
            assert_eq!(bytes, 14);
        }
    }

    #[test]
    fn test_legitimate_relative_paths_still_work() {
        for mut vfs in both("session-legit") {
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
            assert!(vfs.is_file("holiday/moved.jpg"));
            assert!(!vfs.exists("holiday/copy.jpg"));
        }

        // Absolute virtual paths resolve inside the sandbox root
        let vfs = VirtualFileSystem::new("test", "session-legit-root").unwrap();
        let resolved = vfs.resolve_path("/lesson-home/Documents").unwrap();
        assert!(resolved.starts_with(vfs.resolve_path("~").unwrap().parent().unwrap()));
        assert!(resolved.exists());
    }

    #[test]
    fn memory_tree_moves_copies_and_removes_like_the_disk() {
        for vfs in both("session-tree") {
            vfs.create_directory("a/b/c", true).unwrap();
            vfs.write_file("a/b/c/f.txt", "x\n", false).unwrap();
            vfs.write_file("a/top.txt", "t\n", false).unwrap();

            // A folder moves with everything in it
            vfs.move_item("a", "z").unwrap();
            assert_eq!(vfs.read_file("z/b/c/f.txt").unwrap(), "x\n");
            assert!(!vfs.exists("a"));

            // Into an existing folder: under the same name
            vfs.create_directory("box", false).unwrap();
            vfs.move_item("z/top.txt", "box").unwrap();
            assert!(vfs.is_file("box/top.txt"));

            // A folder can't go inside itself, and a copy can't either
            assert!(vfs.move_item("z", "z/b/inner").is_err());
            assert!(vfs.copy("z", "z/b/copy", true).is_err());

            // Recursive copy, then remove
            vfs.copy("z", "y", true).unwrap();
            assert_eq!(vfs.read_file("y/b/c/f.txt").unwrap(), "x\n");
            assert!(vfs.remove("y", false, false).is_err());
            vfs.remove("y", true, false).unwrap();
            assert!(!vfs.exists("y/b/c/f.txt"));
            assert!(vfs.exists("z/b/c/f.txt"));

            // Writing needs the folder to exist; mkdir without -p needs the parent
            assert!(vfs.write_file("nowhere/x.txt", "x", false).is_err());
            assert!(vfs.create_directory("p/q", false).is_err());
            assert!(vfs.create_directory("box", false).is_err());
            assert!(vfs.create_directory("box", true).is_ok());

            // Append, and cp copies the mode
            vfs.write_file("box/top.txt", "u\n", true).unwrap();
            assert_eq!(vfs.read_file("box/top.txt").unwrap(), "t\nu\n");
            vfs.set_mode("box/top.txt", 0o700).unwrap();
            vfs.copy("box/top.txt", "copied.txt", false).unwrap();
            if let Some(mode) = vfs.metadata("copied.txt").unwrap().mode {
                assert_eq!(mode & 0o777, 0o700);
            }

            // Non-text files read as bytes, and cat says it can't
            vfs.write_bytes("bin.dat", &[0xff, 0xfe, 0x00]).unwrap();
            assert_eq!(vfs.read_bytes("bin.dat").unwrap(), vec![0xff, 0xfe, 0x00]);
            assert!(vfs.read_file("bin.dat").unwrap_err().to_string().contains("Failed to read file"));
        }
    }

    #[test]
    fn explorer_lessons_see_only_their_own_files() {
        let library = crate::lesson::LessonLibrary::new();
        let explorer = library.get("explorer-look-around").unwrap();
        let operator = library.get("nav-basics").unwrap();
        let names = |vfs: &VirtualFileSystem| -> Vec<String> {
            vfs.list_directory(None).unwrap().into_iter().map(|e| e.name).collect()
        };
        let disk = VirtualFileSystem::for_lesson(explorer, "session-world").unwrap();
        let memory = VirtualFileSystem::for_lesson_in_memory(explorer).unwrap();
        let mut tops: Vec<String> = explorer
            .setup
            .iter()
            .map(|f| f.path.split('/').next().unwrap().to_string())
            .collect();
        tops.dedup();
        assert_eq!(names(&disk), tops);
        assert_eq!(names(&memory), tops);

        let disk = VirtualFileSystem::for_lesson(operator, "session-world-op").unwrap();
        let memory = VirtualFileSystem::for_lesson_in_memory(operator).unwrap();
        for vfs in [&disk, &memory] {
            assert!(vfs.is_dir("Documents") && vfs.is_file(".bashrc"));
            assert!(operator.setup.iter().all(|f| vfs.is_file(&f.path)));
        }
        assert_eq!(names(&disk), names(&memory));
    }

    #[test]
    fn tree_shows_folders_first_and_the_current_one() {
        for mut vfs in both("session-tree-view") {
            vfs.change_directory("Documents").unwrap();
            let tree = vfs.get_tree(2);
            assert!(tree.first().unwrap().is_dir);
            let docs = tree.iter().find(|n| n.name == "Documents").unwrap();
            assert!(docs.is_current);
            assert!(docs.children.iter().any(|n| n.name == "report.txt"));
        }
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

    fn kind_at(&self, lesson_path: &str) -> Option<crate::lesson::EntryKind> {
        let path = lesson_path.trim_start_matches('/');
        let virtual_abs = self.virtual_path(&format!("/lesson-home/{path}")).ok()?;
        Some(match self.prim_stat(&virtual_abs) {
            None => crate::lesson::EntryKind::Missing,
            Some(meta) if meta.is_dir => crate::lesson::EntryKind::Dir,
            Some(_) => crate::lesson::EntryKind::File,
        })
    }

    fn read_text(&self, lesson_path: &str) -> Option<String> {
        let path = lesson_path.trim_start_matches('/');
        self.read_file(&format!("/lesson-home/{path}")).ok()
    }
}
