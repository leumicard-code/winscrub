//! Operations that are not "delete these files": shell calls, services and
//! asking applications to close.

use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::os::windows::process::CommandExt;
use std::process::{Command, Output, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use crate::model::Action;
use crate::winapi;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn run(program: &str, args: &[&str]) -> Result<Output> {
    Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("could not run {program}"))
}

/// Run a built-in action. Returns a one line description of what happened.
pub fn perform(action: Action, dry: bool) -> Result<String> {
    if dry {
        return Ok(format!("would run: {}", action.as_str()));
    }
    match action {
        Action::FlushDns => {
            let out = run("ipconfig", &["/flushdns"])?;
            if !out.status.success() {
                bail!("ipconfig /flushdns exited with {}", out.status);
            }
            Ok("DNS resolver cache flushed".to_string())
        }
        Action::EmptyRecycleBin => {
            winapi::empty_recycle_bin()?;
            Ok("Recycle Bin emptied".to_string())
        }
        Action::ClearEventLogs => clear_event_logs(),
        Action::ComponentCleanup => component_cleanup(),
    }
}

fn clear_event_logs() -> Result<String> {
    let listed = run("wevtutil", &["el"])?;
    let names = String::from_utf8_lossy(&listed.stdout);
    let mut cleared = 0usize;
    let mut refused = 0usize;
    for name in names.lines().map(str::trim).filter(|l| !l.is_empty()) {
        match run("wevtutil", &["cl", name]) {
            Ok(out) if out.status.success() => cleared += 1,
            _ => refused += 1,
        }
    }
    if cleared == 0 {
        bail!("no event log could be cleared (is this process elevated?)");
    }
    Ok(format!("cleared {cleared} event logs, {refused} refused"))
}

fn component_cleanup() -> Result<String> {
    let out = run(
        "Dism.exe",
        &["/Online", "/Cleanup-Image", "/StartComponentCleanup", "/Quiet"],
    )?;
    if !out.status.success() {
        bail!("DISM exited with {}", out.status);
    }
    Ok("WinSxS component store cleaned".to_string())
}

// ---- services -------------------------------------------------------------

fn service_state(name: &str) -> String {
    match run("sc", &["query", name]) {
        Ok(out) => String::from_utf8_lossy(&out.stdout).to_uppercase(),
        Err(_) => String::new(),
    }
}

/// Stop a service and report whether it had been running, so the caller knows
/// whether to start it again afterwards.
pub fn stop_service(name: &str) -> Result<bool> {
    if !service_state(name).contains("RUNNING") {
        return Ok(false);
    }
    run("net", &["stop", name, "/y"])?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if service_state(name).contains("STOPPED") {
            return Ok(true);
        }
        sleep(Duration::from_millis(400));
    }
    bail!("{name} did not stop in time")
}

pub fn start_service(name: &str) -> Result<()> {
    let out = run("net", &["start", name])?;
    if !out.status.success() {
        let text = String::from_utf8_lossy(&out.stdout).to_uppercase();
        // "already started" is the outcome we wanted anyway.
        if !text.contains("ALREADY") {
            bail!("could not restart {name}");
        }
    }
    Ok(())
}

// ---- closing applications --------------------------------------------------

/// Ask the named processes to close, then force the stragglers.
/// Returns the image names that were still running at the end.
pub fn close_processes(names: &[String], grace: Duration) -> Vec<String> {
    let running = winapi::running_processes();
    let wanted: Vec<&String> = names
        .iter()
        .filter(|n| running.contains(&n.to_ascii_lowercase()))
        .collect();
    if wanted.is_empty() {
        return Vec::new();
    }

    for name in &wanted {
        let _ = run("taskkill", &["/IM", name.as_str()]);
    }

    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        let now: HashSet<String> = winapi::running_processes();
        if !wanted
            .iter()
            .any(|n| now.contains(&n.to_ascii_lowercase()))
        {
            return Vec::new();
        }
        sleep(Duration::from_millis(500));
    }

    for name in &wanted {
        let _ = run("taskkill", &["/IM", name.as_str(), "/F", "/T"]);
    }
    sleep(Duration::from_millis(800));

    let now = winapi::running_processes();
    wanted
        .into_iter()
        .filter(|n| now.contains(&n.to_ascii_lowercase()))
        .cloned()
        .collect()
}
