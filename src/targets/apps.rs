//! Third party app caches. Electron apps keep Chromium cache trees of their
//! own, and package managers keep download caches that are always re-fetchable.

use super::{local, roaming, slug};
use crate::model::{Group, Risk, Target};
use std::path::PathBuf;

const G: Group = Group::Apps;

pub fn targets() -> Vec<Target> {
    let mut out: Vec<Target> = Vec::new();
    let l = local();
    let r = roaming();

    // ---- Electron style apps ---------------------------------------------
    let electron: Vec<(&str, &str, PathBuf, &[&'static str])> = vec![
        ("discord", "Discord", r.join("discord"), &["discord.exe"]),
        ("slack", "Slack", r.join("Slack"), &["slack.exe"]),
        (
            "teams",
            "Microsoft Teams (classic)",
            r.join("Microsoft\\Teams"),
            &["teams.exe"],
        ),
        ("signal", "Signal", r.join("Signal"), &["signal.exe"]),
        ("whatsapp", "WhatsApp", r.join("WhatsApp"), &["whatsapp.exe"]),
        ("obsidian", "Obsidian", r.join("obsidian"), &["obsidian.exe"]),
        ("vscode", "Visual Studio Code", r.join("Code"), &["code.exe"]),
        (
            "vscode-insiders",
            "VS Code Insiders",
            r.join("Code - Insiders"),
            &["code - insiders.exe"],
        ),
    ];

    for (key, name, root, procs) in electron.into_iter() {
        if !root.is_dir() {
            continue;
        }
        for sub in [
            "Cache",
            "CachedData",
            "Code Cache",
            "GPUCache",
            "DawnCache",
            "blob_storage",
            "Service Worker\\CacheStorage",
            "logs",
            "tmp",
            "Crashpad\\reports",
        ] {
            push_dir(
                &mut out,
                format!("{key}.{}", slug(sub)),
                name,
                format!("{sub} cache"),
                root.join(sub),
                Risk::Safe,
                procs,
            );
        }
    }

    // VS Code keeps per workspace state that is cheap to lose but not free.
    push_dir_note(
        &mut out,
        "vscode.workspace-storage",
        "Visual Studio Code",
        "Per workspace state",
        r.join("Code\\User\\workspaceStorage"),
        Risk::Caution,
        &["code.exe"],
        "resets per folder layout, search history and some extension state",
    );

    // ---- media and games --------------------------------------------------
    for (id, app, label, path, procs) in [
        (
            "spotify.data",
            "Spotify",
            "Offline audio cache",
            l.join("Spotify\\Data"),
            &["spotify.exe"] as &[&'static str],
        ),
        (
            "spotify.storage",
            "Spotify",
            "Storage cache",
            l.join("Spotify\\Storage"),
            &["spotify.exe"],
        ),
        (
            "spotify.browser",
            "Spotify",
            "Embedded browser cache",
            l.join("Spotify\\Browser\\Cache"),
            &["spotify.exe"],
        ),
        (
            "steam.htmlcache",
            "Steam",
            "Store and overlay web cache",
            l.join("Steam\\htmlcache"),
            &["steam.exe"],
        ),
        (
            "epic.logs",
            "Epic Games Launcher",
            "Launcher logs",
            l.join("EpicGamesLauncher\\Saved\\Logs"),
            &["epicgameslauncher.exe"],
        ),
        (
            "telegram.cache",
            "Telegram Desktop",
            "Message cache",
            r.join("Telegram Desktop\\tdata\\user_data\\cache"),
            &["telegram.exe"],
        ),
        (
            "telegram.media",
            "Telegram Desktop",
            "Media cache",
            r.join("Telegram Desktop\\tdata\\user_data\\media_cache"),
            &["telegram.exe"],
        ),
    ] {
        push_dir(&mut out, id.to_string(), app, label, path, Risk::Safe, procs);
    }

    let epic_saved = l.join("EpicGamesLauncher\\Saved");
    if epic_saved.is_dir() {
        out.push(
            Target::glob(
                "epic.webcache",
                G,
                "Epic Games Launcher",
                "Launcher web cache",
                epic_saved,
                "webcache*",
            )
            .risk(Risk::Safe)
            .close(&["epicgameslauncher.exe"]),
        );
    }

    // ---- developer package caches ----------------------------------------
    for (id, app, label, path) in [
        ("pip.cache", "pip", "Downloaded wheels", l.join("pip\\Cache")),
        ("npm.cache", "npm", "Package cache", l.join("npm-cache\\_cacache")),
        ("yarn.cache", "Yarn", "Package cache", l.join("Yarn\\Cache")),
        ("nuget.cache", "NuGet", "HTTP cache", l.join("NuGet\\v3-cache")),
        (
            "nuget.plugins",
            "NuGet",
            "Plugin cache",
            l.join("NuGet\\plugins-cache"),
        ),
    ] {
        push_dir(&mut out, id.to_string(), app, label, path, Risk::Safe, &[]);
    }

    out
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
fn push_dir_note(
    out: &mut Vec<Target>,
    id: &str,
    app: &str,
    label: &str,
    path: PathBuf,
    risk: Risk,
    close: &[&'static str],
    note: &'static str,
) {
    if path.is_dir() {
        out.push(
            Target::dir(id, G, app, label, path)
                .risk(risk)
                .close(close)
                .note(note),
        );
    }
}
