//! More sandbox commands that walk the practice files: glob expansion,
//! `find`, `du`, `tar`, `gzip`, and practice SSH keys.
//!
//! Every path goes through [`VirtualFileSystem::resolve_path`], so nothing
//! here can reach outside the lesson sandbox. Symbolic links are never
//! followed or created.

use crate::shell_syntax::{fnmatch, glob_match};
use crate::virtual_fs::VirtualFileSystem;
use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

/// What `find` looks for
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FindQuery {
    /// `-name PATTERN` (or `-iname` when `ignore_case`)
    pub name: Option<String>,
    pub ignore_case: bool,
    /// `-type f` or `-type d`
    pub kind: Option<char>,
    /// `-maxdepth N`
    pub max_depth: Option<usize>,
}

impl FindQuery {
    /// Parse find's arguments into (start paths, query)
    pub fn parse(args: &[String]) -> Result<(Vec<String>, FindQuery), String> {
        let mut paths = Vec::new();
        let mut query = FindQuery::default();
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            let mut value = |what: &str| {
                iter.next()
                    .cloned()
                    .ok_or_else(|| format!("find: {arg} needs {what}"))
            };
            match arg.as_str() {
                "-name" => query.name = Some(value("a pattern, like -name '*.txt'")?),
                "-iname" => {
                    query.name = Some(value("a pattern, like -iname '*.txt'")?);
                    query.ignore_case = true;
                }
                "-type" => {
                    let kind = value("f (files) or d (folders)")?;
                    match kind.as_str() {
                        "f" => query.kind = Some('f'),
                        "d" => query.kind = Some('d'),
                        _ => return Err("find: -type takes f (files) or d (folders)".to_string()),
                    }
                }
                "-maxdepth" => {
                    let n = value("a number")?;
                    query.max_depth =
                        Some(n.parse().map_err(|_| "find: -maxdepth needs a number".to_string())?);
                }
                "-print" => {}
                other if other.starts_with('-') && other.len() > 1 => {
                    return Err(format!(
                        "find: {other} isn't available in the sandbox (try -name, -iname, -type, -maxdepth)"
                    ))
                }
                path if query == FindQuery::default() => paths.push(path.to_string()),
                other => return Err(format!("find: paths go before the tests, not after ({other})")),
            }
        }
        if paths.is_empty() {
            paths.push(".".to_string());
        }
        Ok((paths, query))
    }

    fn matches(&self, name: &str, is_dir: bool) -> bool {
        if let Some(kind) = self.kind {
            if (kind == 'd') != is_dir {
                return false;
            }
        }
        // find's -name, unlike the shell, lets * match a leading dot
        match &self.name {
            Some(pattern) if self.ignore_case => fnmatch(&pattern.to_lowercase(), &name.to_lowercase()),
            Some(pattern) => fnmatch(pattern, name),
            None => true,
        }
    }
}

/// Sorted names in a real directory (no symlinks followed)
fn sorted_children(dir: &Path) -> Vec<(String, PathBuf, bool)> {
    let mut out: Vec<(String, PathBuf, bool)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let meta = fs::symlink_metadata(e.path()).ok()?;
            if meta.file_type().is_symlink() {
                return None;
            }
            Some((e.file_name().to_string_lossy().to_string(), e.path(), meta.is_dir()))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Join a path as typed with a child name: "." + "a" -> "./a", "docs/" + "a" -> "docs/a"
fn join_display(base: &str, name: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), name)
}

/// Reject archive entry paths that are absolute or climb with `..`
fn safe_relative(path: &Path) -> Result<PathBuf> {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => bail!("'{}' would land outside the folder", path.display()),
        }
    }
    if out.as_os_str().is_empty() {
        bail!("empty path in archive");
    }
    Ok(out)
}

