use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, FixedOffset, Utc};

#[derive(Debug)]
pub struct Commit {
    pub author: String,
    pub timestamp: DateTime<FixedOffset>,
}

pub fn fetch_commits(
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
