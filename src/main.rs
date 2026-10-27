//! winscrub - browser and Windows 11 cleanup.
//!
//! `scan` measures, `clean` deletes, `list` enumerates and `doctor` shows the
//! safety configuration. Nothing is removed without an explicit `clean`.

#[cfg(not(windows))]
compile_error!("winscrub uses the Win32 API and only builds for Windows");

mod actions;
mod cli;
mod engine;
mod fsops;
mod model;
mod report;
mod safety;
mod targets;
mod winapi;

use anyhow::Result;
use clap::Parser;
use std::time::Duration;

use cli::{CleanArgs, Cli, Cmd, ListArgs, ScanArgs};
use engine::Ctx;
use fsops::human;
use model::Risk;
use safety::Guard;

fn main() {
    let cli = Cli::parse();
    report::init(cli.no_color);

    let code = match dispatch(&cli) {
        Ok(code) => code,
        Err(err) => {
            report::error(&format!("{err:#}"));
            1
        }
    };
    std::process::exit(code);
}

fn dispatch(cli: &Cli) -> Result<i32> {
    match &cli.cmd {
        Cmd::List(args) => cmd_list(args),
        Cmd::Doctor => cmd_doctor(),
        Cmd::Scan(args) => cmd_scan(args, cli.verbose),
        Cmd::Clean(args) => cmd_clean(args, cli.verbose),
    }
}

fn cmd_list(args: &ListArgs) -> Result<i32> {
    let mut selected = engine::select(targets::collect(), &args.select);
    selected.sort_by(|a, b| {
        a.group
            .as_str()
            .cmp(b.group.as_str())
            .then_with(|| a.app.to_ascii_lowercase().cmp(&b.app.to_ascii_lowercase()))
            .then_with(|| a.id.cmp(&b.id))
    });
    if selected.is_empty() {
        report::warn("no targets match this selection");
        return Ok(0);
    }
    report::print_list(&selected, args.paths);
    Ok(0)
}

fn cmd_doctor() -> Result<i32> {
    let guard = Guard::new();
    report::print_doctor(&guard, winapi::is_elevated());
    let found = targets::collect().len();
    println!("  {found} targets detected on this machine");
    report::blank();
    Ok(0)
}

fn cmd_scan(args: &ScanArgs, _verbose: bool) -> Result<i32> {
    let ctx = Ctx::new(true);
    let selected = engine::select(targets::collect(), &args.select);
    if selected.is_empty() {
        report::warn("no targets match this selection");
        return Ok(0);
    }

    let items = engine::scan(&ctx, selected, true);
    let total = report::print_scan(&items, args.all);
    report::print_scan_footer(&items, total);

    if let Some(path) = &args.json {
        report::write_json(path, "scan", ctx.elevated, true, &items, None)?;
    }
    Ok(0)
}

fn cmd_clean(args: &CleanArgs, verbose: bool) -> Result<i32> {
    let mut ctx = Ctx::new(args.dry_run);
    let selected = engine::select(targets::collect(), &args.select);
    if selected.is_empty() {
        report::warn("no targets match this selection");
        return Ok(0);
    }

    // Hand the whole command line to a fresh elevated process when the
    // selection needs rights this one does not have.
    if args.elevate && !ctx.elevated && selected.iter().any(|t| t.admin) {
        winapi::relaunch_elevated()?;
        println!("  re-launched with administrator rights in a new window");
        return Ok(0);
    }

    if args.close_browsers {
        close_blocking_apps(&mut ctx, &selected, args.yes);
    }

    let mut items = engine::scan(&ctx, selected, true);
    let total = report::print_scan(&items, false);
    report::print_scan_footer(&items, total);

    let ready = items.iter().filter(|i| i.status.is_ready()).count();
    if ready == 0 {
        report::blank();
        println!("  nothing to do");
        return Ok(0);
    }

    let destructive = items
        .iter()
        .any(|i| i.status.is_ready() && i.target.risk == Risk::Destructive);

    if !args.yes && !args.dry_run {
        report::blank();
        let question = if destructive {
            format!(
                "this selection is irreversible and frees {}. continue?",
                human(total)
            )
        } else {
            format!("delete {} from {ready} targets?", human(total))
        };
        if !report::confirm(&question, destructive) {
            println!("  cancelled, nothing was deleted");
            return Ok(0);
        }
    }

    report::blank();
    if args.dry_run {
        println!("  dry run: no file will be deleted");
        report::blank();
    }

    let summary = engine::clean(&ctx, &mut items, verbose);
    report::print_clean_summary(&summary, args.dry_run);

    if let Some(path) = &args.json {
        report::write_json(
            path,
            "clean",
            ctx.elevated,
            args.dry_run,
            &items,
            Some(&summary),
        )?;
    }

    Ok(if summary.failed_targets > 0 { 2 } else { 0 })
}

/// Ask the applications holding selected targets open to quit.
fn close_blocking_apps(ctx: &mut Ctx, selected: &[model::Target], assume_yes: bool) {
    let mut names: Vec<String> = selected
        .iter()
        .flat_map(|t| t.close.iter().map(|p| p.to_string()))
        .collect();
    names.sort();
    names.dedup();

    let running: Vec<String> = names
        .into_iter()
        .filter(|n| ctx.running.contains(&n.to_ascii_lowercase()))
        .collect();
    if running.is_empty() {
        return;
    }

    println!("  running: {}", running.join(", "));
    if !assume_yes && !report::confirm("close these applications now?", false) {
        return;
    }

    let stubborn = actions::close_processes(&running, Duration::from_secs(8));
    if !stubborn.is_empty() {
        report::warn(&format!(
            "still running after a forced close: {}",
            stubborn.join(", ")
        ));
    }
    ctx.refresh_processes();
}
