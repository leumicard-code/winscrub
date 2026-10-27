//! Target catalogue. Every module reports only what actually exists on disk.

pub mod apps;
pub mod browsers;
pub mod system;

use crate::model::Target;
use std::path::PathBuf;

pub fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

/// %LOCALAPPDATA%
pub fn local() -> PathBuf {
    env_path("LOCALAPPDATA")
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("C:\\Users\\Default\\AppData\\Local"))
}

/// %APPDATA%
pub fn roaming() -> PathBuf {
    env_path("APPDATA")
        .or_else(dirs::data_dir)
        .unwrap_or_else(|| PathBuf::from("C:\\Users\\Default\\AppData\\Roaming"))
}

/// %USERPROFILE%\AppData\LocalLow
pub fn locallow() -> PathBuf {
    home().join("AppData\\LocalLow")
}

pub fn home() -> PathBuf {
    env_path("USERPROFILE")
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("C:\\Users\\Default"))
}

/// %TEMP%
pub fn temp() -> PathBuf {
    env_path("TEMP")
        .or_else(|| env_path("TMP"))
        .unwrap_or_else(|| local().join("Temp"))
}

/// %WINDIR%
pub fn windir() -> PathBuf {
    env_path("WINDIR").unwrap_or_else(|| PathBuf::from("C:\\Windows"))
}

/// %ProgramData%
pub fn programdata() -> PathBuf {
    env_path("ProgramData").unwrap_or_else(|| PathBuf::from("C:\\ProgramData"))
}

/// The drive Windows is installed on, with a trailing separator.
pub fn system_drive() -> PathBuf {
    let drive = env_path("SystemDrive")
        .map(|d| d.to_string_lossy().trim_end_matches('\\').to_string())
        .unwrap_or_else(|| "C:".to_string());
    PathBuf::from(format!("{drive}\\"))
}

/// Lowercase, dash separated id fragment for an arbitrary label.
pub fn slug(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_dash = true;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "x".to_string()
    } else {
        out
    }
}

/// Immediate subdirectories of `dir`, sorted by name.
pub fn subdirs(dir: &std::path::Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return out,
    };
    for entry in rd.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            out.push((entry.file_name().to_string_lossy().to_string(), path));
        }
    }
    out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    out
}

/// Keep only the directories that exist.
pub fn existing_dirs(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.into_iter().filter(|p| p.is_dir()).collect()
}

/// Keep only the paths that exist.
pub fn existing(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.into_iter().filter(|p| p.exists()).collect()
}

/// Everything winscrub knows how to clean on this machine.
pub fn collect() -> Vec<Target> {
    let mut all = browsers::targets();
    all.extend(system::targets());
    all.extend(apps::targets());

    // Ids are the public handle for --only / --skip; keep them unique.
    let mut seen = std::collections::HashSet::new();
    all.retain(|t| seen.insert(t.id.clone()));
    all
}
