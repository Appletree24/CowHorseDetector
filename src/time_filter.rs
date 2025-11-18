use anyhow::{anyhow, bail, Result};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};

pub fn parse_time_filter(value: &str, now: DateTime<Utc>) -> Result<DateTime<Utc>> {
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