impl VirtualFileSystem {
    /// Expand a glob against the sandbox (`*.txt`, `logs/day?.log`).
    /// Only the last part of the path may hold a pattern. Returns the
    /// sorted matches, or nothing when none match.
    pub fn glob(&self, pattern: &str) -> Vec<String> {
        let (dir, file_pattern) = match pattern.rsplit_once('/') {
            Some((dir, file)) => (Some(if dir.is_empty() { "/" } else { dir }), file),
            None => (None, pattern),
        };
        if dir.is_some_and(|d| d.contains(['*', '?', '['])) {
            return Vec::new();
        }
        let Ok(entries) = self.list_directory(dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .into_iter()
            .map(|e| e.name)
            .filter(|name| glob_match(file_pattern, name))
            .map(|name| match pattern.rsplit_once('/') {
                Some((d, _)) => format!("{d}/{name}"),
                None => name,
            })
            .collect();
        names.sort();
        names
    }

    /// `find`: walk each start path and list what matches, as paths
    /// written from the start path the way it was typed
    pub fn find(&self, start: &str, query: &FindQuery) -> Result<Vec<String>> {
        let real = self.resolve_path(start)?;
        let meta = fs::symlink_metadata(&real)
            .with_context(|| format!("'{start}': No such file or directory"))?;
        let mut out = Vec::new();
        let start_name = Path::new(start)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| start.to_string());
        if query.matches(&start_name, meta.is_dir()) {
            out.push(start.to_string());
        }
        if meta.is_dir() {
            self.find_in(&real, start, 1, query, &mut out);
        }
        Ok(out)
    }

    fn find_in(&self, dir: &Path, display: &str, depth: usize, query: &FindQuery, out: &mut Vec<String>) {
        if query.max_depth.is_some_and(|max| depth > max) {
            return;
        }
        for (name, path, is_dir) in sorted_children(dir) {
            let shown = join_display(display, &name);
            if query.matches(&name, is_dir) {
                out.push(shown.clone());
            }
            if is_dir {
                self.find_in(&path, &shown, depth + 1, query, out);
            }
        }
    }

    /// `du`: size in bytes of each folder under `path` (deepest first),
    /// or only `path` itself when `summarize`
    pub fn disk_usage(&self, path: &str, summarize: bool) -> Result<Vec<(String, u64)>> {
        let real = self.resolve_path(path)?;
        let meta = fs::symlink_metadata(&real)
            .with_context(|| format!("cannot access '{path}': No such file or directory"))?;
        if !meta.is_dir() {
            return Ok(vec![(path.to_string(), meta.len())]);
        }
        let mut out = Vec::new();
        let total = Self::du_walk(&real, path, summarize, &mut out);
        out.push((path.to_string(), total));
        Ok(out)
    }

    fn du_walk(dir: &Path, display: &str, summarize: bool, out: &mut Vec<(String, u64)>) -> u64 {
        let mut total = 0;
        for (name, path, is_dir) in sorted_children(dir) {
            if is_dir {
                let shown = join_display(display, &name);
                let size = Self::du_walk(&path, &shown, summarize, out);
                if !summarize {
                    out.push((shown, size));
                }
                total += size;
            } else {
                total += fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            }
        }
        total
    }

