//! Browser targets: every Chromium fork plus the Firefox family.
//!
//! Chromium keeps one cache tree per profile under `User Data\<Profile>`, and a
//! few shared caches at the `User Data` level. Firefox splits a profile in two:
//! caches in LocalAppData, everything else in Roaming.
//!
//! Deliberately absent: saved passwords, payment methods, bookmarks and the
//! `Local State` key file. Those are also in the global never-delete list.

use super::{existing, local, roaming, slug, subdirs};
use crate::model::{Group, Risk, Target};
use std::path::{Path, PathBuf};

const G: Group = Group::Browsers;

struct Chromium {
    key: &'static str,
    name: &'static str,
    root: PathBuf,
    procs: &'static [&'static str],
}

struct Gecko {
    key: &'static str,
    name: &'static str,
    roaming_root: PathBuf,
    local_root: PathBuf,
    procs: &'static [&'static str],
}

pub fn targets() -> Vec<Target> {
    let mut out = Vec::new();
    for browser in chromium_browsers() {
        chromium_targets(&browser, &mut out);
    }
    for browser in gecko_browsers() {
        gecko_targets(&browser, &mut out);
    }
    out
}

fn chromium_browsers() -> Vec<Chromium> {
    let l = local();
    let r = roaming();
    vec![
        Chromium {
            key: "chrome",
            name: "Google Chrome",
            root: l.join("Google\\Chrome\\User Data"),
            procs: &["chrome.exe"],
        },
        Chromium {
            key: "chrome-beta",
            name: "Chrome Beta",
            root: l.join("Google\\Chrome Beta\\User Data"),
            procs: &["chrome.exe"],
        },
        Chromium {
            key: "chrome-canary",
            name: "Chrome Canary",
            root: l.join("Google\\Chrome SxS\\User Data"),
            procs: &["chrome.exe"],
        },
        Chromium {
            key: "edge",
            name: "Microsoft Edge",
            root: l.join("Microsoft\\Edge\\User Data"),
            procs: &["msedge.exe"],
        },
        Chromium {
            key: "edge-webview",
            name: "Edge WebView2",
            root: l.join("Microsoft\\EdgeWebView\\User Data"),
            procs: &["msedgewebview2.exe"],
        },
        Chromium {
            key: "brave",
            name: "Brave",
            root: l.join("BraveSoftware\\Brave-Browser\\User Data"),
            procs: &["brave.exe"],
        },
        Chromium {
            key: "vivaldi",
            name: "Vivaldi",
            root: l.join("Vivaldi\\User Data"),
            procs: &["vivaldi.exe"],
        },
        Chromium {
            key: "chromium",
            name: "Chromium",
            root: l.join("Chromium\\User Data"),
            procs: &["chrome.exe"],
        },
        Chromium {
            key: "yandex",
            name: "Yandex Browser",
            root: l.join("Yandex\\YandexBrowser\\User Data"),
            procs: &["browser.exe"],
        },
        Chromium {
            key: "arc",
            name: "Arc",
            root: l.join("Packages\\TheBrowserCompany.Arc_ttt1ap7aakyb4\\LocalCache\\Local\\Arc\\User Data"),
            procs: &["arc.exe"],
        },
        Chromium {
            key: "opera",
            name: "Opera",
            root: r.join("Opera Software\\Opera Stable"),
            procs: &["opera.exe"],
        },
        Chromium {
            key: "opera-gx",
            name: "Opera GX",
            root: r.join("Opera Software\\Opera GX Stable"),
            procs: &["opera.exe"],
        },
    ]
}

/// Profile directories inside a `User Data` root. Opera stores its single
/// profile directly in the root, so detect that shape too.
fn chromium_profiles(root: &Path) -> Vec<(String, PathBuf)> {
    if !root.is_dir() {
        return Vec::new();
    }
    if root.join("Preferences").is_file() {
        return vec![("Default".to_string(), root.to_path_buf())];
    }
    subdirs(root)
        .into_iter()
        .filter(|(name, path)| {
            name == "Default"
                || name == "Guest Profile"
                || name == "System Profile"
                || name.starts_with("Profile ")
                || path.join("Preferences").is_file()
        })
        .collect()
}

