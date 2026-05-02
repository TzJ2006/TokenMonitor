//! Shared rate-limit payload helpers.
//!
//! Pre-rewrite this module also held HTTP response parsing for the Anthropic
//! `/usage_report` API. That API call is gone now; what's left is the small
//! amount of payload-shape logic that both the Claude (statusline) and Codex
//! (JSONL session file) paths still need: building a "fresh-but-failed"
//! placeholder and merging it with cached data.

use crate::models::ProviderRateLimits;
use chrono::Local;

use super::RateLimitFetchError;

/// Build a `ProviderRateLimits` whose only meaningful field is the error
/// message. Used by both providers when their fetch fails — the frontend
/// then merges this against cached windows so the user keeps seeing the
/// last-known utilization while the error banner explains why the data
/// stopped updating.
pub(super) fn provider_rate_limit_error(
    provider: &str,
    error: RateLimitFetchError,
) -> ProviderRateLimits {
    ProviderRateLimits {
        provider: provider.to_string(),
        plan_tier: None,
        windows: vec![],
        extra_usage: None,
        stale: false,
        error: Some(error.message),
        retry_after_seconds: error.retry_after_seconds,
        cooldown_until: error.cooldown_until,
        fetched_at: Local::now().to_rfc3339(),
    }
}

/// Combine a fresh fetch with the previous cache. When the fresh result is
/// an error-only payload (no windows), keep the cached windows but adopt
/// the fresh error metadata so the UI shows "stale + reason" rather than
/// flickering to empty. Otherwise the fresh payload wins.
pub(super) fn merge_provider_rate_limits(
    fresh: Option<ProviderRateLimits>,
    cached: Option<ProviderRateLimits>,
) -> Option<ProviderRateLimits> {
    match (fresh, cached) {
        (Some(fresh), Some(cached))
            if fresh.windows.is_empty() && fresh.error.is_some() && !cached.windows.is_empty() =>
        {
            Some(ProviderRateLimits {
                stale: true,
                error: fresh.error,
                retry_after_seconds: fresh.retry_after_seconds,
                cooldown_until: fresh.cooldown_until,
                fetched_at: fresh.fetched_at,
                ..cached
            })
        }
        (Some(fresh), _) => Some(fresh),
        (None, cached) => cached,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::RateLimitWindow;

    fn provider_rate_limits(
        windows: Vec<RateLimitWindow>,
        error: Option<&str>,
    ) -> ProviderRateLimits {
        ProviderRateLimits {
            provider: "claude".to_string(),
            plan_tier: Some("Pro".to_string()),
            windows,
            extra_usage: None,
            stale: false,
            error: error.map(ToString::to_string),
            retry_after_seconds: None,
            cooldown_until: None,
            fetched_at: "2026-03-17T12:00:00Z".to_string(),
        }
    }

    #[test]
    fn merges_cached_windows_with_fresh_error_metadata() {
        let cached = provider_rate_limits(
            vec![RateLimitWindow {
                window_id: "five_hour".to_string(),
                label: "Session (5hr)".to_string(),
                utilization: 33.0,
                resets_at: Some("2026-03-17T14:00:00Z".to_string()),
            }],
            None,
        );
        let mut fresh_error = provider_rate_limits(vec![], Some("statusline source missing"));
        fresh_error.fetched_at = "2026-03-17T12:04:00Z".to_string();

        let merged = merge_provider_rate_limits(Some(fresh_error), Some(cached)).unwrap();

        assert!(merged.stale);
        assert_eq!(merged.windows.len(), 1);
        assert_eq!(merged.error.as_deref(), Some("statusline source missing"));
        assert_eq!(merged.fetched_at, "2026-03-17T12:04:00Z");
    }

    #[test]
    fn fresh_wins_when_it_has_windows() {
        let cached = provider_rate_limits(
            vec![RateLimitWindow {
                window_id: "five_hour".into(),
                label: "Session".into(),
                utilization: 10.0,
                resets_at: None,
            }],
            None,
        );
        let fresh = provider_rate_limits(
            vec![RateLimitWindow {
                window_id: "five_hour".into(),
                label: "Session".into(),
                utilization: 50.0,
                resets_at: None,
            }],
            None,
        );
        let merged = merge_provider_rate_limits(Some(fresh), Some(cached)).unwrap();
        assert_eq!(merged.windows[0].utilization, 50.0);
    }
}
