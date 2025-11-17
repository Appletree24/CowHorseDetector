use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, TimeZone, Timelike, Utc, Weekday};
use clap::Parser;
use serde::Serialize;

const DEFAULT_IGNORED_AUTHORS: &[&str] = &["BitsAdmin"];

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err:?}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
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

    if commits.is_empty() {
        println!(
            "在 {} 中没有找到符合过滤条件的提交。",
            repo_path.display()
        );
        return Ok(());
    }

    let mut ignored_list: Vec<String> = ignored.into_iter().collect();
    ignored_list.sort();
    let metrics = compute_metrics(&repo_path, &commits, ignored_list);

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&metrics)?);
    } else {
        print_human_report(&metrics, &cli);
    }

    Ok(())
}

#[derive(Parser, Debug)]
#[command(
    name = "cow-horse",
    about = "根据 Git 提交历史衡量一个仓库的“牛马”程度。"
)]
struct Cli {
    /// Path to the git repository to inspect
    #[arg(long, default_value = ".", value_name = "PATH")]
    path: PathBuf,

    /// Only include commits after this instant (e.g. 2023-01-01 or 30d for 30 days ago)
    #[arg(long, value_name = "SINCE")]
    since: Option<String>,

    /// Only include commits before this instant
    #[arg(long, value_name = "UNTIL")]
    until: Option<String>,

    /// Default rolling window (in days) when --since is omitted
    #[arg(long, default_value_t = 90, value_name = "DAYS")]
    window_days: u32,

    /// Filter commits by author substring (passed through to git)
    #[arg(long, value_name = "AUTHOR")]
    author: Option<String>,

    /// Limit the number of commits to read (useful for massive histories)
    #[arg(long, value_name = "COMMITS")]
    limit: Option<usize>,

    /// Output JSON instead of the human summary
    #[arg(long)]
    json: bool,

    /// Authors to drop from the stats (can repeat)
    #[arg(long = "ignore-author", value_name = "AUTHOR")]
    ignore_author: Vec<String>,
}

fn fetch_commits(
    repo_path: &Path,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    author: Option<&str>,
    limit: Option<usize>,
) -> Result<Vec<Commit>> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo_path);
    cmd.args([
        "log",
        "--no-color",
        "--pretty=format:%H\x1f%an\x1f%ad",
        "--date=iso-strict",
    ]);

    if let Some(since) = since {
        cmd.arg(format!("--since={}", since.to_rfc3339()));
    }

    if let Some(until) = until {
        cmd.arg(format!("--until={}", until.to_rfc3339()));
    }

    if let Some(author) = author {
        cmd.arg(format!("--author={author}"));
    }

    if let Some(limit) = limit {
        cmd.arg(format!("-n{limit}"));
    }

    let output = cmd
        .output()
        .with_context(|| format!("failed to execute `git log` in {}", repo_path.display()))?;

    if !output.status.success() {
        bail!(
            "git log failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8(output.stdout)?;
    let mut commits = Vec::new();

    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.split('\x1f');
        let _hash = parts
            .next()
            .ok_or_else(|| anyhow!("git log output missing hash column"))?;
        let author = parts
            .next()
            .ok_or_else(|| anyhow!("git log output missing author column"))?;
        let timestamp_str = parts
            .next()
            .ok_or_else(|| anyhow!("git log output missing timestamp column"))?;
        let timestamp =
            DateTime::parse_from_rfc3339(timestamp_str).with_context(|| {
                format!("failed to parse timestamp {timestamp_str:?}")
            })?;

        commits.push(Commit {
            author: author.to_string(),
            timestamp,
        });
    }

    Ok(commits)
}

#[derive(Debug)]
struct Commit {
    author: String,
    timestamp: DateTime<FixedOffset>,
}

#[derive(Debug, Serialize)]
struct RepoMetrics {
    repo_path: PathBuf,
    analysis_start: Option<DateTime<FixedOffset>>,
    analysis_end: Option<DateTime<FixedOffset>>,
    total_commits: usize,
    unique_authors: usize,
    after_hours_commits: usize,
    weekend_commits: usize,
    night_commits: usize,
    commit_days: usize,
    overtime_days: usize,
    longest_streak_days: usize,
    busiest_day: Option<BusiestDay>,
    severity_score: f64,
    severity_label: String,
    top_after_hours_authors: Vec<AuthorSummary>,
    chill_authors: Vec<AuthorSummary>,
    ignored_authors: Vec<String>,
}

#[derive(Debug, Serialize, Default, Clone)]
struct DayStats {
    total_commits: usize,
    after_hours_commits: usize,
}

#[derive(Default)]
struct AuthorAccumulator {
    total_commits: usize,
    after_hours_commits: usize,
    weekend_commits: usize,
    night_commits: usize,
}

#[derive(Debug, Serialize, Clone)]
struct BusiestDay {
    date: NaiveDate,
    total_commits: usize,
    after_hours_commits: usize,
}

