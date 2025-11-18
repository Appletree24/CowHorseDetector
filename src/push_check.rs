use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use clap::Parser;
use serde::{Deserialize, Serialize};

const DEFAULT_THRESHOLD_MINUTES: u64 = 60;
const STATE_FILE: &str = "push_check.json";

#[derive(Parser, Debug)]
#[command(about = "在 git push 前提醒你起身喝水", name = "push-check")]
pub struct PushCheckCli {
    /// 超过多少分钟视为久坐，需要提醒喝水
    #[arg(long = "threshold", short = 't', default_value_t = DEFAULT_THRESHOLD_MINUTES, value_name = "MINUTES")]
    pub threshold_minutes: u64,

    /// 静默模式：只有需要提醒时才输出
    #[arg(long, default_value_t = false)]
    pub quiet: bool,
}

#[derive(Serialize, Deserialize)]
struct PushCheckState {
    last_push: DateTime<Utc>,
}

pub fn run_push_check(args: &PushCheckCli) -> Result<()> {
    let path = state_file_path()?;
    let now = Utc::now();
    let last_push = read_last_push(&path)?;

    if let Some(last) = last_push {
        let diff = now - last;
        let threshold = Duration::minutes(args.threshold_minutes as i64);
        if diff >= threshold {
            println!(
                "距离上一次 git push 已经过了 {} 分钟，出去走走喝杯水再回来继续吧！",
                diff.num_minutes()
            );
        } else if !args.quiet {
            println!(
                "距上次 push 仅 {} 分钟（提醒阈值 {} 分钟）。继续保持，但别忘了补水~",
                diff.num_minutes(),
                args.threshold_minutes
            );
        }
    } else if !args.quiet {
        println!("第一次记录 push，完成后我会提醒你注意休息。" );
    }

    write_last_push(&path, now)?;
    Ok(())
}

fn state_file_path() -> Result<PathBuf> {
    let mut dir = dirs::config_dir()
        .or_else(dirs::data_dir)
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    dir.push("zzh");
    fs::create_dir_all(&dir).with_context(|| format!("无法创建配置目录：{}", dir.display()))?;
    dir.push(STATE_FILE);
    Ok(dir)
}

fn read_last_push(path: &Path) -> Result<Option<DateTime<Utc>>> {
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(path)
        .with_context(|| format!("无法读取 push 记录：{}", path.display()))?;
    let state: PushCheckState = serde_json::from_str(&data)
        .with_context(|| format!("push 记录损坏：{}", path.display()))?;
    Ok(Some(state.last_push))
}

fn write_last_push(path: &Path, timestamp: DateTime<Utc>) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("无法创建目录：{}", parent.display()))?;
    }
    let state = PushCheckState { last_push: timestamp };
    let payload = serde_json::to_string(&state)?;
    fs::write(path, payload)
        .with_context(|| format!("无法写入 push 记录：{}", path.display()))?;
    Ok(())
}