fn chromium_targets(browser: &Chromium, out: &mut Vec<Target>) {
    if !browser.root.is_dir() {
        return;
    }
    let close = browser.procs;

    // Caches shared by every profile.
    for (sub, label) in [
        ("ShaderCache", "GPU shader cache"),
        ("GrShaderCache", "Skia shader cache"),
        ("GraphiteDawnCache", "Dawn shader cache"),
        ("component_crx_cache", "Component download cache"),
        ("extensions_crx_cache", "Extension download cache"),
        ("Crashpad\\reports", "Crash reports"),
        ("Safe Browsing", "Safe Browsing lists"),
    ] {
        push_dir(
            out,
            format!("{}.shared.{}", browser.key, slug(sub)),
            browser.name,
            label,
            browser.root.join(sub),
            Risk::Safe,
            close,
        );
    }

    for (profile, dir) in chromium_profiles(&browser.root) {
        let pid = slug(&profile);

        for (sub, label) in [
            ("Cache", "HTTP cache"),
            ("Code Cache", "JavaScript and WASM code cache"),
            ("GPUCache", "GPU cache"),
            ("DawnCache", "Dawn cache"),
            ("DawnGraphiteCache", "Dawn graphite cache"),
            ("DawnWebGPUCache", "WebGPU cache"),
            ("Media Cache", "Media cache"),
            ("Application Cache", "Legacy application cache"),
            ("Service Worker\\CacheStorage", "Service worker cache storage"),
            ("Service Worker\\ScriptCache", "Service worker scripts"),
            ("blob_storage", "Temporary blob storage"),
            ("File System\\Cache", "File system cache"),
        ] {
            push_dir(
                out,
                format!("{}.{}.{}", browser.key, pid, slug(sub)),
                browser.name,
                format!("{label} ({profile})"),
                dir.join(sub),
                Risk::Safe,
                close,
            );
        }

        push_paths(
            out,
            format!("{}.{}.cookies", browser.key, pid),
            browser.name,
            format!("Cookies ({profile})"),
            vec![
                dir.join("Network\\Cookies"),
                dir.join("Network\\Cookies-journal"),
                dir.join("Cookies"),
                dir.join("Cookies-journal"),
            ],
            Risk::Caution,
            close,
            Some("signs you out of every site in this profile"),
        );

        push_paths(
            out,
            format!("{}.{}.history", browser.key, pid),
            browser.name,
            format!("Browsing history ({profile})"),
            vec![
                dir.join("History"),
                dir.join("History-journal"),
                dir.join("History Provider Cache"),
                dir.join("Top Sites"),
                dir.join("Top Sites-journal"),
                dir.join("Visited Links"),
                dir.join("Shortcuts"),
                dir.join("Shortcuts-journal"),
                dir.join("Favicons"),
                dir.join("Favicons-journal"),
            ],
            Risk::Caution,
            close,
            Some("also clears the download list and address bar suggestions"),
        );

        push_paths(
            out,
            format!("{}.{}.sessions", browser.key, pid),
            browser.name,
            format!("Saved tabs and session ({profile})"),
            vec![
                dir.join("Sessions"),
                dir.join("Session Storage"),
                dir.join("Current Session"),
                dir.join("Current Tabs"),
                dir.join("Last Session"),
                dir.join("Last Tabs"),
            ],
            Risk::Caution,
            close,
            Some("\"reopen closed tabs\" will come back empty"),
        );
    }
}

