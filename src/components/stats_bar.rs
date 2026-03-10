/// Summary statistics bar rendered above the map.
///
/// Displays total spots, unique callsigns, unique grids, and the time range
/// of the data currently in view.
use leptos::prelude::*;

use crate::models::SpotStats;

#[component]
pub fn StatsBar(
    /// Current aggregate statistics for the displayed data.
    stats: Signal<Option<SpotStats>>,
) -> impl IntoView {
    view! {
        <div id="stats-bar">
            {move || match stats.get() {
                None => view! {
                    <span class="stat-item stat-loading">"Loading statistics..."</span>
                }.into_any(),
                Some(s) => view! {
                    <span class="stat-item">
                        <span class="stat-label">"Spots"</span>
                        <span class="stat-value">{format_count(s.total_spots)}</span>
                    </span>
                    <span class="stat-sep">"|"</span>
                    <span class="stat-item">
                        <span class="stat-label">"Callsigns"</span>
                        <span class="stat-value">{format_count(s.unique_callsigns)}</span>
                    </span>
                    <span class="stat-sep">"|"</span>
                    <span class="stat-item">
                        <span class="stat-label">"Grids"</span>
                        <span class="stat-value">{format_count(s.unique_grids)}</span>
                    </span>
                    <span class="stat-sep">"|"</span>
                    <span class="stat-item">
                        <span class="stat-label">"Window"</span>
                        <span class="stat-value mono">
                            {format_time_range(s.oldest_unix, s.newest_unix)}
                        </span>
                    </span>
                }.into_any(),
            }}
        </div>
    }
}

/// Format a large integer count with thousands separators.
fn format_count(n: u64) -> String {
    // Insert underscores every three digits from the right.
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Format a Unix-timestamp range as `HH:MM–HH:MM UTC` (same-day) or
/// `MM-DD HH:MM–MM-DD HH:MM UTC` (cross-day).
fn format_time_range(oldest: i64, newest: i64) -> String {
    use chrono::{DateTime, Utc};

    if oldest == 0 && newest == 0 {
        return "—".to_string();
    }

    let fmt_short = |ts: i64| -> String {
        DateTime::<Utc>::from_timestamp(ts, 0)
            .map(|dt| dt.format("%H:%MZ").to_string())
            .unwrap_or_else(|| "?".to_string())
    };

    format!("{} – {}", fmt_short(oldest), fmt_short(newest))
}
