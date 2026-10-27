//! Windows 11 targets: temp folders, caches, crash data, update leftovers and
//! the handful of operations that are not plain file deletion.

use super::{existing, existing_dirs, local, programdata, roaming, subdirs, system_drive, temp, windir};
use crate::model::{Action, Group, Risk, Target};
use std::path::PathBuf;

const G: Group = Group::System;
const APP: &str = "Windows";

pub fn targets() -> Vec<Target> {
    let mut out: Vec<Target> = Vec::new();
    let l = local();
    let r = roaming();
    let win = windir();
    let pd = programdata();
    let drive = system_drive();

    // ---- temp -------------------------------------------------------------
    dir(&mut out, "system.temp.user", "User temp folder (%TEMP%)", temp(), Risk::Safe);
    admin_dir(
        &mut out,
        "system.temp.windows",
        "System temp folder",
        win.join("Temp"),
        Risk::Safe,
    );
    admin_dir(
        &mut out,
        "system.temp.system-apps",
        "Per app system temp folder",
        win.join("SystemTemp"),
        Risk::Safe,
    );

    // ---- explorer and shell ----------------------------------------------
    dir(
        &mut out,
        "system.inetcache",
        "Internet cache (WinINet)",
        l.join("Microsoft\\Windows\\INetCache"),
        Risk::Safe,
    );
    dir_note(
        &mut out,
        "system.inetcookies",
        "Legacy WinINet cookies",
        l.join("Microsoft\\Windows\\INetCookies"),
        Risk::Caution,
        "signs you out of apps that use the old Internet Explorer stack",
    );
    dir_note(
        &mut out,
        "system.webcache",
        "WebCache database",
        l.join("Microsoft\\Windows\\WebCache"),
        Risk::Caution,
        "held open by Explorer; locked files are skipped",
    );
    if l.join("Microsoft\\Windows\\Explorer").is_dir() {
        out.push(
            Target::glob(
                "system.thumbnails",
                G,
                APP,
                "Thumbnail cache",
                l.join("Microsoft\\Windows\\Explorer"),
                "thumbcache_*.db",
            )
            .risk(Risk::Caution)
            .note("Explorer rebuilds these; thumbnails are slow again once"),
        );
        out.push(
            Target::glob(
                "system.iconcache",
                G,
                APP,
                "Icon cache",
                l.join("Microsoft\\Windows\\Explorer"),
                "iconcache_*.db",
            )
            .risk(Risk::Caution),
        );
    }
    dir_note(
        &mut out,
        "system.recent",
        "Recent files and jump lists",
        r.join("Microsoft\\Windows\\Recent"),
        Risk::Caution,
        "clears the recent list in Explorer and in app jump lists",
    );

    // ---- crash and error reporting ---------------------------------------
    dir(&mut out, "system.crashdumps", "Application crash dumps", l.join("CrashDumps"), Risk::Safe);
    let wer_user = l.join("Microsoft\\Windows\\WER");
    dirs_target(
        &mut out,
        "system.wer.user",
        "Error reports (this user)",
        vec![
            wer_user.join("ReportArchive"),
            wer_user.join("ReportQueue"),
            wer_user.join("Temp"),
        ],
        Risk::Safe,
        false,
    );
    let wer_machine = pd.join("Microsoft\\Windows\\WER");
    dirs_target(
        &mut out,
        "system.wer.machine",
        "Error reports (machine wide)",
        vec![
            wer_machine.join("ReportArchive"),
            wer_machine.join("ReportQueue"),
            wer_machine.join("Temp"),
        ],
        Risk::Safe,
        true,
    );

    // ---- graphics and font caches ----------------------------------------
    for (id, label, path) in [
        ("system.gpu.d3d", "DirectX shader cache", l.join("D3DSCache")),
        ("system.gpu.nvidia-dx", "NVIDIA DirectX cache", l.join("NVIDIA\\DXCache")),
        ("system.gpu.nvidia-gl", "NVIDIA OpenGL cache", l.join("NVIDIA\\GLCache")),
        ("system.gpu.nvidia-nv", "NVIDIA driver cache", l.join("NVIDIA Corporation\\NV_Cache")),
        ("system.gpu.amd-dx", "AMD DirectX cache", l.join("AMD\\DxCache")),
        ("system.gpu.amd-gl", "AMD OpenGL cache", l.join("AMD\\GLCache")),
        ("system.gpu.intel", "Intel shader cache", l.join("Intel\\ShaderCache")),
    ] {
        dir(&mut out, id, label, path, Risk::Safe);
    }
    dir(&mut out, "system.fontcache.user", "Font cache (this user)", l.join("FontCache"), Risk::Safe);
    if win
        .join("ServiceProfiles\\LocalService\\AppData\\Local\\FontCache")
        .is_dir()
    {
        out.push(
            Target::dir(
                "system.fontcache.service",
                G,
                APP,
                "Font cache (service)",
                win.join("ServiceProfiles\\LocalService\\AppData\\Local\\FontCache"),
            )
            .risk(Risk::Safe)
            .admin()
            .deep()
            .services(&["FontCache"]),
        );
    }

    // ---- store apps -------------------------------------------------------
    let packages = l.join("Packages");
    if packages.is_dir() {
        let package_dirs = subdirs(&packages);
        let inet: Vec<PathBuf> = package_dirs
            .iter()
            .map(|(_, p)| p.join("AC\\INetCache"))
            .collect();
        dirs_target(
            &mut out,
            "system.store.inetcache",
            "Store app web caches",
            inet,
            Risk::Safe,
            false,
        );
        let tempstate: Vec<PathBuf> = package_dirs
            .iter()
            .map(|(_, p)| p.join("TempState"))
            .collect();
        dirs_target(
            &mut out,
            "system.store.tempstate",
            "Store app temp state",
            tempstate,
            Risk::Caution,
            false,
        );
        let cookies: Vec<PathBuf> = package_dirs
            .iter()
            .map(|(_, p)| p.join("AC\\INetCookies"))
            .collect();
        dirs_target(
            &mut out,
            "system.store.cookies",
            "Store app cookies",
            cookies,
            Risk::Caution,
            false,
        );
    }

    // ---- update, servicing and logs --------------------------------------
    if win.join("SoftwareDistribution\\Download").is_dir() {
        out.push(
            Target::dir(
                "system.update.download",
                G,
                APP,
                "Windows Update download cache",
                win.join("SoftwareDistribution\\Download"),
            )
            .risk(Risk::Caution)
            .admin()
            .deep()
            .services(&["wuauserv", "bits"])
            .note("a pending update will download again"),
        );
    }
    let delivery = win
        .join("ServiceProfiles\\NetworkService\\AppData\\Local\\Microsoft\\Windows\\DeliveryOptimization\\Cache");
    if delivery.is_dir() {
        out.push(
            Target::dir(
                "system.update.delivery",
                G,
                APP,
                "Delivery Optimization cache",
                delivery,
            )
            .risk(Risk::Safe)
            .admin()
            .deep()
            .services(&["dosvc"]),
        );
    }
    for (id, label, sub) in [
        ("system.logs.cbs", "Servicing logs (CBS)", "Logs\\CBS"),
        ("system.logs.dism", "DISM logs", "Logs\\DISM"),
        ("system.logs.waasmedic", "Update medic logs", "Logs\\waasmedic"),
        ("system.logs.measuredboot", "Measured boot logs", "Logs\\MeasuredBoot"),
        ("system.logs.setup", "Setup logs", "Panther"),
    ] {
        admin_deep_dir(&mut out, id, label, win.join(sub), Risk::Safe);
    }

    // ---- kernel dumps and upgrade leftovers ------------------------------
    admin_deep_dir(&mut out, "system.minidump", "Kernel minidumps", win.join("Minidump"), Risk::Caution);
    let dumps = existing(vec![win.join("MEMORY.DMP"), win.join("LiveKernelReports")]);
    if !dumps.is_empty() {
        out.push(
            Target::paths("system.memorydump", G, APP, "Kernel memory dumps", dumps)
                .risk(Risk::Caution)
                .admin()
                .deep()
                .note("needed only if someone is debugging a blue screen"),
        );
    }
    let leftovers = existing(vec![
        drive.join("Windows.old"),
        drive.join("$Windows.~BT"),
        drive.join("$Windows.~WS"),
        drive.join("$WinREAgent"),
    ]);
    if !leftovers.is_empty() {
        out.push(
            Target::paths(
                "system.windows-old",
                G,
                APP,
                "Previous Windows installation",
                leftovers,
            )
            .risk(Risk::Destructive)
            .admin()
            .deep()
            .note("removes the ability to roll back the last Windows upgrade"),
        );
    }

    // ---- operations -------------------------------------------------------
    out.push(
        Target::action(
            "system.recycle-bin",
            G,
            APP,
            "Recycle Bin (all drives)",
            Action::EmptyRecycleBin,
        )
        .risk(Risk::Destructive)
        .note("deleted files are gone, not recoverable from the bin"),
    );
    out.push(
        Target::action("system.dns", G, APP, "DNS resolver cache", Action::FlushDns).risk(Risk::Safe),
    );
    out.push(
        Target::action(
            "system.eventlogs",
            G,
            APP,
            "Windows event logs",
            Action::ClearEventLogs,
        )
        .risk(Risk::Destructive)
        .admin()
        .deep()
        .note("destroys the audit trail; troubleshooting gets harder"),
    );
    out.push(
        Target::action(
            "system.component-store",
            G,
            APP,
            "WinSxS component store",
            Action::ComponentCleanup,
        )
        .risk(Risk::Caution)
        .admin()
        .deep()
        .note("runs DISM; can take several minutes and cannot be interrupted safely"),
    );

    out
}

