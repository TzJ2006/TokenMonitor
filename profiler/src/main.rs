//! Quantify the per-refresh CPU cost before/after the invalidation fix, against
//! the real Claude/Codex logs on this machine. Read-only: never mutates logs.
//!
//! "OLD per-append cost" = what the 2s poll forced on every file append before
//! the fix: clear_cache() then recompute earliest-date + aggregate from cold.
//! "NEW per-append cost" = what an append costs now: caches kept warm, so
//! has_entries_before is O(1) and the listing is reused. The only unavoidable
//! per-tick cost left is the change-detection stat sweep.

#[path = "../../src-tauri/src/models.rs"]
mod models;
#[path = "../../src-tauri/src/paths.rs"]
mod paths;
#[path = "../../src-tauri/src/stats/mod.rs"]
mod stats;
mod secrets;
#[path = "commands_mod.rs"]
mod commands;
#[path = "usage_mod.rs"]
mod usage;

use std::time::Instant;

use chrono::{Local, NaiveDate};

use crate::usage::parser::UsageParser;

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn main() {
    let parser = UsageParser::new();
    let before = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    let today = Local::now().format("%Y%m%d").to_string();

    // One untimed warmup so the OS file cache is hot and the parser caches are
    // populated — we are measuring CPU work, not cold-disk I/O.
    parser.get_daily("all", &today);
    let _ = parser.has_entries_before("all", before);

    println!("provider=all  before={before}  today={today}");
    println!(
        "{:<34} {:>10} {:>10} {:>10}",
        "scenario (ms)", "run1", "run2", "run3"
    );

    // ── OLD per-append cost: cold earliest-date recompute (re-parse-all). ──
    let mut old_earliest = Vec::new();
    for _ in 0..3 {
        parser.clear_cache();
        let t = Instant::now();
        let _ = parser.has_entries_before("all", before);
        old_earliest.push(ms(t.elapsed()));
    }
    row("OLD append: earliest (cold)", &old_earliest);

    // ── NEW per-append cost: earliest stays warm after invalidate. ──
    let mut new_earliest = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        let _ = parser.has_entries_before("all", before);
        new_earliest.push(ms(t.elapsed()));
    }
    row("NEW append: earliest (warm)", &new_earliest);

    // ── OLD per-append cost: cold day aggregate (tree re-walk + reparse). ──
    let mut old_agg = Vec::new();
    for _ in 0..3 {
        parser.clear_cache();
        let t = Instant::now();
        parser.get_daily("all", &today);
        old_agg.push(ms(t.elapsed()));
    }
    row("OLD append: day aggregate (cold)", &old_agg);

    // ── NEW per-append cost: warm day aggregate (listing + file cache kept). ──
    let mut new_agg = Vec::new();
    for _ in 0..3 {
        parser.clear_entries_cache();
        let t = Instant::now();
        parser.get_daily("all", &today);
        new_agg.push(ms(t.elapsed()));
    }
    row("NEW append: day aggregate (warm)", &new_agg);

    // ── Remaining unavoidable per-2s-tick cost: the change-detection sweep. ──
    // With nothing changed this stats every cached file and returns false.
    let mut sweep = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        let changed = parser.invalidate_if_changed();
        sweep.push(ms(t.elapsed()));
        let _ = changed;
    }
    row("per-tick: detection sweep (idle)", &sweep);
}

fn row(label: &str, xs: &[f64]) {
    let g = |i: usize| xs.get(i).copied().unwrap_or(f64::NAN);
    println!("{label:<34} {:>10.1} {:>10.1} {:>10.1}", g(0), g(1), g(2));
}
