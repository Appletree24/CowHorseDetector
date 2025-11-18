use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, Timelike, Weekday};
use serde::Serialize;

use crate::gitlog::Commit;

#[derive(Debug, Serialize)]
pub struct RepoMetrics {
    pub repo_path: PathBuf,
    pub analysis_start: Option<DateTime<FixedOffset>>,
    pub analysis_end: Option<DateTime<FixedOffset>>,
    pub total_commits: usize,
    pub unique_authors: usize,
    pub after_hours_commits: usize,
    pub weekend_commits: usize,
    pub night_commits: usize,
    pub commit_days: usize,
    pub overtime_days: usize,
    pub longest_streak_days: usize,
    pub busiest_day: Option<BusiestDay>,
    pub severity_score: f64,
    pub severity_label: String,
    pub top_after_hours_authors: Vec<AuthorSummary>,
    pub chill_authors: Vec<AuthorSummary>,
    pub ignored_authors: Vec<String>,
    pub alias_rules: Vec<AliasRule>,
}

#[derive(Debug, Serialize, Clone)]
pub struct AliasRule {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct BusiestDay {
    pub date: NaiveDate,
    pub total_commits: usize,
    pub after_hours_commits: usize,
}

#[derive(Debug, Serialize, Clone)]
pub struct AuthorSummary {
    pub name: String,
    pub total_commits: usize,
    pub after_hours_commits: usize,
    pub weekend_commits: usize,
    pub night_commits: usize,
    pub after_hours_ratio: f64,
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

pub fn compute_metrics(
    repo_path: &Path,
    commits: &[Commit],
    ignored_authors: Vec<String>,
    alias_rules: Vec<AliasRule>,
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
        let is_after_hours = hour < 10 || hour >= 18;
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
        alias_rules,
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

pub fn percentage(part: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 / total as f64
    }
}