// ---- small builders -------------------------------------------------------

fn dir(out: &mut Vec<Target>, id: &str, label: &str, path: PathBuf, risk: Risk) {
    if path.is_dir() {
        out.push(Target::dir(id, G, APP, label, path).risk(risk));
    }
}

fn dir_note(
    out: &mut Vec<Target>,
    id: &str,
    label: &str,
    path: PathBuf,
    risk: Risk,
    note: &'static str,
) {
    if path.is_dir() {
        out.push(Target::dir(id, G, APP, label, path).risk(risk).note(note));
    }
}

fn admin_dir(out: &mut Vec<Target>, id: &str, label: &str, path: PathBuf, risk: Risk) {
    if path.is_dir() {
        out.push(Target::dir(id, G, APP, label, path).risk(risk).admin());
    }
}

fn admin_deep_dir(out: &mut Vec<Target>, id: &str, label: &str, path: PathBuf, risk: Risk) {
    if path.is_dir() {
        out.push(
            Target::dir(id, G, APP, label, path)
                .risk(risk)
                .admin()
                .deep(),
        );
    }
}

fn dirs_target(
    out: &mut Vec<Target>,
    id: &str,
    label: &str,
    paths: Vec<PathBuf>,
    risk: Risk,
    admin: bool,
) {
    let present = existing_dirs(paths);
    if present.is_empty() {
        return;
    }
    let mut target = Target::dirs(id, G, APP, label, present).risk(risk);
    if admin {
        target = target.admin();
    }
    out.push(target);
}
