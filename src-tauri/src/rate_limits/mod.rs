mod claude;
mod codex;
mod http;

use crate::models::RateLimitsPayload;
use crate::statusline::windows::ClaudePlanTier;
use crate::usage::parser::UsageParser;
use std::path::PathBuf;
use std::sync::Arc;

use claude::fetch_claude_rate_limits;
use codex::extract_codex_rate_limits;
use http::{merge_provider_rate_limits, provider_rate_limit_error};

#[derive(Debug, Clone)]
pub(crate) struct RateLimitFetchError {
    pub(crate) message: String,
    pub(crate) retry_after_seconds: Option<u64>,
    pub(crate) cooldown_until: Option<String>,
}

impl RateLimitFetchError {
    fn message(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retry_after_seconds: None,
            cooldown_until: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitSelection {
    All,
    Claude,
    Codex,
}

impl RateLimitSelection {
    pub fn includes_claude(self) -> bool {
        matches!(self, Self::All | Self::Claude)
    }

    pub fn includes_codex(self) -> bool {
        matches!(self, Self::All | Self::Codex)
    }
}

pub fn merge_rate_limits(
    fresh: RateLimitsPayload,
    cached: Option<&RateLimitsPayload>,
) -> RateLimitsPayload {
    RateLimitsPayload {
        claude: merge_provider_rate_limits(
            fresh.claude,
            cached.and_then(|payload| payload.claude.clone()),
        ),
        codex: merge_provider_rate_limits(
            fresh.codex,
            cached.and_then(|payload| payload.codex.clone()),
        ),
    }
}

/// Compute Claude + Codex rate-limit payloads concurrently. Both branches
/// are fully local (statusline event file for Claude, JSONL scrape for
/// Codex), so we run them on the blocking thread pool — IO overhead is in
/// the millisecond range and there are no network calls.
pub async fn fetch_selected_rate_limits(
    parser: Arc<UsageParser>,
    codex_dir: PathBuf,
    plan: ClaudePlanTier,
    selection: RateLimitSelection,
    cached: Option<&RateLimitsPayload>,
) -> RateLimitsPayload {
    let cached_claude = cached.and_then(|payload| payload.claude.clone());
    let cached_codex = cached.and_then(|payload| payload.codex.clone());

    let claude_future = async {
        if !selection.includes_claude() {
            return cached_claude;
        }
        let parser = Arc::clone(&parser);
        match tokio::task::spawn_blocking(move || fetch_claude_rate_limits(&parser, plan)).await {
            Ok(rl) => Some(rl),
            Err(e) => Some(provider_rate_limit_error(
                "claude",
                RateLimitFetchError::message(format!("Task failed: {e}")),
            )),
        }
    };

    let codex_future = async move {
        if !selection.includes_codex() {
            return cached_codex;
        }
        match tokio::task::spawn_blocking(move || extract_codex_rate_limits(&codex_dir)).await {
            Ok(Ok(rate_limits)) => Some(rate_limits),
            Ok(Err(error)) => Some(provider_rate_limit_error(
                "codex",
                RateLimitFetchError::message(error),
            )),
            Err(error) => Some(provider_rate_limit_error(
                "codex",
                RateLimitFetchError::message(format!("Task failed: {error}")),
            )),
        }
    };

    let (claude, codex) = tokio::join!(claude_future, codex_future);
    RateLimitsPayload { claude, codex }
}
