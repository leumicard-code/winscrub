//! The guard rails.
//!
//! Every deletion goes through `Guard`. A path is removable only when it sits
//! inside one of a small set of cleanup roots, is not itself a protected
//! folder, and does not contain one. On top of that, a name blocklist protects
//! files holding credentials, bookmarks or boot data even if some target
//! definition were to point at them by mistake.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

/// Files that are never deleted, wherever they are found.
pub const NEVER_DELETE: &[&str] = &[
    // Chromium credentials / payment data / encryption key / bookmarks
    "login data",
    "login data-journal",
    "login data for account",
    "login data for account-journal",
    "web data",
    "web data-journal",
    "local state",
    "bookmarks",
    "bookmarks.bak",
    "affiliation database",
    // Firefox profile data
    "places.sqlite",
    "places.sqlite-wal",
    "places.sqlite-shm",
    "key3.db",
    "key4.db",
    "logins.json",
    "logins-backup.json",
    "cert9.db",
    "prefs.js",
    "profiles.ini",
    // Windows
    "ntuser.dat",
    "ntuser.ini",
    "pagefile.sys",
    "swapfile.sys",
    "hiberfil.sys",
    "bootmgr",
    "ntldr",
    "boot.ini",
    "bcd",
];

pub fn is_never_delete(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    NEVER_DELETE.contains(&lower.as_str())
}

/// Case folded, separator normalised form used for every comparison.
pub fn key(path: &Path) -> String {
    let mut s = path.to_string_lossy().replace('/', "\\").to_ascii_lowercase();
    if let Some(rest) = s.strip_prefix("\\\\?\\") {
        s = rest.to_string();
    }
    while s.len() > 3 && s.ends_with('\\') {
        s.pop();
    }
    s
}

fn inside(child: &str, parent: &str) -> bool {
    child == parent || child.starts_with(&format!("{parent}\\"))
}

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

pub struct Guard {
    roots: Vec<String>,
    protected: Vec<String>,
    exact: Vec<String>,
}

impl Guard {
    pub fn new() -> Self {
        let home = env_dir("USERPROFILE").or_else(dirs::home_dir);
        let local = env_dir("LOCALAPPDATA").or_else(dirs::data_local_dir);
        let roaming = env_dir("APPDATA").or_else(dirs::data_dir);
        let temp = env_dir("TEMP").or_else(|| env_dir("TMP"));
        let windir = env_dir("WINDIR").unwrap_or_else(|| PathBuf::from("C:\\Windows"));
        let programdata = env_dir("ProgramData").unwrap_or_else(|| PathBuf::from("C:\\ProgramData"));
        let drive = env_dir("SystemDrive")
            .map(|d| PathBuf::from(format!("{}\\", d.to_string_lossy().trim_end_matches('\\'))))
            .unwrap_or_else(|| PathBuf::from("C:\\"));

        let mut roots: Vec<String> = Vec::new();
        if let Some(p) = local.as_ref() {
            roots.push(key(p));
        }
        if let Some(p) = roaming.as_ref() {
            roots.push(key(p));
        }
        if let Some(p) = home.as_ref() {
            roots.push(key(&p.join("AppData")));
        }
        if let Some(p) = temp.as_ref() {
            roots.push(key(p));
        }
        for sub in [
            "Temp",
            "SystemTemp",
            "Prefetch",
            "SoftwareDistribution",
            "Logs",
            "Minidump",
            "LiveKernelReports",
            "ServiceProfiles",
            "Panther",
        ] {
            roots.push(key(&windir.join(sub)));
        }
        roots.push(key(&programdata.join("Microsoft\\Windows\\WER")));

        // Well known upgrade leftovers that sit too close to the drive root for
        // the generic depth rule, but are safe to remove on purpose.
        let exact = vec![
            key(&drive.join("Windows.old")),
            key(&drive.join("$Windows.~BT")),
            key(&drive.join("$Windows.~WS")),
            key(&drive.join("$WinREAgent")),
            key(&windir.join("MEMORY.DMP")),
        ];

        let mut protected: Vec<String> = vec![
            key(&drive),
            key(&windir),
            key(&windir.join("System32")),
            key(&windir.join("SysWOW64")),
            key(&windir.join("WinSxS")),
            key(&windir.join("ServiceProfiles")),
            key(&programdata),
            key(&PathBuf::from("C:\\Users")),
            key(&PathBuf::from("C:\\Program Files")),
            key(&PathBuf::from("C:\\Program Files (x86)")),
        ];
        if let Some(p) = local.as_ref() {
            protected.push(key(p));
            protected.push(key(&p.join("Packages")));
            protected.push(key(&p.join("Microsoft")));
            protected.push(key(&p.join("Microsoft\\Windows")));
        }
        if let Some(p) = roaming.as_ref() {
            protected.push(key(p));
            protected.push(key(&p.join("Microsoft")));
        }
        if let Some(home) = home.as_ref() {
            protected.push(key(home));
            protected.push(key(&home.join("AppData")));
            protected.push(key(&home.join("AppData\\Local")));
            protected.push(key(&home.join("AppData\\LocalLow")));
            protected.push(key(&home.join("AppData\\Roaming")));
            for folder in [
                "Desktop",
                "Documents",
                "Downloads",
                "Pictures",
                "Music",
                "Videos",
                "Favorites",
                "Links",
                "Contacts",
                "Searches",
                "Saved Games",
                "3D Objects",
                "OneDrive",
                "source",
                ".ssh",
                ".gnupg",
            ] {
                protected.push(key(&home.join(folder)));
            }
        }

        roots.sort();
        roots.dedup();
        protected.sort();
        protected.dedup();

        Guard {
            roots,
            protected,
            exact,
        }
    }

