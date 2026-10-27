//! Command line surface.

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    name = "winscrub",
    version,
    about = "Clean browser caches and Windows 11 junk from one place",
    long_about = "winscrub measures and removes browser caches, cookies, history, \
Windows temp folders, crash dumps, update leftovers and app caches.\n\n\
Nothing is deleted without an explicit `clean` and a confirmation. Every target \
is scoped to a fixed list of cleanup roots, credential and bookmark files are \
never touched, and `scan` is always read only."
)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Cmd,

    /// Never emit ANSI colour
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Print every path as it is processed
    #[arg(long, short, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Measure what could be freed. Never deletes anything.
    Scan(ScanArgs),
    /// Delete the selected targets.
    Clean(CleanArgs),
    /// Print every target that exists on this machine.
    List(ListArgs),
    /// Show the safety configuration: roots, protected folders, privileges.
    Doctor,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Level {
    /// Caches and temp files only. Nothing user visible is lost.
    Safe,
    /// Also cookies, history, saved tabs, prefetch and recent files.
    Caution,
    /// Also irreversible targets: recycle bin, event logs, Windows.old.
    All,
}

#[derive(Args, Debug, Clone)]
pub struct Select {
    /// Include browser targets
    #[arg(long)]
    pub browsers: bool,

    /// Include Windows targets
    #[arg(long)]
    pub system: bool,

    /// Include third party app targets
    #[arg(long)]
    pub apps: bool,

    /// Restrict to these target ids or id prefixes (comma separated, `*` allowed)
    #[arg(long, value_delimiter = ',', value_name = "ID")]
    pub only: Vec<String>,

    /// Exclude these target ids or id prefixes
    #[arg(long, value_delimiter = ',', value_name = "ID")]
    pub skip: Vec<String>,

    /// How much risk to accept
    #[arg(long, value_enum, default_value_t = Level::Safe)]
    pub level: Level,

    /// Include slow, admin only targets (update cache, WinSxS, event logs)
    #[arg(long)]
    pub deep: bool,
}

#[derive(Args, Debug)]
pub struct ScanArgs {
    #[command(flatten)]
    pub select: Select,

    /// Also list targets that are already empty
    #[arg(long)]
    pub all: bool,

    /// Write a machine readable report here
    #[arg(long, value_name = "FILE")]
    pub json: Option<String>,
}

#[derive(Args, Debug)]
pub struct CleanArgs {
    #[command(flatten)]
    pub select: Select,

    /// Do not prompt for confirmation
    #[arg(long, short = 'y')]
    pub yes: bool,

    /// Go through the whole run without deleting anything
    #[arg(long)]
    pub dry_run: bool,

    /// Ask running browsers to close first (graceful, then forced)
    #[arg(long)]
    pub close_browsers: bool,

    /// Re-launch through UAC when admin targets are selected
    #[arg(long)]
    pub elevate: bool,

    /// Write a machine readable report here
    #[arg(long, value_name = "FILE")]
    pub json: Option<String>,
}

#[derive(Args, Debug)]
pub struct ListArgs {
    #[command(flatten)]
    pub select: Select,

    /// Show the paths behind every target
    #[arg(long)]
    pub paths: bool,
}
