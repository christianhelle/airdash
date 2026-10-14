//! Human-readable times and file names.

use chrono::{Local, TimeZone};

/// "just now", "5 min ago", "3 h ago", "2 days ago" relative to `now` (epoch seconds).
pub fn time_ago(epoch: i64, now: i64) -> String {
    let seconds = now - epoch;
    if seconds < 60 {
        return "just now".into();
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes} min ago");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours} h ago");
    }
    let days = hours / 24;
    format!("{days} day{} ago", if days == 1 { "" } else { "s" })
}

/// "in 4 min" / "now" for an upcoming time.
pub fn time_until(epoch: i64, now: i64) -> String {
    let minutes = ((epoch - now) as f64 / 60.0).round() as i64;
    if minutes <= 0 {
        "now".into()
    } else if minutes < 60 {
        format!("in {minutes} min")
    } else {
        format!("in {} h", (minutes as f64 / 60.0).round() as i64)
    }
}

/// "Oct 14, 10:05" in local time.
pub fn format_date_time(epoch: i64) -> String {
    Local
        .timestamp_opt(epoch, 0)
        .single()
        .map(|t| t.format("%b %-d, %H:%M").to_string())
        .unwrap_or_default()
}

/// A file name that is safe on every OS.
pub fn safe_file_name(name: &str) -> String {
    let mut out = String::new();
    let mut in_invalid = false;
    for c in name.chars() {
        if matches!(c, '\\' |'/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
            if !in_invalid {
                out.push('-');
            }
            in_invalid = true;
        } else {
            out.push(c);
            in_invalid = false;
        }
    }
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if out.is_empty() { "airdash".into() } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_791_979_200; // 2026-10-14T12:00:00Z

    #[test]
    fn formats_past_times() {
        assert_eq!(time_ago(NOW - 10, NOW), "just now");
        assert_eq!(time_ago(NOW - 5 * 60, NOW), "5 min ago");
        assert_eq!(time_ago(NOW - 3 * 3600, NOW), "3 h ago");
        assert_eq!(time_ago(NOW - 24 * 3600, NOW), "1 day ago");
        assert_eq!(time_ago(NOW - 3 * 24 * 3600, NOW), "3 days ago");
    }

    #[test]
    fn formats_upcoming_times() {
        assert_eq!(time_until(NOW - 1, NOW), "now");
        assert_eq!(time_until(NOW + 4 * 60, NOW), "in 4 min");
        assert_eq!(time_until(NOW + 2 * 3600, NOW), "in 2 h");
    }

    #[test]
    fn strips_characters_that_are_invalid_in_file_names() {
        assert_eq!(safe_file_name("Living room: \"main\""), "Living room- -main-");
        assert_eq!(safe_file_name("  "), "airdash");
    }
}
