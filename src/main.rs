mod alias;
mod cli;
mod gitlog;
mod metrics;
mod push_check;
mod report;
mod time_filter;
mod timestamp;

use std::collections::HashSet;
use std::env;

use anyhow::{bail, Result};
use chrono::{Duration, Utc};
use clap::Parser;

use crate::alias::parse_aliases;
use crate::cli::Cli;
use crate::gitlog::fetch_commits;
use crate::metrics::{compute_metrics, AliasRule};
use crate::push_check::{run_push_check, PushCheckCli};
use crate::report::print_human_report;
use crate::time_filter::parse_time_filter;
use crate::timestamp::convert_unix_timestamp;

const DEFAULT_IGNORED_AUTHORS: &[&str] = &["BitsAdmin"];

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err:?}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match parse_command()? {
        AppCommand::Unix(ts) => {
            let conversion = convert_unix_timestamp(ts)?;
            let fmt = "%Y-%m-%d %H:%M:%S %:z";
            println!("Unix 时间戳：{}", conversion.timestamp);
            println!("UTC  时间：{}", conversion.utc.format(fmt));
            println!("本地时间：{}", conversion.local.format(fmt));
            Ok(())
        }
        AppCommand::PushCheck(cfg) => run_push_check(&cfg),
        AppCommand::CowHorse(cli) => run_cow_horse(cli),
    }
}

fn run_cow_horse(cli: Cli) -> Result<()> {
    let repo_path = cli
        .path
        .canonicalize()
        .unwrap_or_else(|_| cli.path.clone());
    let now = Utc::now();

    let since = if let Some(ref raw) = cli.since {
        Some(parse_time_filter(raw, now)?)
    } else if cli.window_days > 0 {
        Some(now - Duration::days(cli.window_days as i64))
    } else {
        None
    };

    let until = if let Some(ref raw) = cli.until {
        Some(parse_time_filter(raw, now)?)
    } else {
        None
    };

    if let (Some(s), Some(u)) = (since, until) {
        if s >= u {
            bail!("`since` must be earlier than `until`");
        }
    }

    let alias_map = parse_aliases(&cli.alias)?;

    let mut commits = fetch_commits(
        &repo_path,
        since,
        until,
        cli.author.as_deref(),
        cli.limit,
    )?;

    let mut ignored: HashSet<String> = DEFAULT_IGNORED_AUTHORS
        .iter()
        .map(|s| s.to_string())
        .collect();
    ignored.extend(cli.ignore_author.iter().cloned());

    if !ignored.is_empty() {
        commits.retain(|commit| !ignored.contains(&commit.author));
    }

    let mut alias_rules: Vec<AliasRule> = alias_map
        .iter()
        .map(|(from, to)| AliasRule {
            from: from.clone(),
            to: to.clone(),
        })
        .collect();
    alias_rules.sort_by(|a, b| a.from.cmp(&b.from));
    if !alias_map.is_empty() {
        for commit in &mut commits {
            if let Some(mapped) = alias_map.get(commit.author.as_str()) {
                commit.author = mapped.clone();
            }
        }
    }

    if commits.is_empty() {
        println!(
            "在 {} 中没有找到符合过滤条件的提交。",
            repo_path.display()
        );
        return Ok(());
    }

    let mut ignored_list: Vec<String> = ignored.into_iter().collect();
    ignored_list.sort();
    let metrics = compute_metrics(&repo_path, &commits, ignored_list, alias_rules);

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&metrics)?);
    } else {
        print_human_report(&metrics, &cli);
    }

    Ok(())
}

enum AppCommand {
    Unix(i64),
    PushCheck(PushCheckCli),
    CowHorse(Cli),
}

fn parse_command() -> Result<AppCommand> {
    let mut args: Vec<String> = env::args().collect();
    if args.len() > 1 && args[1] == "push-check" {
        let mut sub_args = Vec::with_capacity(args.len() - 1);
        sub_args.push(args[0].clone());
        sub_args.extend_from_slice(&args[2..]);
        let cfg = PushCheckCli::parse_from(sub_args);
        return Ok(AppCommand::PushCheck(cfg));
    }

    for arg in args.iter_mut().skip(1) {
        if arg == "-unix" {
            *arg = "--unix".to_string();
        }
    }

    let cli = Cli::parse_from(args);
    if let Some(ts) = cli.unix {
        Ok(AppCommand::Unix(ts))
    } else {
        Ok(AppCommand::CowHorse(cli))
    }
}
