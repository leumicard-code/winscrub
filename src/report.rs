//! Everything the user sees, plus the JSON report.

use serde::Serialize;
use std::collections::BTreeMap;
use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::engine::{Item, Summary};
use crate::fsops::{human, thousands};
use crate::model::{Group, Risk, Target};
use crate::safety::Guard;

static COLOR: AtomicBool = AtomicBool::new(false);

pub fn init(no_color: bool) {
    let wanted = !no_color
        && std::env::var_os("NO_COLOR").is_none()
        && std::io::stdout().is_terminal()
        && crate::winapi::enable_virtual_terminal();
    COLOR.store(wanted, Ordering::Relaxed);
}

fn paint(text: &str, code: &str) -> String {
    if COLOR.load(Ordering::Relaxed) {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

fn bold(t: &str) -> String {
    paint(t, "1")
}
fn dim(t: &str) -> String {
    paint(t, "2")
}
fn red(t: &str) -> String {
    paint(t, "31")
}
fn green(t: &str) -> String {
    paint(t, "32")
}
fn yellow(t: &str) -> String {
    paint(t, "33")
}
fn cyan(t: &str) -> String {
    paint(t, "36")
}

/// Colour the risk word after padding, so ANSI codes never skew a column.
fn risk_cell(risk: Risk, width: usize) -> String {
    let word = match risk {
        Risk::Safe => "safe",
        Risk::Caution => "caution",
        Risk::Destructive => "DESTRUCTIVE",
    };
    let padded = pad(word, width);
    match risk {
        Risk::Safe => dim(&padded),
        Risk::Caution => yellow(&padded),
        Risk::Destructive => red(&padded),
    }
}

fn pad(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len >= width {
        text.to_string()
    } else {
        format!("{text}{}", " ".repeat(width - len))
    }
}

fn rpad(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len >= width {
        text.to_string()
    } else {
        format!("{}{text}", " ".repeat(width - len))
    }
}

// ---- simple messages ------------------------------------------------------

pub fn error(msg: &str) {
    eprintln!("{} {msg}", red("error:"));
}

pub fn warn(msg: &str) {
    eprintln!("{} {msg}", yellow("warning:"));
}

pub fn hint(msg: &str) {
    println!("  {} {msg}", dim("hint:"));
}

pub fn blank() {
    println!();
}

pub fn progress(done: usize, total: usize) {
    if COLOR.load(Ordering::Relaxed) {
        eprint!("\r\x1b[2K  measuring {done}/{total}");
        let _ = std::io::stderr().flush();
    }
}

pub fn progress_clear() {
    if COLOR.load(Ordering::Relaxed) {
        eprint!("\r\x1b[2K");
        let _ = std::io::stderr().flush();
    }
}

// ---- scan table -----------------------------------------------------------

pub fn print_scan(items: &[Item], show_all: bool) -> u64 {
    let id_width = items
        .iter()
        .map(|i| i.target.id.chars().count())
        .max()
        .unwrap_or(20)
        .clamp(20, 42);

    let mut total = 0u64;
    let mut shown = 0usize;
    let mut by_group: BTreeMap<u8, Vec<&Item>> = BTreeMap::new();
    for item in items {
        let key = match item.target.group {
            Group::Browsers => 0u8,
            Group::System => 1,
            Group::Apps => 2,
        };
        by_group.entry(key).or_default().push(item);
    }

    for (key, group_items) in &by_group {
        let group = match *key {
            0 => Group::Browsers,
            1 => Group::System,
            _ => Group::Apps,
        };
        let group_total: u64 = group_items
            .iter()
            .filter(|i| i.status.is_ready())
            .map(|i| i.bytes)
            .sum();
        let visible: Vec<&&Item> = group_items
            .iter()
            .filter(|i| show_all || i.status.is_ready())
            .collect();
        if visible.is_empty() {
            continue;
        }

        println!();
        println!(
            "{}  {}",
            bold(group.title()),
            dim(&format!("{} reclaimable", human(group_total)))
        );

        let mut current_app = String::new();
        for item in visible {
            if item.target.app != current_app {
                current_app = item.target.app.clone();
                println!("  {}", cyan(&current_app));
            }
            shown += 1;
            if item.status.is_ready() {
                total += item.bytes;
                println!(
                    "    {} {} {} {}  {}",
                    rpad(&human(item.bytes), 10),
                    rpad(&thousands(item.files), 9),
                    pad(&item.target.id, id_width),
                    risk_cell(item.target.risk, 11),
                    dim(&item.target.label)
                );
            } else {
                println!(
                    "    {} {} {} {}  {}",
                    dim(&rpad("-", 10)),
                    dim(&rpad("-", 9)),
                    pad(&item.target.id, id_width),
                    yellow(&pad(&item.status.label(), 11)),
                    dim(&item.target.label)
                );
            }
            if let Some(note) = item.target.note {
                println!("      {}", dim(&format!("note: {note}")));
            }
        }
    }

    if shown == 0 {
        println!();
        println!("  {}", dim("nothing to clean for this selection"));
    }
    total
}

pub fn print_scan_footer(items: &[Item], total: u64) {
    let ready = items.iter().filter(|i| i.status.is_ready()).count();
    let files: u64 = items
        .iter()
        .filter(|i| i.status.is_ready())
        .map(|i| i.files)
        .sum();

    println!();
    println!(
        "  {} across {} targets ({} items)",
        bold(&human(total)),
        ready,
        thousands(files)
    );

    let needs_admin = items
        .iter()
        .filter(|i| matches!(i.status, crate::engine::Status::NeedsAdmin))
        .count();
    if needs_admin > 0 {
        hint(&format!(
            "{needs_admin} targets need an elevated shell; add --elevate to clean"
        ));
    }

    let blocked: Vec<&str> = items
        .iter()
        .filter(|i| matches!(i.status, crate::engine::Status::Blocked(_)))
        .map(|i| i.target.app.as_str())
        .collect();
    if !blocked.is_empty() {
        let mut names: Vec<&str> = blocked;
        names.sort_unstable();
        names.dedup();
        hint(&format!(
            "close {} (or pass --close-browsers) to include those targets",
            names.join(", ")
        ));
    }
}

// ---- list and doctor ------------------------------------------------------

pub fn print_list(targets: &[Target], show_paths: bool) {
    let mut current_app = String::new();
    for target in targets {
        if target.app != current_app {
            current_app = target.app.clone();
            println!();
            println!("{} {}", cyan(&current_app), dim(target.group.as_str()));
        }
        let mut flags = Vec::new();
        if target.admin {
            flags.push("admin".to_string());
        }
        if target.deep {
            flags.push("deep".to_string());
        }
        if !target.close.is_empty() {
            flags.push(format!("close {}", target.close.join(" ")));
        }
        let flag_text = if flags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", flags.join(", "))
        };
        println!(
            "  {} {}  {}{}",
            pad(&target.id, 40),
            risk_cell(target.risk, 11),
            target.label,
            dim(&flag_text)
        );
        if show_paths {
            for path in target.display_paths() {
                println!("      {}", dim(&path));
            }
        }
    }
    println!();
    println!("  {} targets available here", targets.len());
}

pub fn print_doctor(guard: &Guard, elevated: bool) {
    println!();
    println!("{}", bold("winscrub doctor"));
    println!(
        "  elevated: {}",
        if elevated {
            green("yes")
        } else {
            yellow("no (admin targets will be skipped)")
        }
    );

    println!();
    println!("{}", bold("cleanup roots"));
    println!("  {}", dim("deletion is refused anywhere outside these"));
    for root in guard.roots() {
        println!("    {root}");
    }

    println!();
    println!("{}", bold("protected folders"));
    println!(
        "  {}",
        dim("never emptied, and never contained by a target")
    );
    for path in guard.protected() {
        println!("    {path}");
    }

    println!();
    println!("{}", bold("never deleted, by name"));
    for chunk in crate::safety::NEVER_DELETE.chunks(4) {
        println!("    {}", chunk.join("  "));
    }
    println!();
}

// ---- clean ----------------------------------------------------------------

pub fn clean_line(item: &Item, verbose: bool) {
    let id = pad(&item.target.id, 42);
    if let Some(err) = &item.error {
        println!("  {} {id} {}", red("x"), dim(err));
        return;
    }
    match &item.outcome {
        Some(outcome) => {
            let detail = if let Some(message) = &item.message {
                message.clone()
            } else {
                format!(
                    "{} in {} items",
                    human(outcome.freed),
                    thousands(outcome.removed)
                )
            };
            let skipped = if outcome.skipped > 0 {
                dim(&format!(" ({} locked)", thousands(outcome.skipped)))
            } else {
                String::new()
            };
            println!("  {} {id} {detail}{skipped}", green("+"));
            if verbose {
                for err in &outcome.errors {
                    println!("      {}", dim(err));
                }
            }
        }
        None => println!("  {} {id} {}", dim("-"), dim(&item.status.label())),
    }
}

pub fn print_clean_summary(summary: &Summary, dry: bool) {
    println!();
    let headline = if dry {
        format!(
            "{} would be freed from {} targets ({} items)",
            bold(&human(summary.freed)),
            summary.cleaned_targets,
            thousands(summary.removed)
        )
    } else {
        format!(
            "{} freed from {} targets ({} items)",
            bold(&human(summary.freed)),
            summary.cleaned_targets,
            thousands(summary.removed)
        )
    };
    println!("  {headline}");
    if summary.skipped_files > 0 {
        println!(
            "  {}",
            dim(&format!(
                "{} files were locked or protected and left alone",
                thousands(summary.skipped_files)
            ))
        );
    }
    if summary.skipped_targets > 0 {
        println!(
            "  {}",
            dim(&format!("{} targets skipped", summary.skipped_targets))
        );
    }
    if summary.failed_targets > 0 {
        println!(
            "  {}",
            yellow(&format!("{} targets failed", summary.failed_targets))
        );
    }
    println!();
}

/// Ask before deleting. Destructive selections demand the whole word.
pub fn confirm(question: &str, require_word: bool) -> bool {
    let suffix = if require_word {
        "type yes to continue"
    } else {
        "[y/N]"
    };
    print!("  {question} {} ", dim(suffix));
    let _ = std::io::stdout().flush();

    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    let answer = line.trim().to_ascii_lowercase();
    if require_word {
        answer == "yes"
    } else {
        answer == "y" || answer == "yes"
    }
}

// ---- json -----------------------------------------------------------------

#[derive(Serialize)]
struct JsonItem {
    id: String,
    group: &'static str,
    app: String,
    label: String,
    risk: &'static str,
    status: String,
    bytes: u64,
    items: u64,
    paths: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    freed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    removed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    locked: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<String>,
}

#[derive(Serialize)]
struct JsonReport {
    tool: &'static str,
    version: &'static str,
    mode: &'static str,
    generated_unix: u64,
    elevated: bool,
    dry_run: bool,
    total_bytes: u64,
    total_items: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    freed_bytes: Option<u64>,
    targets: Vec<JsonItem>,
}

pub fn write_json(
    path: &str,
    mode: &'static str,
    elevated: bool,
    dry_run: bool,
    items: &[Item],
    summary: Option<&Summary>,
) -> anyhow::Result<()> {
    let targets: Vec<JsonItem> = items
        .iter()
        .map(|item| JsonItem {
            id: item.target.id.clone(),
            group: item.target.group.as_str(),
            app: item.target.app.clone(),
            label: item.target.label.clone(),
            risk: item.target.risk.as_str(),
            status: item.status.label(),
            bytes: item.bytes,
            items: item.files,
            paths: item.target.display_paths(),
            freed: item.outcome.as_ref().map(|o| o.freed),
            removed: item.outcome.as_ref().map(|o| o.removed),
            locked: item.outcome.as_ref().map(|o| o.skipped),
            errors: item
                .outcome
                .as_ref()
                .map(|o| o.errors.clone())
                .unwrap_or_default(),
        })
        .collect();

    let report = JsonReport {
        tool: "winscrub",
        version: env!("CARGO_PKG_VERSION"),
        mode,
        generated_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        elevated,
        dry_run,
        total_bytes: items
            .iter()
            .filter(|i| i.status.is_ready())
            .map(|i| i.bytes)
            .sum(),
        total_items: items
            .iter()
            .filter(|i| i.status.is_ready())
            .map(|i| i.files)
            .sum(),
        freed_bytes: summary.map(|s| s.freed),
        targets,
    };

    let json = serde_json::to_string_pretty(&report)?;
    std::fs::write(path, json)?;
    println!("  {}", dim(&format!("report written to {path}")));
    Ok(())
}
