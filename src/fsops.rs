//! Measuring and removing files.
//!
//! Nothing here follows symlinks or junctions, nothing deletes a path the
//! `Guard` has not cleared, and a file that is locked by a running process is
//! counted as skipped rather than treated as a failure.

use crate::safety::{self, Guard};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const MAX_ERRORS: usize = 12;

#[derive(Default, Clone, Debug)]
pub struct Measure {
    pub bytes: u64,
    pub files: u64,
}

impl Measure {
    pub fn merge(&mut self, other: Measure) {
        self.bytes += other.bytes;
        self.files += other.files;
    }
}

#[derive(Default, Clone, Debug)]
pub struct Removal {
    pub freed: u64,
    pub removed: u64,
    pub skipped: u64,
    pub errors: Vec<String>,
}

impl Removal {
    pub fn merge(&mut self, other: Removal) {
        self.freed += other.freed;
        self.removed += other.removed;
        self.skipped += other.skipped;
        for e in other.errors {
            self.note(e);
        }
    }

    fn note(&mut self, msg: String) {
        if self.errors.len() < MAX_ERRORS {
            self.errors.push(msg);
        }
    }
}

/// Windows still trips over paths beyond MAX_PATH in some APIs; Chromium cache
/// trees get close. Opt into the extended form only when it is needed.
fn extended(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    let looks_absolute = s.len() > 2 && s.as_bytes()[1] == b':';
    if s.len() >= 240 && looks_absolute && !s.starts_with("\\\\?\\") {
        PathBuf::from(format!("\\\\?\\{s}"))
    } else {
        path.to_path_buf()
    }
}

fn clear_readonly(path: &Path) -> bool {
    match fs::metadata(path) {
        Ok(md) => {
            let mut perms = md.permissions();
            if !perms.readonly() {
                return false;
            }
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            fs::set_permissions(path, perms).is_ok()
        }
        Err(_) => false,
    }
}

pub fn measure(path: &Path) -> Measure {
    let mut m = Measure::default();
    let md = match fs::symlink_metadata(path) {
        Ok(md) => md,
        Err(_) => return m,
    };
    if md.file_type().is_symlink() {
        return m;
    }
    if md.is_file() {
        m.bytes = md.len();
        m.files = 1;
        return m;
    }
    for entry in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            if let Ok(md) = entry.metadata() {
                m.bytes += md.len();
                m.files += 1;
            }
        }
    }
    m
}

pub fn measure_all(paths: &[PathBuf]) -> Measure {
    let mut m = Measure::default();
    for p in paths {
        m.merge(measure(p));
    }
    m
}

pub fn measure_glob(dir: &Path, pattern: &str) -> Measure {
    let mut m = Measure::default();
    let rd = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return m,
    };
    for entry in rd.filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().to_string();
        if wildcard_match(&name, pattern) {
            m.merge(measure(&entry.path()));
        }
    }
    m
}

fn rm_file(path: &Path, dry: bool, out: &mut Removal) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if safety::is_never_delete(&name) {
        out.skipped += 1;
        return;
    }
    let size = fs::symlink_metadata(path).map(|m| m.len()).unwrap_or(0);
    if dry {
        out.freed += size;
        out.removed += 1;
        return;
    }
    let target = extended(path);
    match fs::remove_file(&target) {
        Ok(()) => {
            out.freed += size;
            out.removed += 1;
        }
        Err(first) => {
            if clear_readonly(&target) && fs::remove_file(&target).is_ok() {
                out.freed += size;
                out.removed += 1;
            } else {
                out.skipped += 1;
                out.note(format!("{}: {first}", path.display()));
            }
        }
    }
}

/// Remove a directory tree depth first. Symlinks and junctions are left alone.
fn rm_tree(root: &Path, dry: bool, keep_root: bool, out: &mut Removal) {
    for entry in WalkDir::new(root)
        .contents_first(true)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        let is_root = path == root;
        if is_root && keep_root {
            continue;
        }
        let ft = entry.file_type();
        if ft.is_symlink() {
            out.skipped += 1;
            continue;
        }
        if ft.is_dir() {
            if !dry {
                let _ = fs::remove_dir(extended(path));
            }
        } else {
            rm_file(path, dry, out);
        }
    }
}

pub fn delete_dir_contents(dir: &Path, guard: &Guard, dry: bool) -> Removal {
    let mut out = Removal::default();
    if let Err(e) = guard.check_contents(dir) {
        out.note(format!("blocked: {e}"));
        return out;
    }
    if !dir.is_dir() {
        return out;
    }
    rm_tree(dir, dry, true, &mut out);
    out
}

pub fn delete_paths(paths: &[PathBuf], guard: &Guard, dry: bool) -> Removal {
    let mut out = Removal::default();
    for path in paths {
        let md = match fs::symlink_metadata(path) {
            Ok(md) => md,
            Err(_) => continue,
        };
        if let Err(e) = guard.check_delete(path) {
            out.note(format!("blocked: {e}"));
            continue;
        }
        if md.file_type().is_symlink() {
            out.skipped += 1;
            continue;
        }
        if md.is_dir() {
            rm_tree(path, dry, false, &mut out);
        } else {
            rm_file(path, dry, &mut out);
        }
    }
    out
}

pub fn delete_glob(dir: &Path, pattern: &str, guard: &Guard, dry: bool) -> Removal {
    let mut matched: Vec<PathBuf> = Vec::new();
    let mut out = Removal::default();
    let rd = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return out,
    };
    for entry in rd.filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().to_string();
        if wildcard_match(&name, pattern) {
            matched.push(entry.path());
        }
    }
    out.merge(delete_paths(&matched, guard, dry));
    out
}

/// Case insensitive glob over a single name. Supports `*` and `?`.
pub fn wildcard_match(name: &str, pattern: &str) -> bool {
    let n: Vec<char> = name.to_ascii_lowercase().chars().collect();
    let p: Vec<char> = pattern.to_ascii_lowercase().chars().collect();
    let (mut i, mut j) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut mark = 0usize;

    while i < n.len() {
        if j < p.len() && (p[j] == '?' || p[j] == n[i]) {
            i += 1;
            j += 1;
        } else if j < p.len() && p[j] == '*' {
            star = Some(j);
            mark = i;
            j += 1;
        } else if let Some(s) = star {
            j = s + 1;
            mark += 1;
            i = mark;
        } else {
            return false;
        }
    }
    while j < p.len() && p[j] == '*' {
        j += 1;
    }
    j == p.len()
}

pub fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (idx, ch) in digits.chars().enumerate() {
        if idx > 0 && (digits.len() - idx) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards() {
        assert!(wildcard_match("thumbcache_256.db", "thumbcache_*.db"));
        assert!(wildcard_match("THUMBCACHE_96.DB", "thumbcache_*.db"));
        assert!(!wildcard_match("thumbcache_256.dbx", "thumbcache_*.db"));
        assert!(wildcard_match("webcache_4430", "webcache*"));
        assert!(wildcard_match("anything", "*"));
    }

    #[test]
    fn sizes() {
        assert_eq!(human(512), "512 B");
        assert_eq!(human(1024), "1.0 KB");
        assert_eq!(human(1024 * 1024 * 3), "3.0 MB");
        assert_eq!(thousands(1234567), "1,234,567");
    }
}
