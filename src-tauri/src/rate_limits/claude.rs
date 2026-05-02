//! Claude rate-limit assembly from local data only.
//!
//! Replaces the prior OAuth + Keychain + CLI-probe pipeline. The display
//! percentage **always comes from CC's statusline payload** when one is
//! present — that field is server-authoritative and Anthropic computes it
//! against the user's actual plan, not our local plan-tier guess.
//!
//! The local JSONL rolling-window math in `statusline::windows` is used for
//! two narrow purposes only:
//!   1. As the *percentage* source when the payload has no `rate_limits`
//!      block at all (CC versions older than 2.x).
//!   2. As a `resets_at` source when the payload's `resets_at` has already
//!      rolled into the past, so the UI's reset countdown stays accurate.
//!
//! Earlier revisions also fell back to the JSONL pct when payload's
//! `resets_at` expired. That was wrong: the JSONL math depends entirely on
//! the user's `claudePlanTier` setting, which defaults to "Pro" (200K/5h)
//! and is frequently mis-set on Max 5x/20x accounts. The fallback caused
//! the displayed % to swing from ~10% (server) to 388%+ (Pro budget vs.
//! Max-20x usage) between refreshes — confusing users with inconsistent
//! reads. Trust the server number; only the timer needs local backup.
//!
//! We never make a network request and never read a credential.

use chrono::{DateTime, Duration, Utc};

use crate::models::{ProviderRateLimits, RateLimitWindow};
use crate::statusline::{
    events_file, source,
    source::StatuslineWindow,
    windows::{self, ClaudePlanTier, WindowSummary},
};
use crate::usage::parser::UsageParser;

/// How recently CC must have fired its statusline hook for the data to count
/// as "live". Pre-fix this was 30 min; bumped to 6h because CC sessions can
/// stay quiet for long stretches without that meaning the cached numbers
/// are wrong — Anthropic's window itself is 5h, so anything within 6h is
/// still load-bearing for the current display.
const FRESHNESS_WINDOW_HOURS: i64 = 6;

pub fn fetch_claude_rate_limits(parser: &UsageParser, plan: ClaudePlanTier) -> ProviderRateLimits {
    let now = Utc::now();
    let now_unix = now.timestamp();
    let session = source::latest_active_session(&events_file()).ok().flatten();

    // Always compute the local rolling-window summaries. CC's statusline
    // payload echoes its last cached `rate_limits` until a new API call
    // refreshes them — which doesn't happen between sessions until the user
    // sends their first prompt. Without a local fallback ready, a new
    // session lands on a stale `resets_at` and the UI gets stuck displaying
    // "Resetting..." with the previous session's percentage. 8-day overshoot
    // vs. the 7-day window keeps us robust against clock skew on SSH-synced
    // hosts.
    let since = (now - Duration::days(8)).date_naive();
    let (entries, _, _) = parser.load_entries("claude", Some(since));
    let (jsonl_five_hour, jsonl_weekly) = windows::compute(&entries, plan, now);

    let payload_five_hour = session.as_ref().and_then(|s| s.five_hour);
    let payload_seven_day = session.as_ref().and_then(|s| s.seven_day);

    let five_hour = build_window(
        "five_hour",
        "Session (5h)",
        payload_five_hour,
        &jsonl_five_hour,
        now_unix,
    );
    let weekly = build_window(
        "weekly",
        "Weekly",
        payload_seven_day,
        &jsonl_weekly,
        now_unix,
    );

    // Surface `plan_tier` only when we had to use the local pct because the
    // payload was missing a window entirely. When payload supplies the pct
    // (the common case), Anthropic already calibrated against the user's
    // real plan and showing our local guess label would be misleading.
    let used_local = payload_five_hour.is_none() || payload_seven_day.is_none();

    let stale = match session.as_ref() {
        Some(s) => !s.is_fresh(Duration::hours(FRESHNESS_WINDOW_HOURS), now),
        None => true,
    };

    ProviderRateLimits {
        provider: "claude".into(),
        plan_tier: if used_local {
            Some(plan.label().into())
        } else {
            None
        },
        windows: vec![five_hour, weekly],
        extra_usage: None,
        stale,
        error: None,
        retry_after_seconds: None,
        cooldown_until: None,
        fetched_at: now.to_rfc3339(),
    }
}