    pub fn roots(&self) -> &[String] {
        &self.roots
    }

    pub fn protected(&self) -> &[String] {
        &self.protected
    }

    /// May we delete everything *inside* this directory?
    pub fn check_contents(&self, path: &Path) -> Result<()> {
        self.check(path, false)
    }

    /// May we delete this path itself?
    pub fn check_delete(&self, path: &Path) -> Result<()> {
        self.check(path, true)
    }

    fn check(&self, path: &Path, strict: bool) -> Result<()> {
        let k = key(path);

        if k.len() < 4 || !k.contains('\\') {
            bail!("refusing to touch a drive root ({k})");
        }
        for prot in &self.protected {
            if k == *prot {
                bail!("{k} is a protected folder");
            }
            if inside(prot, &k) {
                bail!("{k} contains the protected folder {prot}");
            }
        }
        if self.exact.iter().any(|e| inside(&k, e)) {
            return Ok(());
        }

        let depth = k.split('\\').filter(|c| !c.is_empty()).count();
        if depth < 3 {
            bail!("refusing to touch a top level folder ({k})");
        }

        let allowed = self.roots.iter().any(|r| {
            if strict {
                inside(&k, r) && k != *r
            } else {
                inside(&k, r)
            }
        });
        if !allowed {
            bail!("{k} is outside the allowed cleanup roots");
        }
        Ok(())
    }
}

impl Default for Guard {
    fn default() -> Self {
        Guard::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_drive_root() {
        let g = Guard::new();
        assert!(g.check_contents(Path::new("C:\\")).is_err());
    }

    #[test]
    fn rejects_windows() {
        let g = Guard::new();
        assert!(g.check_contents(Path::new("C:\\Windows")).is_err());
        assert!(g.check_contents(Path::new("C:\\Windows\\System32")).is_err());
    }

    #[test]
    fn rejects_outside_roots() {
        let g = Guard::new();
        assert!(g.check_contents(Path::new("D:\\Photos\\2024")).is_err());
    }

    #[test]
    fn blocklist_is_case_insensitive() {
        assert!(is_never_delete("Login Data"));
        assert!(is_never_delete("PLACES.SQLITE"));
        assert!(!is_never_delete("f_00001a"));
    }
}
