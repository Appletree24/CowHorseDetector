use anyhow::{anyhow, Result};
use chrono::{DateTime, Local, Utc};

pub struct UnixConversion {
    pub timestamp: i64,
    pub utc: DateTime<Utc>,
    pub local: DateTime<Local>,
}

pub fn convert_unix_timestamp(timestamp: i64) -> Result<UnixConversion> {
    let utc = DateTime::<Utc>::from_timestamp(timestamp, 0)
        .ok_or_else(|| anyhow!("非法的 Unix 时间戳：{timestamp}"))?;
    let local = utc.with_timezone(&Local);
    Ok(UnixConversion {
        timestamp,
        utc,
        local,
    })
}
