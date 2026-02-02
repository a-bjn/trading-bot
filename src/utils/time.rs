use chrono::{DateTime, Utc};

pub fn get_current_timestamp() -> DateTime<Utc> {
    Utc::now()
}

pub fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, secs)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}

pub fn timestamp_to_seconds(timestamp: i64) -> u64 {
    timestamp as u64
}