#[derive(Debug, Serialize, Clone)]
struct AuthorSummary {
    name: String,
    total_commits: usize,
    after_hours_commits: usize,
    weekend_commits: usize,
    night_commits: usize,
    after_hours_ratio: f64,
}

fn compute_metrics(
    repo_path: &Path,
    commits: &[Commit],
    ignored_authors: Vec<String>,
) -> RepoMetrics {
    let mut after_hours = 0usize;
    let mut weekend = 0usize;
    let mut night = 0usize;
    let mut day_stats: BTreeMap<NaiveDate, DayStats> = BTreeMap::new();
    let mut author_stats: HashMap<String, AuthorAccumulator> = HashMap::new();
    let mut analysis_start = None;
    let mut analysis_end = None;

    for commit in commits {
        if analysis_start.map_or(true, |s| commit.timestamp < s) {
            analysis_start = Some(commit.timestamp);
        }
        if analysis_end.map_or(true, |e| commit.timestamp > e) {
            analysis_end = Some(commit.timestamp);
        }

        let date = commit.timestamp.date_naive();
        let weekday = commit.timestamp.weekday();
        let hour = commit.timestamp.hour();
        let is_weekend = matches!(weekday, Weekday::Sat | Weekday::Sun);
        let is_after_hours = hour < 10 || hour >= 19;
        let is_night = hour < 6 || hour >= 23;

        if is_after_hours {
            after_hours += 1;
        }

        if is_weekend {
            weekend += 1;
        }

        if is_night {
            night += 1;
        }

        let entry = day_stats.entry(date).or_default();
        entry.total_commits += 1;
        if is_after_hours {
            entry.after_hours_commits += 1;
        }

        let author_entry = author_stats
            .entry(commit.author.clone())
            .or_default();
        author_entry.total_commits += 1;
        if is_after_hours {
            author_entry.after_hours_commits += 1;
        }
        if is_weekend {
            author_entry.weekend_commits += 1;
        }
        if is_night {
            author_entry.night_commits += 1;
        }
    }

    let commit_days = day_stats.len();
    let overtime_days = day_stats
        .values()
        .filter(|stats| stats.after_hours_commits > 0)
        .count();
    let longest_streak_days = longest_streak(day_stats.keys().copied());
    let busiest_day = day_stats
        .iter()
        .max_by(|(_, a), (_, b)| {
            a.total_commits
                .cmp(&b.total_commits)
                .then(a.after_hours_commits.cmp(&b.after_hours_commits))
        })
        .map(|(date, stats)| BusiestDay {
            date: *date,
            total_commits: stats.total_commits,
            after_hours_commits: stats.after_hours_commits,
        });

    let unique_authors = author_stats.len();

    let mut author_summaries: Vec<AuthorSummary> = author_stats
        .into_iter()
        .map(|(name, stats)| {
            let ratio = percentage(stats.after_hours_commits, stats.total_commits);
            AuthorSummary {
                name,
                total_commits: stats.total_commits,
                after_hours_commits: stats.after_hours_commits,
                weekend_commits: stats.weekend_commits,
                night_commits: stats.night_commits,
                after_hours_ratio: ratio,
            }
        })
        .collect();

    let mut nightowls = author_summaries.clone();
    nightowls.sort_by(|a, b| {
        b.after_hours_ratio
            .partial_cmp(&a.after_hours_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.after_hours_commits.cmp(&a.after_hours_commits))
    });
    nightowls.truncate(3);

    author_summaries.sort_by(|a, b| {
        a.after_hours_ratio
            .partial_cmp(&b.after_hours_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.total_commits.cmp(&a.total_commits))
    });
    let mut chill_authors = author_summaries;
    chill_authors.truncate(3);

    let total_commits = commits.len();
    let severity_score = severity_score(
        total_commits,
        after_hours,
        weekend,
        night,
        overtime_days,
        commit_days,
        longest_streak_days,
    );
    let severity_label = severity_label(severity_score).to_string();

    RepoMetrics {
        repo_path: repo_path.to_path_buf(),
        analysis_start,
        analysis_end,
        total_commits,
        unique_authors,
        after_hours_commits: after_hours,
        weekend_commits: weekend,
        night_commits: night,
        commit_days,
        overtime_days,
        longest_streak_days,
        busiest_day,
        severity_score,
        severity_label,
        top_after_hours_authors: nightowls,
        chill_authors,
        ignored_authors,
    }
}

fn longest_streak<I>(dates: I) -> usize
where
    I: IntoIterator<Item = NaiveDate>,
{
    let mut prev: Option<NaiveDate> = None;
    let mut current = 0usize;
    let mut best = 0usize;

    for date in dates {
        match prev {
            None => {
                current = 1;
            }
            Some(prev_date) => {
                let diff = date.signed_duration_since(prev_date).num_days();
                if diff == 1 {
                    current += 1;
                } else {
                    current = 1;
                }
            }
        }

        best = best.max(current);
        prev = Some(date);
    }

    best
}