/// Build a single window. The displayed percentage prefers the payload's
/// server-authoritative `used_percentage` whenever a payload is present —
/// even if its `resets_at` has rolled into the past. The reset timestamp
/// independently prefers payload-then-JSONL so the countdown stays correct
/// after a window rolls over.
///
/// Concretely:
/// - payload present, `resets_at` future: use payload's pct + payload's reset.
/// - payload present, `resets_at` past:  use payload's pct + JSONL anchor's
///   reset (CC refreshes the payload on its next prompt).
/// - payload absent:                     use JSONL pct + JSONL reset.
fn build_window(
    window_id: &str,
    label: &str,
    payload: Option<StatuslineWindow>,
    jsonl: &WindowSummary,
    now_unix: i64,
) -> RateLimitWindow {
    let (utilization_pct, used_payload_pct) = match payload {
        Some(w) => (w.used_percentage, true),
        None => (jsonl.utilization_pct, false),
    };
    let resets_at_iso = match payload {
        Some(w) if w.resets_at_unix > now_unix => {
            DateTime::from_timestamp(w.resets_at_unix, 0).map(|t| t.to_rfc3339())
        }
        _ => jsonl.resets_at.map(|t| t.to_rfc3339()),
    };
    tracing::debug!(
        window = window_id,
        utilization_pct,
        from_payload = used_payload_pct,
        has_payload_reset = matches!(payload, Some(w) if w.resets_at_unix > now_unix),
        jsonl_pct = jsonl.utilization_pct,
        "claude rate-limit window built"
    );
    RateLimitWindow::new(
        window_id.into(),
        label.into(),
        utilization_pct,
        resets_at_iso,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jsonl_summary(pct: f64, resets_at: Option<DateTime<Utc>>) -> WindowSummary {
        WindowSummary {
            used_tokens: 0,
            budget_tokens: 0,
            utilization_pct: pct,
            anchor_at: None,
            resets_at,
        }
    }

    #[test]
    fn fresh_payload_wins_over_jsonl() {
        let now: DateTime<Utc> = "2026-04-29T12:00:00Z".parse().unwrap();
        let now_unix = now.timestamp();
        let payload = Some(StatuslineWindow {
            used_percentage: 42.0,
            resets_at_unix: now_unix + 3_600, // 1h in the future
        });
        let jsonl = jsonl_summary(7.0, Some(now + Duration::hours(2)));

        let w = build_window("five_hour", "Session (5h)", payload, &jsonl, now_unix);
        assert_eq!(w.utilization, 42.0);
    }

    #[test]
    fn expired_payload_keeps_pct_uses_jsonl_reset() {
        // CC's statusline echoes a `resets_at` that has already rolled over.
        // The pct is still server-authoritative — the previous attempt to
        // swap in JSONL math here caused a 388%-vs-10% jump when the
        // user's local plan-tier setting didn't match their real plan.
        // Now we keep the payload pct and only adopt the JSONL anchor for
        // the reset countdown.
        let now: DateTime<Utc> = "2026-04-29T12:00:00Z".parse().unwrap();
        let now_unix = now.timestamp();
        let payload = Some(StatuslineWindow {
            used_percentage: 88.0,
            resets_at_unix: now_unix - 60, // 1 min in the past
        });
        let fresh_reset = now + Duration::hours(4);
        let jsonl = jsonl_summary(388.0, Some(fresh_reset)); // wildly different — must not leak

        let w = build_window("five_hour", "Session (5h)", payload, &jsonl, now_unix);
        assert_eq!(
            w.utilization, 88.0,
            "payload pct must win, not the 388% local guess"
        );
        assert_eq!(
            w.resets_at.as_deref(),
            Some(fresh_reset.to_rfc3339()).as_deref()
        );
    }

    #[test]
    fn missing_payload_uses_jsonl() {
        let now: DateTime<Utc> = "2026-04-29T12:00:00Z".parse().unwrap();
        let now_unix = now.timestamp();
        let jsonl = jsonl_summary(15.0, None);

        let w = build_window("weekly", "Weekly", None, &jsonl, now_unix);
        assert_eq!(w.utilization, 15.0);
        assert!(w.resets_at.is_none());
    }
}
