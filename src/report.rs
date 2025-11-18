use chrono::{DateTime, FixedOffset};

use crate::cli::Cli;
use crate::metrics::{percentage, RepoMetrics};

pub fn print_human_report(metrics: &RepoMetrics, cli: &Cli) {
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
    if !metrics.alias_rules.is_empty() {
        let pairs: Vec<String> = metrics
            .alias_rules
            .iter()
            .map(|rule| format!("{}=>{}", rule.from, rule.to))
            .collect();
        println!("别名合并：{}", pairs.join(", "));
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
