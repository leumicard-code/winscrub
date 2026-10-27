//! Selection, measurement and removal.

use rayon::prelude::*;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::actions;
use crate::cli::{Level, Select};
use crate::fsops::{self, Removal};
use crate::model::{Action, Group, Risk, Target, TargetKind};
use crate::report;
use crate::safety::Guard;
use crate::winapi;

pub struct Ctx {
    pub guard: Guard,
    pub elevated: bool,
    pub running: HashSet<String>,
    pub dry: bool,
}

impl Ctx {
    pub fn new(dry: bool) -> Self {
        Ctx {
            guard: Guard::new(),
            elevated: winapi::is_elevated(),
            running: winapi::running_processes(),
            dry,
        }
    }

    pub fn refresh_processes(&mut self) {
        self.running = winapi::running_processes();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// Has content and may be cleaned.
    Ready,
    /// Exists but there is nothing in it.
    Empty,
    /// Needs an elevated process.
    NeedsAdmin,
    /// An application is holding it open.
    Blocked(String),
    /// The safety guard said no.
    Refused(String),
}

impl Status {
    pub fn is_ready(&self) -> bool {
        matches!(self, Status::Ready)
    }

    pub fn label(&self) -> String {
        match self {
            Status::Ready => "ready".to_string(),
            Status::Empty => "empty".to_string(),
            Status::NeedsAdmin => "needs admin".to_string(),
            Status::Blocked(who) => format!("blocked, {who}"),
            Status::Refused(why) => format!("refused, {why}"),
        }
    }
}

pub struct Item {
    pub target: Target,
    pub bytes: u64,
    pub files: u64,
    pub status: Status,
    pub outcome: Option<Removal>,
    pub message: Option<String>,
    pub error: Option<String>,
}

#[derive(Default, Debug)]
pub struct Summary {
    pub freed: u64,
    pub removed: u64,
    pub skipped_files: u64,
    pub cleaned_targets: usize,
    pub failed_targets: usize,
    pub skipped_targets: usize,
}

// ---- selection ------------------------------------------------------------

fn wanted_groups(sel: &Select) -> Vec<Group> {
    let mut groups = Vec::new();
    if sel.browsers {
        groups.push(Group::Browsers);
    }
    if sel.system {
        groups.push(Group::System);
    }
    if sel.apps {
        groups.push(Group::Apps);
    }
    if groups.is_empty() {
        groups.extend_from_slice(&Group::ALL);
    }
    groups
}

fn level_allows(level: Level, risk: Risk) -> bool {
    match level {
        Level::Safe => risk == Risk::Safe,
        Level::Caution => risk != Risk::Destructive,
        Level::All => true,
    }
}

/// An id matches a pattern when it is the pattern, sits under it as a dotted
/// prefix, or matches it as a glob.
fn id_matches(patterns: &[String], id: &str) -> bool {
    patterns.iter().any(|pattern| {
        let p = pattern.trim().to_ascii_lowercase();
        if p.is_empty() {
            return false;
        }
        id == p || id.starts_with(&format!("{p}.")) || fsops::wildcard_match(id, &p)
    })
}

/// Naming a target explicitly with --only overrides the risk level and --deep,
/// because the user has already said which one they mean.
pub fn select(targets: Vec<Target>, sel: &Select) -> Vec<Target> {
    let groups = wanted_groups(sel);
    targets
        .into_iter()
        .filter(|t| {
            if !groups.contains(&t.group) {
                return false;
            }
            if id_matches(&sel.skip, &t.id) {
                return false;
            }
            if !sel.only.is_empty() {
                return id_matches(&sel.only, &t.id);
            }
            if t.deep && !sel.deep {
                return false;
            }
            level_allows(sel.level, t.risk)
        })
        .collect()
}

// ---- scanning -------------------------------------------------------------

fn group_order(group: Group) -> u8 {
    match group {
        Group::Browsers => 0,
        Group::System => 1,
        Group::Apps => 2,
    }
}

fn guard_check(guard: &Guard, kind: &TargetKind) -> Result<(), String> {
    let result = match kind {
        TargetKind::DirContents(paths) => paths.iter().try_for_each(|p| guard.check_contents(p)),
        TargetKind::Paths(paths) => paths.iter().try_for_each(|p| guard.check_delete(p)),
        TargetKind::Glob { dir, .. } => guard.check_contents(dir),
        TargetKind::Action(_) => Ok(()),
    };
    result.map_err(|e| e.to_string())
}

fn measure_target(target: &Target) -> (u64, u64) {
    match &target.kind {
        TargetKind::DirContents(paths) => {
            let m = fsops::measure_all(paths);
            (m.bytes, m.files)
        }
        TargetKind::Paths(paths) => {
            let m = fsops::measure_all(paths);
            (m.bytes, m.files)
        }
        TargetKind::Glob { dir, pattern } => {
            let m = fsops::measure_glob(dir, pattern);
            (m.bytes, m.files)
        }
        TargetKind::Action(Action::EmptyRecycleBin) => winapi::recycle_bin_info().unwrap_or((0, 0)),
        TargetKind::Action(_) => (0, 0),
    }
}

fn scan_one(ctx: &Ctx, target: Target) -> Item {
    let mut status = Status::Ready;

    if target.admin && !ctx.elevated {
        status = Status::NeedsAdmin;
    }
    if status.is_ready() {
        if let Some(proc_name) = target
            .close
            .iter()
            .find(|p| ctx.running.contains(&p.to_ascii_lowercase()))
        {
            status = Status::Blocked(format!("{proc_name} is running"));
        }
    }
    if status.is_ready() {
        if let Err(why) = guard_check(&ctx.guard, &target.kind) {
            status = Status::Refused(why);
        }
    }

    let (bytes, files) = measure_target(&target);
    let nothing_to_do = match &target.kind {
        TargetKind::Action(Action::EmptyRecycleBin) => bytes == 0 && files == 0,
        TargetKind::Action(_) => false,
        _ => bytes == 0 && files == 0,
    };
    if status.is_ready() && nothing_to_do {
        status = Status::Empty;
    }

    Item {
        target,
        bytes,
        files,
        status,
        outcome: None,
        message: None,
        error: None,
    }
}

pub fn scan(ctx: &Ctx, targets: Vec<Target>, progress: bool) -> Vec<Item> {
    let total = targets.len();
    let done = AtomicUsize::new(0);

    let mut items: Vec<Item> = targets
        .into_par_iter()
        .map(|target| {
            let item = scan_one(ctx, target);
            if progress {
                report::progress(done.fetch_add(1, Ordering::Relaxed) + 1, total);
            }
            item
        })
        .collect();

    if progress {
        report::progress_clear();
    }

    items.sort_by(|a, b| {
        group_order(a.target.group)
            .cmp(&group_order(b.target.group))
            .then_with(|| {
                a.target
                    .app
                    .to_ascii_lowercase()
                    .cmp(&b.target.app.to_ascii_lowercase())
            })
            .then_with(|| b.bytes.cmp(&a.bytes))
            .then_with(|| a.target.id.cmp(&b.target.id))
    });
    items
}

// ---- cleaning -------------------------------------------------------------

pub fn clean(ctx: &Ctx, items: &mut [Item], verbose: bool) -> Summary {
    let mut summary = Summary::default();

    for item in items.iter_mut() {
        if !item.status.is_ready() {
            summary.skipped_targets += 1;
            continue;
        }

        // Some targets hold a service open. Stop it, clean, put it back.
        let mut restart: Vec<&'static str> = Vec::new();
        if !ctx.dry {
            for service in item.target.services.clone() {
                match actions::stop_service(service) {
                    Ok(true) => restart.push(service),
                    Ok(false) => {}
                    Err(e) => item.error = Some(format!("{service}: {e}")),
                }
            }
        }

        let kind = item.target.kind.clone();
        let known_bytes = item.bytes;
        let known_files = item.files;

        let result: Result<Removal, String> = match &kind {
            TargetKind::DirContents(paths) => {
                let mut removal = Removal::default();
                for path in paths {
                    removal.merge(fsops::delete_dir_contents(path, &ctx.guard, ctx.dry));
                }
                Ok(removal)
            }
            TargetKind::Paths(paths) => Ok(fsops::delete_paths(paths, &ctx.guard, ctx.dry)),
            TargetKind::Glob { dir, pattern } => {
                Ok(fsops::delete_glob(dir, pattern, &ctx.guard, ctx.dry))
            }
            TargetKind::Action(action) => match actions::perform(*action, ctx.dry) {
                Ok(message) => {
                    item.message = Some(message);
                    let mut removal = Removal::default();
                    if *action == Action::EmptyRecycleBin {
                        removal.freed = known_bytes;
                        removal.removed = known_files;
                    }
                    Ok(removal)
                }
                Err(e) => Err(format!("{e:#}")),
            },
        };

        for service in restart {
            if let Err(e) = actions::start_service(service) {
                item.error = Some(format!("{service} was not restarted: {e}"));
            }
        }

        match result {
            Ok(removal) => {
                summary.freed += removal.freed;
                summary.removed += removal.removed;
                summary.skipped_files += removal.skipped;
                summary.cleaned_targets += 1;
                item.outcome = Some(removal);
            }
            Err(e) => {
                summary.failed_targets += 1;
                item.error = Some(e);
            }
        }

        report::clean_line(item, verbose);
    }

    summary
}
