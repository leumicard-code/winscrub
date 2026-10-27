//! Core data model: what a cleanup target is, how risky it is, and how it is removed.

use serde::Serialize;
use std::path::PathBuf;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Group {
    Browsers,
    System,
    Apps,
}

impl Group {
    pub fn as_str(self) -> &'static str {
        match self {
            Group::Browsers => "browsers",
            Group::System => "system",
            Group::Apps => "apps",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Group::Browsers => "BROWSERS",
            Group::System => "WINDOWS",
            Group::Apps => "APPS",
        }
    }

    pub const ALL: [Group; 3] = [Group::Browsers, Group::System, Group::Apps];
}

/// How much a user loses if a target is wiped.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    /// Caches and temp files. Regenerated automatically, nothing is lost.
    Safe,
    /// Convenience data: cookies, history, saved tabs, prefetch, recent files.
    Caution,
    /// One way street: recycle bin, event logs, Windows.old.
    Destructive,
}

impl Risk {
    pub fn as_str(self) -> &'static str {
        match self {
            Risk::Safe => "safe",
            Risk::Caution => "caution",
            Risk::Destructive => "destructive",
        }
    }
}

/// Built-in operations that are not "delete these files".
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    EmptyRecycleBin,
    FlushDns,
    ClearEventLogs,
    ComponentCleanup,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Action::EmptyRecycleBin => "empty recycle bin",
            Action::FlushDns => "ipconfig /flushdns",
            Action::ClearEventLogs => "wevtutil cl (every log)",
            Action::ComponentCleanup => "dism /cleanup-image /startcomponentcleanup",
        }
    }
}

#[derive(Clone, Debug)]
pub enum TargetKind {
    /// Delete everything inside each directory, keep the directories themselves.
    DirContents(Vec<PathBuf>),
    /// Delete each path outright (file or directory).
    Paths(Vec<PathBuf>),
    /// Delete direct children of `dir` whose name matches `pattern` (`*` and `?`).
    Glob { dir: PathBuf, pattern: String },
    /// Run a built-in action instead of deleting paths.
    Action(Action),
}

#[derive(Clone, Debug)]
pub struct Target {
    /// Stable dotted id, e.g. `chrome.default.cache`. Used by --only / --skip.
    pub id: String,
    pub group: Group,
    /// Owning application, used as a table heading.
    pub app: String,
    /// Human description of what gets removed.
    pub label: String,
    pub risk: Risk,
    pub kind: TargetKind,
    /// Needs an elevated process.
    pub admin: bool,
    /// Slow or heavy; only runs with --deep.
    pub deep: bool,
    /// Process image names that must not be running.
    pub close: Vec<&'static str>,
    /// Services stopped before the target is cleaned and restarted afterwards.
    pub services: Vec<&'static str>,
    pub note: Option<&'static str>,
}

impl Target {
    pub fn new(
        id: impl Into<String>,
        group: Group,
        app: impl Into<String>,
        label: impl Into<String>,
        kind: TargetKind,
    ) -> Self {
        Target {
            id: id.into(),
            group,
            app: app.into(),
            label: label.into(),
            risk: Risk::Safe,
            kind,
            admin: false,
            deep: false,
            close: Vec::new(),
            services: Vec::new(),
            note: None,
        }
    }

    pub fn dir(
        id: impl Into<String>,
        group: Group,
        app: impl Into<String>,
        label: impl Into<String>,
        path: PathBuf,
    ) -> Self {
        Target::new(id, group, app, label, TargetKind::DirContents(vec![path]))
    }

    pub fn dirs(
        id: impl Into<String>,
        group: Group,
        app: impl Into<String>,
        label: impl Into<String>,
        paths: Vec<PathBuf>,
    ) -> Self {
        Target::new(id, group, app, label, TargetKind::DirContents(paths))
    }

    pub fn paths(
        id: impl Into<String>,
        group: Group,
        app: impl Into<String>,
        label: impl Into<String>,
        paths: Vec<PathBuf>,
    ) -> Self {
        Target::new(id, group, app, label, TargetKind::Paths(paths))
    }

    pub fn glob(
        id: impl Into<String>,
        group: Group,
        app: impl Into<String>,
        label: impl Into<String>,
        dir: PathBuf,
        pattern: impl Into<String>,
    ) -> Self {
        let pattern = pattern.into();
        Target::new(id, group, app, label, TargetKind::Glob { dir, pattern })
    }

    pub fn action(
        id: impl Into<String>,
        group: Group,
        app: impl Into<String>,
        label: impl Into<String>,
        action: Action,
    ) -> Self {
        Target::new(id, group, app, label, TargetKind::Action(action))
    }

    pub fn risk(mut self, risk: Risk) -> Self {
        self.risk = risk;
        self
    }

    pub fn admin(mut self) -> Self {
        self.admin = true;
        self
    }

    pub fn deep(mut self) -> Self {
        self.deep = true;
        self
    }

    pub fn close(mut self, procs: &[&'static str]) -> Self {
        self.close = procs.to_vec();
        self
    }

    pub fn services(mut self, services: &[&'static str]) -> Self {
        self.services = services.to_vec();
        self
    }

    pub fn note(mut self, note: &'static str) -> Self {
        self.note = Some(note);
        self
    }

    /// Paths this target would touch, for display and reports.
    pub fn display_paths(&self) -> Vec<String> {
        match &self.kind {
            TargetKind::DirContents(paths) => paths
                .iter()
                .map(|p| format!("{}{}*", p.display(), std::path::MAIN_SEPARATOR))
                .collect(),
            TargetKind::Paths(paths) => paths.iter().map(|p| p.display().to_string()).collect(),
            TargetKind::Glob { dir, pattern } => vec![format!(
                "{}{}{}",
                dir.display(),
                std::path::MAIN_SEPARATOR,
                pattern
            )],
            TargetKind::Action(a) => vec![a.as_str().to_string()],
        }
    }

    /// Whether anything is actually on disk for this target right now.
    pub fn exists(&self) -> bool {
        match &self.kind {
            TargetKind::DirContents(paths) => paths.iter().any(|p| p.is_dir()),
            TargetKind::Paths(paths) => paths.iter().any(|p| p.exists()),
            TargetKind::Glob { dir, .. } => dir.is_dir(),
            TargetKind::Action(_) => true,
        }
    }

    pub fn is_action(&self) -> bool {
        matches!(self.kind, TargetKind::Action(_))
    }
}