    /// `tar -c`: pack files and folders into an archive. Returns the
    /// entries written, for `-v`.
    pub fn tar_create(&self, archive: &str, inputs: &[String], gzip: bool) -> Result<Vec<String>> {
        if inputs.is_empty() {
            bail!("tar: say what to pack, like: tar -czf notes.tar.gz notes");
        }
        let mut names = Vec::new();
        let mut buffer = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut buffer);
            builder.follow_symlinks(false);
            for input in inputs {
                let rel = safe_relative(Path::new(input))
                    .with_context(|| format!("tar: pack files from here down ({input})"))?;
                let real = self.resolve_path(input)?;
                let meta = fs::symlink_metadata(&real)
                    .with_context(|| format!("tar: {input}: No such file or directory"))?;
                if meta.is_dir() {
                    Self::tar_dir(&mut builder, &real, &rel, &mut names)?;
                } else if meta.is_file() {
                    builder.append_path_with_name(&real, &rel)?;
                    names.push(rel.to_string_lossy().to_string());
                }
            }
            builder.finish()?;
        }
        let bytes = if gzip {
            let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            std::io::Write::write_all(&mut encoder, &buffer)?;
            encoder.finish()?
        } else {
            buffer
        };
        fs::write(self.resolve_path(archive)?, bytes)
            .with_context(|| format!("tar: can't write {archive}"))?;
        Ok(names)
    }

    fn tar_dir<W: std::io::Write>(
        builder: &mut tar::Builder<W>,
        real: &Path,
        rel: &Path,
        names: &mut Vec<String>,
    ) -> Result<()> {
        builder.append_dir(rel, real)?;
        names.push(format!("{}/", rel.to_string_lossy()));
        for (name, path, is_dir) in sorted_children(real) {
            let child = rel.join(&name);
            if is_dir {
                Self::tar_dir(builder, &path, &child, names)?;
            } else {
                builder.append_path_with_name(&path, &child)?;
                names.push(child.to_string_lossy().to_string());
            }
        }
        Ok(())
    }

    fn tar_bytes(&self, archive: &str) -> Result<Vec<u8>> {
        let raw = fs::read(self.resolve_path(archive)?)
            .with_context(|| format!("tar: {archive}: No such file"))?;
        // Unpack gzip whenever the file is gzip, as GNU tar does
        if raw.starts_with(&[0x1f, 0x8b]) {
            let mut out = Vec::new();
            flate2::read::GzDecoder::new(&raw[..])
                .read_to_end(&mut out)
                .with_context(|| format!("tar: {archive} is damaged"))?;
            Ok(out)
        } else {
            Ok(raw)
        }
    }

    /// `tar -t`: list an archive
    pub fn tar_list(&self, archive: &str) -> Result<Vec<String>> {
        let bytes = self.tar_bytes(archive)?;
        let mut names = Vec::new();
        for entry in tar::Archive::new(&bytes[..])
            .entries()
            .with_context(|| format!("tar: {archive} isn't an archive"))?
        {
            let entry = entry?;
            let mut name = entry.path()?.to_string_lossy().to_string();
            if entry.header().entry_type().is_dir() && !name.ends_with('/') {
                name.push('/');
            }
            names.push(name);
        }
        Ok(names)
    }

    /// `tar -x`: unpack into `dest` (files and folders only; nothing may
    /// land outside `dest`). Returns the entries written.
    pub fn tar_extract(&self, archive: &str, dest: &str) -> Result<Vec<String>> {
        let bytes = self.tar_bytes(archive)?;
        let mut names = Vec::new();
        for entry in tar::Archive::new(&bytes[..])
            .entries()
            .with_context(|| format!("tar: {archive} isn't an archive"))?
        {
            let mut entry = entry?;
            let rel = safe_relative(&entry.path()?)?;
            let target = self.resolve_path(&join_display(dest, &rel.to_string_lossy()))?;
            let kind = entry.header().entry_type();
            if kind.is_dir() {
                fs::create_dir_all(&target)?;
                names.push(format!("{}/", rel.to_string_lossy()));
            } else if kind.is_file() {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                fs::write(&target, data)?;
                #[cfg(unix)]
                if let Ok(mode) = entry.header().mode() {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(&target, fs::Permissions::from_mode(mode & 0o755));
                }
                names.push(rel.to_string_lossy().to_string());
            }
            // Links and devices are skipped
        }
        Ok(names)
    }

    /// `gzip FILE`: compress to FILE.gz and remove FILE
    pub fn gzip(&self, path: &str) -> Result<String> {
        let real = self.resolve_path(path)?;
        if path.ends_with(".gz") {
            bail!("gzip: {path} already has the .gz suffix");
        }
        let data = fs::read(&real).with_context(|| format!("gzip: {path}: No such file"))?;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, &data)?;
        let out = format!("{path}.gz");
        fs::write(self.resolve_path(&out)?, encoder.finish()?)?;
        fs::remove_file(&real)?;
        Ok(out)
    }

    /// `gunzip FILE.gz`: decompress to FILE and remove FILE.gz
    pub fn gunzip(&self, path: &str) -> Result<String> {
        let Some(out) = path.strip_suffix(".gz") else {
            bail!("gunzip: {path}: the name doesn't end in .gz");
        };
        let real = self.resolve_path(path)?;
        let raw = fs::read(&real).with_context(|| format!("gunzip: {path}: No such file"))?;
        let mut data = Vec::new();
        flate2::read::GzDecoder::new(&raw[..])
            .read_to_end(&mut data)
            .with_context(|| format!("gunzip: {path} isn't gzip data"))?;
        fs::write(self.resolve_path(out)?, data)?;
        fs::remove_file(&real)?;
        Ok(out.to_string())
    }

    /// `ssh-keygen -f PATH -C COMMENT`: write a PRACTICE key pair. The
    /// files have the real shape (a private key readable only by you, a
    /// one-line public key) but hold no key material.
    pub fn practice_ssh_key(&self, path: &str, kind: &str, comment: &str) -> Result<(String, String)> {
        let public = format!("{path}.pub");
        for p in [path, public.as_str()] {
            if self.resolve_path(p)?.exists() {
                bail!("{p} already exists. Remove it first, or pick another name with -f");
            }
        }
        if let Some(parent) = self.resolve_path(path)?.parent() {
            fs::create_dir_all(parent)?;
        }
        let private_text = format!(
            "-----BEGIN OPENSSH PRIVATE KEY-----\n\
             (practice key made in the arct sandbox: it opens nothing)\n\
             -----END OPENSSH PRIVATE KEY-----\n"
        );
        let algo = if kind == "rsa" { "ssh-rsa" } else { "ssh-ed25519" };
        let public_text = format!("{algo} AAAAPRACTICEKEYxARCTxSANDBOXxNOTxREAL {comment}\n");
        let private_real = self.resolve_path(path)?;
        fs::write(&private_real, private_text)?;
        fs::write(self.resolve_path(&public)?, public_text)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&private_real, fs::Permissions::from_mode(0o600))?;
        }
        Ok((path.to_string(), public))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vfs(name: &str) -> VirtualFileSystem {
        let vfs = VirtualFileSystem::new("tools", name).unwrap();
        vfs.create_directory("proj/src", true).unwrap();
        vfs.write_file("proj/README.md", "hello\n", false).unwrap();
        vfs.write_file("proj/src/main.txt", "abcdef\n", false).unwrap();
        vfs.write_file("proj/src/.hidden.txt", "x\n", false).unwrap();
        vfs
    }

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn glob_expands_in_folders() {
        let v = vfs("glob");
        assert_eq!(v.glob("proj/src/*.txt"), ["proj/src/main.txt"]);
        assert_eq!(v.glob("proj/src/.*.txt"), ["proj/src/.hidden.txt"]);
        assert!(v.glob("proj/*.nothing").is_empty());
        assert!(v.glob("p*/src/*.txt").is_empty(), "patterns only in the last part");
    }

    #[test]
    fn find_by_name_type_and_depth() {
        let v = vfs("find");
        let (paths, q) = FindQuery::parse(&strings(&["proj", "-name", "*.txt"])).unwrap();
        assert_eq!(paths, ["proj"]);
        assert_eq!(v.find("proj", &q).unwrap(), ["proj/src/.hidden.txt", "proj/src/main.txt"]);

        let (_, q) = FindQuery::parse(&strings(&["-type", "d"])).unwrap();
        v.create_directory("solo", false).unwrap();
        let found = v.find("proj", &q).unwrap();
        assert_eq!(found, ["proj", "proj/src"]);

        let (_, q) = FindQuery::parse(&strings(&["-maxdepth", "1"])).unwrap();
        assert_eq!(v.find("proj", &q).unwrap(), ["proj", "proj/README.md", "proj/src"]);

        let (_, q) = FindQuery::parse(&strings(&["-iname", "readme*"])).unwrap();
        assert_eq!(v.find("proj", &q).unwrap(), ["proj/README.md"]);

        assert!(FindQuery::parse(&strings(&["-exec", "rm"])).is_err());
        assert!(v.find("nowhere", &FindQuery::default()).is_err());
    }

    #[test]
    fn find_from_dot_lists_everything_under_it() {
        let mut v = vfs("finddot");
        v.change_directory("proj").unwrap();
        let found = v.find(".", &FindQuery::default()).unwrap();
        assert_eq!(found[0], ".");
        assert!(found.contains(&"./src/main.txt".to_string()));
    }

    #[test]
    fn du_counts_bytes() {
        let v = vfs("du");
        let all = v.disk_usage("proj", false).unwrap();
        assert_eq!(all.last().unwrap(), &("proj".to_string(), 6 + 7 + 2));
        assert_eq!(all[0], ("proj/src".to_string(), 9));
        assert_eq!(v.disk_usage("proj", true).unwrap().len(), 1);
    }

    #[test]
    fn tar_round_trip() {
        let v = vfs("tar");
        let packed = v.tar_create("proj.tar.gz", &strings(&["proj"]), true).unwrap();
        assert!(packed.contains(&"proj/src/main.txt".to_string()));
        assert!(v.tar_list("proj.tar.gz").unwrap().contains(&"proj/README.md".to_string()));

        v.create_directory("restore", false).unwrap();
        v.tar_extract("proj.tar.gz", "restore").unwrap();
        assert_eq!(v.read_file("restore/proj/src/main.txt").unwrap(), "abcdef\n");

        // Plain (not gzipped) archives too
        v.tar_create("plain.tar", &strings(&["proj/README.md"]), false).unwrap();
        assert_eq!(v.tar_list("plain.tar").unwrap(), ["proj/README.md"]);
    }

    #[test]
    fn tar_refuses_paths_that_climb_out() {
        let v = vfs("tarclimb");
        assert!(v.tar_create("x.tar", &strings(&["../proj"]), false).is_err());
        assert!(v.tar_create("x.tar", &strings(&["/lesson-home/proj"]), false).is_err());

        // A hand-made archive with ../ in an entry is refused on extract
        let mut raw = Vec::new();
        {
            let mut b = tar::Builder::new(&mut raw);
            let mut header = tar::Header::new_gnu();
            header.set_size(3);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Regular);
            {
                let name = b"../escape.txt";
                let gnu = header.as_gnu_mut().unwrap();
                gnu.name[..name.len()].copy_from_slice(name);
            }
            header.set_cksum();
            b.append(&header, &b"bad"[..]).unwrap();
            b.finish().unwrap();
        }
        std::fs::write(v.resolve_path("evil.tar").unwrap(), raw).unwrap();
        assert!(v.tar_extract("evil.tar", "proj").is_err());
        assert!(!v.resolve_path("escape.txt").unwrap().exists());
    }

    #[test]
    fn gzip_round_trip() {
        let v = vfs("gzip");
        assert_eq!(v.gzip("proj/README.md").unwrap(), "proj/README.md.gz");
        assert!(!v.resolve_path("proj/README.md").unwrap().exists());
        assert_eq!(v.gunzip("proj/README.md.gz").unwrap(), "proj/README.md");
        assert_eq!(v.read_file("proj/README.md").unwrap(), "hello\n");
    }

    #[test]
    fn practice_keys_have_the_real_shape() {
        let v = vfs("ssh");
        let (private, public) = v.practice_ssh_key(".ssh/id_ed25519", "ed25519", "me@laptop").unwrap();
        assert_eq!(public, ".ssh/id_ed25519.pub");
        assert!(v.read_file(&private).unwrap().contains("PRIVATE KEY"));
        assert!(v.read_file(&public).unwrap().starts_with("ssh-ed25519 "));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(v.resolve_path(&private).unwrap()).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert!(v.practice_ssh_key(".ssh/id_ed25519", "ed25519", "x").is_err(), "never overwrites");
    }
}