fn severity_score(
    total: usize,
    after_hours: usize,
    weekend: usize,
    night: usize,
    overtime_days: usize,
    commit_days: usize,
    longest_streak: usize,
) -> f64 {
    if total == 0 {
        return 0.0;
    }

    let after_hours_ratio = percentage(after_hours, total);
    let weekend_ratio = percentage(weekend, total);
    let night_ratio = percentage(night, total);
    let overtime_day_ratio = if commit_days == 0 {
        0.0
    } else {
        percentage(overtime_days, commit_days)
    };
    let streak_factor = (longest_streak.min(14) as f64) / 14.0;

    let score = after_hours_ratio * 40.0
        + weekend_ratio * 20.0
        + night_ratio * 20.0
        + overtime_day_ratio * 10.0
        + streak_factor * 10.0;

    score.min(100.0)
}

fn severity_label(score: f64) -> &'static str {
    match score as u32 {
        0..=20 => "轻松自在",
        21..=40 => "基本健康",
        41..=60 => "持续加班",
        61..=80 => "半牛马状态",
        _ => "全面牛马预警",
    }
}

fn percentage(part: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 / total as f64
    }
}

fn parse_time_filter(value: &str, now: DateTime<Utc>) -> Result<DateTime<Utc>> {
    if let Some(relative) = try_parse_relative(value, now) {
        return Ok(relative);
    }

    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Ok(dt.with_timezone(&Utc));
    }

    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        let naive = date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| anyhow!("invalid date: {date}"))?;
        return Ok(Utc.from_utc_datetime(&naive));
    }

    bail!("Cannot parse time filter {value:?}");
}

fn try_parse_relative(value: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let trimmed = value.trim().to_ascii_lowercase();
    if trimmed.len() < 2 {
        return None;
    }

    let (amount_part, suffix) = trimmed.split_at(trimmed.len() - 1);
    let num: i64 = amount_part.parse().ok()?;
    if num <= 0 {
        return None;
    }

    let duration = match suffix.chars().next()? {
        'd' => Duration::days(num),
        'w' => Duration::weeks(num),
        'h' => Duration::hours(num),
        _ => return None,
    };

    Some(now - duration)
}

fn print_human_report(metrics: &RepoMetrics, cli: &Cli) {
    println!("仓库：{}", metrics.repo_path.display());
    if let (Some(start), Some(end)) = (&metrics.analysis_start, &metrics.analysis_end) {
        println!(
            "时间范围：{}  ->  {}",
            format_timestamp(start),
            format_timestamp(end)
        );
    }

    if cli.author.is_some() {
        println!("作者过滤：{}", cli.author.as_deref().unwrap());
    }

    if !metrics.ignored_authors.is_empty() {
        println!("忽略作者：{}", metrics.ignored_authors.join(", "));
    }

    println!(
        "分析提交：{}（作者：{} 人，活跃天数：{} 天）",
        metrics.total_commits, metrics.unique_authors, metrics.commit_days
    );
    println!(
        "牛马指数：{:>5.1}/100 -> {}",
        metrics.severity_score, metrics.severity_label
    );
    println!(
        "下班后提交：{}（{:.1}%）",
        metrics.after_hours_commits,
        percentage(metrics.after_hours_commits, metrics.total_commits) * 100.0
    );
    println!(
        "周末提交：{}（{:.1}%）",
        metrics.weekend_commits,
        percentage(metrics.weekend_commits, metrics.total_commits) * 100.0
    );
    println!(
        "深夜提交 (23:00-05:59)：{}（{:.1}%）",
        metrics.night_commits,
        percentage(metrics.night_commits, metrics.total_commits) * 100.0
    );
    println!(
        "加班天数：{} / {} 天",
        metrics.overtime_days, metrics.commit_days
    );
    println!("最长连续工作天数：{} 天", metrics.longest_streak_days);

    if let Some(day) = &metrics.busiest_day {
        println!(
            "最忙的一天：{} -> {} 次提交（{} 次下班后）",
            day.date, day.total_commits, day.after_hours_commits
        );
    }

    if !metrics.top_after_hours_authors.is_empty() {
        println!("\n夜猫子榜单：");
        for author in &metrics.top_after_hours_authors {
            println!(
                "  - {} -> {} 次提交 | {:.1}% 下班后 | {} 次周末 | {} 次深夜",
                author.name,
                author.total_commits,
                author.after_hours_ratio * 100.0,
                author.weekend_commits,
                author.night_commits
            );
        }
    }

    if !metrics.chill_authors.is_empty() {
        println!("\n摸鱼榜单：");
        for author in &metrics.chill_authors {
            println!(
                "  - {} -> {} 次提交 | {:.1}% 下班后 | {} 次周末 | {} 次深夜",
                author.name,
                author.total_commits,
                author.after_hours_ratio * 100.0,
                author.weekend_commits,
                author.night_commits
            );
        }
    }
}

fn format_timestamp(value: &DateTime<FixedOffset>) -> String {
    value.format("%Y-%m-%d %H:%M").to_string()
}
