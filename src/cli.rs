use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "cow-horse",
    about = "根据 Git 提交历史衡量一个仓库的“牛马”程度。"
)]
pub struct Cli {
    /// Path to the git repository to inspect
    #[arg(long, default_value = ".", value_name = "PATH")]
    pub path: PathBuf,

    /// Only include commits after this instant (e.g. 2023-01-01 or 30d for 30 days ago)
    #[arg(long, value_name = "SINCE")]
    pub since: Option<String>,

    /// Only include commits before this instant
    #[arg(long, value_name = "UNTIL")]
    pub until: Option<String>,

    /// Default rolling window (in days) when --since is omitted
    #[arg(long, default_value_t = 90, value_name = "DAYS")]
    pub window_days: u32,

    /// Filter commits by author substring (passed through to git)
    #[arg(long, value_name = "AUTHOR")]
    pub author: Option<String>,

    /// Limit the number of commits to read (useful for massive histories)
    #[arg(long, value_name = "COMMITS")]
    pub limit: Option<usize>,

    /// Output JSON instead of the human summary
    #[arg(long)]
    pub json: bool,

    /// Authors to drop from the stats (can repeat)
    #[arg(long = "ignore-author", value_name = "AUTHOR")]
    pub ignore_author: Vec<String>,

    /// Merge多个作者名称：格式为“旧名=统一名”，可重复
    #[arg(long = "alias", value_name = "A=B")]
    pub alias: Vec<String>,

    /// 快速转换 Unix 时间戳为可读时间（优先执行该操作）
    #[arg(long = "unix", value_name = "TIMESTAMP")]
    pub unix: Option<i64>,
}