fn gecko_browsers() -> Vec<Gecko> {
    let l = local();
    let r = roaming();
    vec![
        Gecko {
            key: "firefox",
            name: "Mozilla Firefox",
            roaming_root: r.join("Mozilla\\Firefox\\Profiles"),
            local_root: l.join("Mozilla\\Firefox\\Profiles"),
            procs: &["firefox.exe"],
        },
        Gecko {
            key: "librewolf",
            name: "LibreWolf",
            roaming_root: r.join("librewolf\\Profiles"),
            local_root: l.join("librewolf\\Profiles"),
            procs: &["librewolf.exe"],
        },
        Gecko {
            key: "waterfox",
            name: "Waterfox",
            roaming_root: r.join("Waterfox\\Profiles"),
            local_root: l.join("Waterfox\\Profiles"),
            procs: &["waterfox.exe"],
        },
        Gecko {
            key: "zen",
            name: "Zen Browser",
            roaming_root: r.join("zen\\Profiles"),
            local_root: l.join("zen\\Profiles"),
            procs: &["zen.exe"],
        },
        Gecko {
            key: "tor",
            name: "Tor Browser",
            roaming_root: r.join("tor browser\\Profiles"),
            local_root: l.join("tor browser\\Profiles"),
            procs: &["tor.exe", "firefox.exe"],
        },
    ]
}

fn gecko_targets(browser: &Gecko, out: &mut Vec<Target>) {
    let close = browser.procs;

    for (profile, dir) in subdirs(&browser.local_root) {
        let pid = slug(&profile);
        for (sub, label) in [
            ("cache2", "HTTP cache"),
            ("startupCache", "Startup cache"),
            ("thumbnails", "Page thumbnails"),
            ("shader-cache", "Shader cache"),
            ("jumpListCache", "Jump list cache"),
            ("OfflineCache", "Offline cache"),
            ("safebrowsing", "Safe Browsing lists"),
        ] {
            push_dir(
                out,
                format!("{}.{}.{}", browser.key, pid, slug(sub)),
                browser.name,
                format!("{label} ({profile})"),
                dir.join(sub),
                Risk::Safe,
                close,
            );
        }
    }

    for (profile, dir) in subdirs(&browser.roaming_root) {
        let pid = slug(&profile);

        push_paths(
            out,
            format!("{}.{}.cookies", browser.key, pid),
            browser.name,
            format!("Cookies ({profile})"),
            vec![
                dir.join("cookies.sqlite"),
                dir.join("cookies.sqlite-wal"),
                dir.join("cookies.sqlite-shm"),
            ],
            Risk::Caution,
            close,
            Some("signs you out of every site in this profile"),
        );

        push_paths(
            out,
            format!("{}.{}.sessions", browser.key, pid),
            browser.name,
            format!("Saved tabs and session ({profile})"),
            vec![
                dir.join("sessionstore-backups"),
                dir.join("sessionstore.jsonlz4"),
                dir.join("recovery.jsonlz4"),
            ],
            Risk::Caution,
            close,
            None,
        );

        push_paths(
            out,
            format!("{}.{}.formhistory", browser.key, pid),
            browser.name,
            format!("Saved form entries ({profile})"),
            vec![
                dir.join("formhistory.sqlite"),
                dir.join("webappsstore.sqlite"),
            ],
            Risk::Caution,
            close,
            Some("Firefox history is not offered: it shares places.sqlite with bookmarks"),
        );

        push_dir(
            out,
            format!("{}.{}.crashes", browser.key, pid),
            browser.name,
            format!("Crash reports ({profile})"),
            dir.join("crashes"),
            Risk::Safe,
            close,
        );
    }
}

fn push_dir(
    out: &mut Vec<Target>,
    id: String,
    app: &str,
    label: impl Into<String>,
    path: PathBuf,
    risk: Risk,
    close: &[&'static str],
) {
    if path.is_dir() {
        out.push(Target::dir(id, G, app, label, path).risk(risk).close(close));
    }
}

#[allow(clippy::too_many_arguments)]
fn push_paths(
    out: &mut Vec<Target>,
    id: String,
    app: &str,
    label: impl Into<String>,
    paths: Vec<PathBuf>,
    risk: Risk,
    close: &[&'static str],
    note: Option<&'static str>,
) {
    let present = existing(paths);
    if present.is_empty() {
        return;
    }
    let mut target = Target::paths(id, G, app, label, present)
        .risk(risk)
        .close(close);
    if let Some(note) = note {
        target = target.note(note);
    }
    out.push(target);
}
