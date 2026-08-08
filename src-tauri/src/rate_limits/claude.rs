use crate::models::{ExtraUsageInfo, ProviderRateLimits, RateLimitWindow};
use chrono::{DateTime, Local, Utc};
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;
use std::sync::Mutex;

use super::http::rate_limit_error_from_response;
use super::RateLimitFetchError;

const ANTHROPIC_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const ANTHROPIC_ACCOUNT_URL: &str = "https://api.anthropic.com/api/oauth/account";

/// In-process cache of the Claude OAuth access token.
///
/// Claude Code rewrites the `Claude Code-credentials` Keychain item each time
/// it rotates its OAuth token. That rewrite resets the item's ACL / partition
/// list, so the user's "Always Allow" grant for TokenMonitor is lost — and
/// without a cache the next background refresh (every ~2.5 min) re-prompts.
/// Caching lets us reuse the token across refresh cycles and only touch the
/// Keychain on a cold cache or when the API returns 401 (real rotation).
static CACHED_ACCESS_TOKEN: Mutex<Option<String>> = Mutex::new(None);

fn cached_access_token() -> Option<String> {
    CACHED_ACCESS_TOKEN.lock().ok().and_then(|g| g.clone())
}

fn store_access_token(token: &str) {
    if let Ok(mut guard) = CACHED_ACCESS_TOKEN.lock() {
        *guard = Some(token.to_string());
    }
}

fn invalidate_access_token_cache() {
    if let Ok(mut guard) = CACHED_ACCESS_TOKEN.lock() {
        *guard = None;
    }
}

/// Extract `claudeAiOauth.accessToken` from a JSON string.
fn extract_access_token(json_str: &str) -> Result<String, String> {
    let parsed: serde_json::Value =
        serde_json::from_str(json_str.trim()).map_err(|e| format!("Invalid JSON: {e}"))?;

    parsed
        .get("claudeAiOauth")
        .and_then(|o| o.get("accessToken"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "No claudeAiOauth.accessToken in credentials".to_string())
}

/// Extract `claudeAiOauth.refreshToken` from a JSON string. Used by the
/// OAuth refresh-grant flow to mint a fresh access token without prompting
/// the user. macOS-only because the refresh path is too — Linux/Windows
/// currently rely on `~/.claude/.credentials.json` instead.
#[cfg(target_os = "macos")]
fn extract_refresh_token(json_str: &str) -> Result<String, String> {
    let parsed: serde_json::Value =
        serde_json::from_str(json_str.trim()).map_err(|e| format!("Invalid JSON: {e}"))?;

    parsed
        .get("claudeAiOauth")
        .and_then(|o| o.get("refreshToken"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "No claudeAiOauth.refreshToken in credentials".to_string())
}

/// Patch the credentials JSON with a refreshed access token, optionally
/// rotating the refresh token + expiresAt as well. Preserves every other
/// field so the stored payload keeps its `subscriptionType`, `rateLimitTier`,
/// MCP OAuth state, etc. — important because the result is written back into
/// Claude Code's own item. macOS-only; the only caller is
/// [`try_refresh_claude_oauth`].
#[cfg(target_os = "macos")]
fn update_credentials_with_refresh(
    original_json: &str,
    new_access_token: &str,
    new_refresh_token: Option<&str>,
    expires_in_secs: Option<u64>,
) -> Result<String, String> {
    let mut parsed: serde_json::Value =
        serde_json::from_str(original_json.trim()).map_err(|e| format!("Invalid JSON: {e}"))?;

    let oauth = parsed
        .get_mut("claudeAiOauth")
        .ok_or_else(|| "Missing claudeAiOauth root".to_string())?
        .as_object_mut()
        .ok_or_else(|| "claudeAiOauth is not an object".to_string())?;

    oauth.insert(
        "accessToken".to_string(),
        serde_json::Value::String(new_access_token.to_string()),
    );
    if let Some(refresh) = new_refresh_token {
        oauth.insert(
            "refreshToken".to_string(),
            serde_json::Value::String(refresh.to_string()),
        );
    }
    if let Some(secs) = expires_in_secs {
        let expires_at_ms =
            (chrono::Utc::now().timestamp_millis() as u64) + secs.saturating_mul(1000);
        oauth.insert(
            "expiresAt".to_string(),
            serde_json::Value::Number(serde_json::Number::from(expires_at_ms)),
        );
    }

    serde_json::to_string(&parsed).map_err(|e| format!("Failed to serialize updated JSON: {e}"))
}

/// Retired: the Keychain item TokenMonitor used to mirror Claude Code's
/// credentials into. Kept only so [`purge_legacy_owned_keychain_item`] can
/// clean it off machines that ran an older build.
///
/// The mirror is gone because it could not work on this app. Its whole
/// premise was that `SecKeychainAddGenericPassword` records the *calling
/// binary's code-signing identity* in the item's ACL, so only TokenMonitor
/// could read it back. But TokenMonitor ships unsigned — `tauri.conf.json`
/// declares no `signingIdentity` — so every build is ad-hoc signed with a
/// cdhash-derived identity that changes on every rebuild *and every release*.
/// The ACL written by build N therefore rejects build N+1, and the app
/// auto-updates. Worse, `security_framework`'s `set_generic_password` is a
/// find-then-modify when the item already exists (decrypt + modify, both ACL
/// authorizations), so each subsequent write raised the modal "enter your
/// keychain password" panel — from the background refresh loop, with no user
/// gesture. Reading Claude Code's item through `/usr/bin/security` instead
/// (see [`read_raw_credentials_via_security_cli`]) is silent, survives token
/// rotation, and needs no grant at all, which leaves the mirror with nothing
/// to contribute.
#[cfg(target_os = "macos")]
const LEGACY_OWNED_KEYCHAIN_SERVICE: &str = "com.tokenmonitor.app.claude-oauth";
#[cfg(target_os = "macos")]
const LEGACY_OWNED_KEYCHAIN_ACCOUNT: &str = "default";

/// Remove the retired mirror item, once per process.
///
/// Left in place it is inert — nothing reads it any more — but it is a stale
/// copy of the user's OAuth credentials sitting in their login keychain, so
/// we clear it. Uses the unified `SecItemDelete`, which does not consult the
/// legacy ACL and so cannot raise a panel.
#[cfg(target_os = "macos")]
fn purge_legacy_owned_keychain_item() {
    use std::sync::Once;
    static PURGED: Once = Once::new();

    PURGED.call_once(|| {
        match security_framework::passwords::delete_generic_password(
            LEGACY_OWNED_KEYCHAIN_SERVICE,
            LEGACY_OWNED_KEYCHAIN_ACCOUNT,
        ) {
            Ok(()) => tracing::info!(
                service = LEGACY_OWNED_KEYCHAIN_SERVICE,
                "Removed retired TokenMonitor Keychain mirror"
            ),
            Err(e) => {
                // Absent is the steady state — only log anything else.
                let msg = format!("{e}");
                if !msg.contains("-25300") && !msg.to_ascii_lowercase().contains("not found") {
                    tracing::debug!(error = %msg, "Could not remove retired Keychain mirror");
                }
            }
        }
    });
}

/// Service name of the Keychain item Claude Code stores its OAuth
/// credentials under.
#[cfg(target_os = "macos")]
const CLAUDE_KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

/// Read Claude Code's credentials JSON by shelling out to `/usr/bin/security`.
///
/// This is the same mechanism Claude Code itself uses — it stores the item
/// with `security add-generic-password -U -a <user> -s "Claude Code-credentials"`
/// and reads it back with `security find-generic-password -a <user> -w -s ...`.
/// Because the item is *created* by that binary, the ACL's trusted app is
/// `/usr/bin/security`, not Claude Code. Any process that goes through the
/// same binary therefore reads it silently, and keeps doing so after Claude
/// Code rotates its token and resets the ACL.
///
/// Reading the identical item in-process through Security.framework
/// ([`read_token_from_keychain`]) fails with `errSecAuthFailed` (-25293)
/// instead, because TokenMonitor is not in that ACL and never can be — the
/// next rotation would drop the grant again. That's why this path exists.
///
/// Claude Code passes `-a <macOS username>`; we match that first and retry on
/// the service alone so a credentials item written under a different account
/// name still resolves.
#[cfg(target_os = "macos")]
fn read_raw_credentials_via_security_cli() -> Result<String, String> {
    use crate::platform::macos::keychain::find_generic_password;

    let account = std::env::var("USER").ok().filter(|u| !u.trim().is_empty());

    match find_generic_password(CLAUDE_KEYCHAIN_SERVICE, account.as_deref()) {
        Ok(raw) => Ok(raw),
        Err(with_account) if account.is_some() => {
            find_generic_password(CLAUDE_KEYCHAIN_SERVICE, None)
                .map_err(|without_account| format!("{with_account}; {without_account}"))
        }
        Err(err) => Err(err),
    }
}

#[cfg(target_os = "macos")]
fn read_token_via_security_cli() -> Result<String, String> {
    let raw = read_raw_credentials_via_security_cli()?;
    let token = extract_access_token(&raw)?;
    tracing::debug!(
        prefix = &token[..token.len().min(7)],
        "security CLI Keychain read succeeded"
    );
    Ok(token)
}

/// Write a credentials JSON back into Claude Code's own Keychain item.
///
/// Uses the exact command Claude Code uses to store it
/// (`security add-generic-password -U -a <user> -s "Claude Code-credentials" -w <json>`),
/// so the item keeps `/usr/bin/security` as its trusted app and Claude Code
/// goes on reading it normally.
///
/// **This write is mandatory after any refresh-grant performed against
/// credentials read from this item.** Anthropic rotates the *refresh* token
/// on every refresh — verified against a live account — so the copy Claude
/// Code holds is dead the moment our refresh succeeds. Without the write-back
/// the user is silently signed out of Claude Code the next time it rotates.
///
/// The payload goes through argv, briefly visible in `ps`. That is the same
/// exposure Claude Code itself accepts; `security` has no stdin path for `-w`.
#[cfg(target_os = "macos")]
fn write_credentials_to_claude_code_keychain(credentials_json: &str) -> Result<(), String> {
    let account = std::env::var("USER")
        .ok()
        .filter(|u| !u.trim().is_empty())
        .ok_or_else(|| "Cannot determine account name for Keychain write".to_string())?;

    // `upsert_generic_password` reads the item back before returning Ok, so a
    // silent no-op write cannot leave Claude Code holding a refresh token
    // Anthropic has already rotated away.
    crate::platform::macos::keychain::upsert_generic_password(
        CLAUDE_KEYCHAIN_SERVICE,
        &account,
        credentials_json,
    )?;

    tracing::info!("Refreshed Claude credentials written back to Claude Code's Keychain item");
    Ok(())
}

/// Read OAuth token from macOS Keychain via Security.framework.
///
/// Suppressing the Keychain prompt requires **two** mechanisms, because
/// macOS has two keychain stores with different UI-gating knobs:
///
/// 1. `skip_authenticated_items(true)` →
///    `kSecUseAuthenticationUI = kSecUseAuthenticationUISkip`. This governs
///    the **Data Protection keychain** (Touch ID / Face ID items). For
///    those, a would-prompt item is omitted from the result set.
/// 2. `SecKeychain::disable_user_interaction()` →
///    `SecKeychainSetUserInteractionAllowed(false)`. This is a process-wide
///    flag and is the **only** thing that suppresses the classic
///    "Always Allow / Allow / Deny" prompt produced by the **legacy
///    keychain** — which is what `Claude Code-credentials` lives in, since
///    Claude Code writes it through the legacy ACL path. Without this,
///    `kSecUseAuthenticationUISkip` is silently ignored and macOS still
///    pops the ACL panel whenever the machine is awake. (Log evidence for
///    this: reads that failed during dark wake reported "In dark wake, no
///    UI possible" — macOS only emits that after deciding UI was needed,
///    which means the UI-skip flag wasn't consulted.)
///
/// The UI-suppression flag is process-wide, so the search runs inside
/// [`with_ui_suppressed`], which also serialises against any other in-process
/// Keychain read — the crate's RAII guard restores the flag to *allowed* on
/// drop rather than to its previous value, so two overlapping reads would
/// otherwise unmask the panel this exists to suppress.
///
/// In practice this always fails with `errSecAuthFailed`: TokenMonitor is not
/// in the item's ACL and cannot be, because Claude Code resets the ACL on
/// every token rotation and this app's ad-hoc code identity changes on every
/// build. It is kept only as a last resort in case a future Claude Code build
/// stores its credentials somewhere `/usr/bin/security` cannot reach.
#[cfg(target_os = "macos")]
fn read_token_from_keychain() -> Result<String, String> {
    use crate::platform::macos::keychain::with_ui_suppressed;
    use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};

    let results = with_ui_suppressed(|| {
        ItemSearchOptions::new()
            .class(ItemClass::generic_password())
            .service(CLAUDE_KEYCHAIN_SERVICE)
            .load_data(true)
            .limit(1)
            .skip_authenticated_items(true)
            .search()
            .map_err(|e| format!("Claude Code credentials not available in Keychain: {e}"))
    })??;

    let data = results
        .into_iter()
        .find_map(|r| match r {
            SearchResult::Data(bytes) => Some(bytes),
            _ => None,
        })
        .ok_or_else(|| "Keychain returned no data for Claude Code-credentials".to_string())?;

    let raw = String::from_utf8(data).map_err(|e| format!("Invalid UTF-8 from Keychain: {e}"))?;
    extract_access_token(&raw)
}

/// Backs the "Allow Keychain access" button that the rate-limit empty state
/// still offers.
///
/// It no longer opens anything. The button existed to seed the owned mirror
/// via an ACL panel, and both the mirror and the panel are gone: Claude
/// Code's item is read through `/usr/bin/security`, which is already the
/// item's trusted app, so there is nothing for the user to grant. This now
/// just performs the ordinary silent read and reports whether a token came
/// back, so the frontend's granted/denied contract still holds while nothing
/// on the Claude credential path can raise a Keychain panel.
#[cfg(target_os = "macos")]
pub(super) fn prime_token_from_keychain_interactive() -> Result<(), String> {
    purge_legacy_owned_keychain_item();
    let token = get_claude_oauth_token()?;
    store_access_token(&token);
    Ok(())
}

fn read_token_from_credentials_path(cred_path: &Path) -> Result<String, String> {
    tracing::debug!(path = %cred_path.display(), "reading file (claude credentials)");
    let raw = std::fs::read_to_string(cred_path)
        .map_err(|e| format!("Failed to read {}: {e}", cred_path.display()))?;

    extract_access_token(&raw)
}

/// Read OAuth token from `~/.claude/.credentials.json`.
///
/// Newer Claude Code builds keep this file current on macOS as well as on
/// Windows/Linux. Prefer it over Keychain because it is a normal file read
/// from the same Claude config directory the app already discloses, so it
/// cannot trigger a macOS Keychain prompt during background refresh.
fn read_token_from_credentials_file() -> Result<String, String> {
    let cred_path = crate::paths::claude_credentials_file()
        .ok_or_else(|| "Cannot determine Claude credentials file path".to_string())?;
    read_token_from_credentials_path(&cred_path)
}

#[cfg(target_os = "macos")]
fn read_token_from_silent_platform_source(credentials_error: String) -> Result<String, String> {
    // `/usr/bin/security` before Security.framework: Claude Code writes the
    // item through that binary, so the CLI read is the one that actually
    // succeeds. The in-process read is kept as a last resort for the case
    // where a future Claude Code build stores the item some other way.
    let cli_error = match read_token_via_security_cli() {
        Ok(token) => return Ok(token),
        Err(err) => {
            tracing::debug!(error = %err, "security CLI Keychain read failed");
            err
        }
    };

    read_token_from_keychain().map_err(|keychain_error| {
        format!(
            "Claude credentials file unavailable ({credentials_error}); \
             security CLI read failed ({cli_error}); \
             Keychain unavailable ({keychain_error})"
        )
    })
}

#[cfg(not(target_os = "macos"))]
fn read_token_from_silent_platform_source(credentials_error: String) -> Result<String, String> {
    Err(credentials_error)
}

/// Get Claude Code OAuth access token (cross-platform).
///
/// Resolution order:
/// 1. `CLAUDE_CODE_OAUTH_TOKEN` environment variable (JSON string) — never cached
/// 2. In-process cache (set on previous successful read)
/// 3. `~/.claude/.credentials.json`
/// 4. macOS only: `/usr/bin/security find-generic-password` against Claude
///    Code's item. Silent and rotation-proof, because Claude Code creates
///    that item with the same binary — see
///    [`read_raw_credentials_via_security_cli`].
/// 5. macOS only: in-process Security.framework read of the same item
///    (last resort; normally denied with `errSecAuthFailed` because
///    TokenMonitor is not in the item's ACL).
///
/// Every step is silent: no step can raise a macOS Keychain panel. Steps 4
/// and 5 both target Claude Code's own item, so nothing here depends on a
/// user grant that Claude Code's next token rotation would wipe.
///
/// On a successful read the token is stored in the in-process cache. Callers
/// that observe a 401 from the API should call
/// [`invalidate_access_token_cache`] so the next call re-reads from a fresh
/// source instead of replaying the stale token.
pub(crate) fn get_claude_oauth_token() -> Result<String, String> {
    // Environment variable override (all platforms). Cheap to read each call,
    // and we don't want to cache an env value that the user might change.
    if let Ok(env_json) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN") {
        if !env_json.trim().is_empty() {
            return extract_access_token(&env_json);
        }
    }

    if let Some(cached) = cached_access_token() {
        return Ok(cached);
    }

    // Opportunistic one-shot cleanup: machines that ran an older build still
    // have the retired mirror item sitting in their login keychain.
    #[cfg(target_os = "macos")]
    purge_legacy_owned_keychain_item();

    let token =
        read_token_from_credentials_file().or_else(read_token_from_silent_platform_source)?;

    store_access_token(&token);
    Ok(token)
}

// ── Claude API response types ──

/// Known Claude usage windows, in dashboard display order.
///
/// Anthropic's OAuth usage payload and Claude Code's statusline `rate_limits`
/// object share these keys. Missing keys are omitted; any additional object
/// with a numeric `utilization` / `used_percentage` becomes a new bar via
/// [`claude_usage_windows`] / statusline extraction.
const KNOWN_CLAUDE_WINDOWS: &[(&str, &str)] = &[
    // (apiField, label)
    ("five_hour", "Session (5hr)"),
    ("seven_day", "Weekly (7 day)"),
    ("seven_day_sonnet", "Weekly Sonnet"),
    ("seven_day_opus", "Weekly Opus"),
    ("seven_day_fable", "Weekly Fable"),
    ("seven_day_overage_included", "Weekly Fable"),
    ("seven_day_oauth_apps", "Weekly OAuth Apps"),
    ("seven_day_cowork", "Weekly Cowork"),
];

#[derive(Deserialize)]
pub(crate) struct ClaudeExtraUsageData {
    pub is_enabled: bool,
    pub monthly_limit: f64,
    pub used_credits: f64,
    pub utilization: Option<f64>,
}

/// Display label for a Claude window id. Known ids get Anthropic-aligned
/// names; unknown ids are humanized from the field name so new pools surface
/// without a TokenMonitor release for *structure* changes.
pub(super) fn claude_window_label(window_id: &str) -> String {
    KNOWN_CLAUDE_WINDOWS
        .iter()
        .find(|(id, _)| *id == window_id)
        .map(|(_, label)| (*label).to_string())
        .unwrap_or_else(|| humanize_snake_case(window_id))
}

fn humanize_snake_case(field: &str) -> String {
    field
        .split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    let mut out = first.to_uppercase().collect::<String>();
                    out.push_str(&chars.as_str().to_lowercase());
                    out
                }
                None => part.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn claude_window_from_value(value: &Value) -> Option<(f64, Option<String>)> {
    // OAuth usage uses `utilization`; statusline uses `used_percentage`.
    let utilization = value
        .get("utilization")
        .and_then(as_f64)
        .or_else(|| value.get("used_percentage").and_then(as_f64))?;
    let resets_at = value.get("resets_at").and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => n
            .as_i64()
            .and_then(|ts| DateTime::<Utc>::from_timestamp(ts, 0))
            .map(|dt| dt.to_rfc3339()),
        _ => None,
    });
    Some((utilization, resets_at))
}

fn claude_scoped_weekly_window(usage: &Value) -> Option<RateLimitWindow> {
    let limits = usage.get("limits")?.as_array()?;
    let (_, utilization, limit) = limits
        .iter()
        .filter(|limit| {
            matches!(
                limit.get("kind").and_then(Value::as_str),
                Some("weekly_scoped" | "seven_day_overage_included")
            )
        })
        .filter_map(|limit| {
            let utilization = limit
                .get("percent")
                .and_then(as_f64)
                .or_else(|| limit.get("utilization").and_then(as_f64))?;
            Some((
                limit
                    .get("is_active")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                utilization,
                limit,
            ))
        })
        .max_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        })?;
    let model = limit
        .pointer("/scope/model/display_name")
        .and_then(Value::as_str)
        .or_else(|| limit.pointer("/scope/model/id").and_then(Value::as_str))?;
    let model_key = model
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_");
    let resets_at = limit.get("resets_at").and_then(|value| match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => n
            .as_i64()
            .and_then(|ts| DateTime::<Utc>::from_timestamp(ts, 0))
            .map(|dt| dt.to_rfc3339()),
        _ => None,
    });

    Some(RateLimitWindow::new(
        format!("seven_day_{model_key}"),
        format!("Weekly {model}"),
        utilization,
        resets_at,
    ))
}

/// Build rate-limit windows from whatever meters Claude/Anthropic returns.
///
/// Known fields keep stable display names; unknown window objects become
/// additional bars so count changes track the API without a release.
pub(super) fn claude_usage_windows(usage: &Value) -> Vec<RateLimitWindow> {
    let Some(obj) = usage.as_object() else {
        return Vec::new();
    };

    let mut windows = Vec::new();
    let mut consumed = std::collections::HashSet::new();
    consumed.insert("extra_usage");
    consumed.insert("limits");

    for (api_field, _) in KNOWN_CLAUDE_WINDOWS {
        consumed.insert(*api_field);
        let Some(value) = obj.get(*api_field) else {
            continue;
        };
        let Some((utilization, resets_at)) = claude_window_from_value(value) else {
            continue;
        };
        windows.push(RateLimitWindow::new(
            (*api_field).to_string(),
            claude_window_label(api_field),
            utilization,
            resets_at,
        ));
    }

    if let Some(scoped) = claude_scoped_weekly_window(usage) {
        if !windows
            .iter()
            .any(|window| window.window_id == scoped.window_id)
        {
            windows.push(scoped);
        }
    }

    let mut extras: Vec<(&String, f64, Option<String>)> = obj
        .iter()
        .filter(|(key, _)| !consumed.contains(key.as_str()))
        .filter_map(|(key, value)| {
            let (utilization, resets_at) = claude_window_from_value(value)?;
            Some((key, utilization, resets_at))
        })
        .collect();
    extras.sort_by(|a, b| a.0.cmp(b.0));

    for (field, utilization, resets_at) in extras {
        windows.push(RateLimitWindow::new(
            field.clone(),
            claude_window_label(field),
            utilization,
            resets_at,
        ));
    }

    windows
}

pub(crate) fn normalize_claude_extra_usage(extra_usage: ClaudeExtraUsageData) -> ExtraUsageInfo {
    ExtraUsageInfo {
        is_enabled: extra_usage.is_enabled,
        // The OAuth usage endpoint reports credit values in cents.
        monthly_limit: extra_usage.monthly_limit / 100.0,
        used_credits: extra_usage.used_credits / 100.0,
        utilization: extra_usage.utilization,
    }
}

#[derive(Deserialize)]
struct ClaudeAccountResponse {
    memberships: Vec<ClaudeMembership>,
}

#[derive(Deserialize)]
struct ClaudeMembership {
    organization: ClaudeOrganization,
}

#[derive(Deserialize)]
struct ClaudeOrganization {
    capabilities: Option<Vec<String>>,
    rate_limit_tier: Option<String>,
}

/// Outcome of one API attempt. We surface 401 separately so the outer
/// function can drop the cached token and retry with a fresh read.
enum FetchAttempt {
    Ok(ProviderRateLimits),
    Unauthorized(RateLimitFetchError),
    Other(RateLimitFetchError),
}

pub(super) async fn fetch_claude_rate_limits() -> Result<ProviderRateLimits, RateLimitFetchError> {
    match try_fetch_claude_rate_limits().await {
        FetchAttempt::Ok(rate_limits) => Ok(rate_limits),
        FetchAttempt::Other(err) => Err(err),
        FetchAttempt::Unauthorized(unauthorized_err) => {
            // Access token is stale — Claude Code's stored token lives ~8h,
            // so any overnight gap in Claude Code usage lands here. On macOS
            // we exchange the stored refresh token for a fresh pair and carry
            // on; only a *confirmed* revocation drops the credentials and
            // falls back to the interactive prompt path. On Linux/Windows we
            // have no refresh flow yet, so we just drop the in-mem cache and
            // retry against the source.
            #[cfg(target_os = "macos")]
            {
                match try_refresh_claude_oauth().await {
                    RefreshResult::Refreshed => {
                        tracing::info!("Claude OAuth: refresh succeeded; retrying API call");
                        return match try_fetch_claude_rate_limits().await {
                            FetchAttempt::Ok(rate_limits) => Ok(rate_limits),
                            FetchAttempt::Unauthorized(err) | FetchAttempt::Other(err) => Err(err),
                        };
                    }
                    RefreshResult::Revoked(reason) => {
                        // Claude Code has to sign in again; nothing we can
                        // recover from here beyond dropping the dead token.
                        tracing::warn!(reason = %reason,
                            "Claude OAuth: refresh token revoked");
                        invalidate_access_token_cache();
                    }
                    RefreshResult::Transient(reason) => {
                        tracing::warn!(reason = %reason,
                            "Claude OAuth: refresh transient failure");
                        invalidate_access_token_cache();
                        return Err(unauthorized_err);
                    }
                    RefreshResult::NoCredentials => {
                        // No refresh token to use — drop the in-mem cache and
                        // retry against the source in case Claude Code has
                        // rotated the item underneath us.
                        invalidate_access_token_cache();
                    }
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = unauthorized_err;
                invalidate_access_token_cache();
            }

            match try_fetch_claude_rate_limits().await {
                FetchAttempt::Ok(rate_limits) => Ok(rate_limits),
                FetchAttempt::Unauthorized(err) | FetchAttempt::Other(err) => Err(err),
            }
        }
    }
}

/// Outcome of an attempted refresh-grant.
#[cfg(target_os = "macos")]
#[derive(Debug)]
enum RefreshResult {
    /// Credentials were refreshed, persisted back to the store they came
    /// from, and the new access token is in the in-process cache, ready for
    /// the next API attempt.
    Refreshed,
    /// Anthropic rejected the refresh token — the user has to sign in
    /// through Claude Code again.
    Revoked(String),
    /// Network / 5xx / parse error. Stored credentials are left intact.
    Transient(String),
    /// No readable credentials with a refresh token.
    NoCredentials,
}

/// Test hook — exposed via `rate_limits::debug_force_refresh` so an IPC
/// can drive the refresh-grant flow without needing a real 401 from
/// Anthropic. Returns a one-line summary suitable for a log line / toast.
#[cfg(target_os = "macos")]
pub(super) async fn debug_force_refresh() -> String {
    match try_refresh_claude_oauth().await {
        RefreshResult::Refreshed => "refreshed".to_string(),
        RefreshResult::Revoked(reason) => format!("revoked: {reason}"),
        RefreshResult::Transient(reason) => format!("transient: {reason}"),
        RefreshResult::NoCredentials => "no_credentials".to_string(),
    }
}

/// Exchange Claude Code's stored refresh token for a fresh pair and write the
/// result straight back into Claude Code's own Keychain item.
///
/// Anthropic rotates the *refresh* token on every refresh, so the copy in the
/// Keychain is dead the moment the grant succeeds. The write-back is what
/// keeps Claude Code signed in, which is why a failure there is reported as a
/// failed refresh rather than logged and swallowed. Every Keychain operation
/// on this path goes through `/usr/bin/security`, so none of it can raise a
/// panel.
#[cfg(target_os = "macos")]
async fn try_refresh_claude_oauth() -> RefreshResult {
    use super::oauth_refresh::{refresh_oauth_token, RefreshOutcome};

    let raw = match read_raw_credentials_via_security_cli() {
        Ok(raw) => raw,
        Err(e) => {
            tracing::debug!(error = %e, "OAuth refresh: Claude Code credentials unreadable");
            return RefreshResult::NoCredentials;
        }
    };
    let refresh_token = match extract_refresh_token(&raw) {
        Ok(t) => t,
        Err(e) => {
            tracing::debug!(error = %e, "OAuth refresh: credentials missing refresh_token");
            return RefreshResult::NoCredentials;
        }
    };

    tracing::info!("Claude OAuth: attempting refresh-grant against Anthropic");
    match refresh_oauth_token(&refresh_token).await {
        RefreshOutcome::Refreshed(resp) => {
            let updated = match update_credentials_with_refresh(
                &raw,
                &resp.access_token,
                resp.refresh_token.as_deref(),
                resp.expires_in,
            ) {
                Ok(j) => j,
                Err(e) => return RefreshResult::Transient(format!("rewrite: {e}")),
            };

            if let Err(e) = write_credentials_to_claude_code_keychain(&updated) {
                tracing::error!(error = %e,
                    "Claude OAuth: refresh succeeded but the rotated credentials could not be \
                     written back to Claude Code's Keychain item — Claude Code may need to \
                     sign in again");
                return RefreshResult::Transient(format!("claude keychain write: {e}"));
            }

            invalidate_access_token_cache();
            store_access_token(&resp.access_token);
            RefreshResult::Refreshed
        }
        RefreshOutcome::Revoked(reason) => RefreshResult::Revoked(reason),
        RefreshOutcome::Transient(reason) => RefreshResult::Transient(reason),
    }
}

async fn try_fetch_claude_rate_limits() -> FetchAttempt {
    let token = match get_claude_oauth_token() {
        Ok(token) => token,
        Err(err) => {
            tracing::debug!(reason = %err, "Claude OAuth: no token available");
            return FetchAttempt::Other(RateLimitFetchError::message(err));
        }
    };

    let client = reqwest::Client::new();

    // Fetch usage + account in parallel
    let usage_fut = client
        .get(ANTHROPIC_USAGE_URL)
        .bearer_auth(&token)
        .header("anthropic-beta", "oauth-2025-04-20")
        .send();

    let account_fut = client
        .get(ANTHROPIC_ACCOUNT_URL)
        .bearer_auth(&token)
        .header("anthropic-beta", "oauth-2025-04-20")
        .send();

    let (usage_res, account_res) = tokio::join!(usage_fut, account_fut);

    // Parse usage response
    let usage_resp = match usage_res {
        Ok(r) => r,
        Err(e) => {
            return FetchAttempt::Other(RateLimitFetchError::message(format!(
                "Usage API request failed: {e}"
            )));
        }
    };
    if !usage_resp.status().is_success() {
        let err = rate_limit_error_from_response(&usage_resp);
        return if usage_resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            FetchAttempt::Unauthorized(err)
        } else {
            FetchAttempt::Other(err)
        };
    }
    let usage: Value = match usage_resp.json().await {
        Ok(u) => u,
        Err(e) => {
            return FetchAttempt::Other(RateLimitFetchError::message(format!(
                "Failed to parse usage response: {e}"
            )));
        }
    };

    // Parse account response (non-fatal if it fails)
    let plan_tier = match account_res {
        Ok(resp) if resp.status().is_success() => resp
            .json::<ClaudeAccountResponse>()
            .await
            .ok()
            .and_then(|acct| detect_claude_plan(&acct)),
        _ => None,
    };

    let windows = claude_usage_windows(&usage);
    let extra_usage = usage
        .get("extra_usage")
        .cloned()
        .and_then(|v| serde_json::from_value::<ClaudeExtraUsageData>(v).ok())
        .map(normalize_claude_extra_usage);

    tracing::debug!(
        windows_count = windows.len(),
        plan_tier = ?plan_tier,
        has_extra_usage = extra_usage.is_some(),
        "Claude OAuth: API success"
    );

    FetchAttempt::Ok(ProviderRateLimits {
        provider: "claude".to_string(),
        plan_tier,
        windows,
        extra_usage,
        credits: None,
        stale: false,
        error: None,
        retry_after_seconds: None,
        cooldown_until: None,
        fetched_at: Local::now().to_rfc3339(),
    })
}

fn detect_claude_plan(acct: &ClaudeAccountResponse) -> Option<String> {
    for membership in &acct.memberships {
        if let Some(caps) = &membership.organization.capabilities {
            if caps.iter().any(|c| c == "claude_max") {
                // Use rate_limit_tier for more detail if available
                if let Some(tier) = &membership.organization.rate_limit_tier {
                    return Some(format_claude_plan_tier(tier));
                }
                return Some("Max".to_string());
            }
        }
    }
    // Fallback: check first membership with capabilities
    for membership in &acct.memberships {
        if let Some(caps) = &membership.organization.capabilities {
            if caps.contains(&"chat".to_string()) && !caps.contains(&"api".to_string()) {
                return Some("Pro".to_string());
            }
        }
    }
    None
}

fn format_claude_plan_tier(tier: &str) -> String {
    if tier.contains("claude_max_20x") {
        "Max 20x".to_string()
    } else if tier.contains("claude_max") {
        "Max 5x".to_string() // covers claude_max_5x and base Max plan ($100)
    } else if tier.contains("pro") {
        "Pro".to_string()
    } else {
        tier.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[cfg(target_os = "macos")]
    use std::sync::Mutex as StdMutex;
    use tempfile::TempDir;

    #[cfg(target_os = "macos")]
    static ENV_LOCK: StdMutex<()> = StdMutex::new(());

    /// This file's own source with the test module stripped, so the
    /// source-scanning guards below don't match their own string literals.
    #[cfg(target_os = "macos")]
    fn production_source() -> &'static str {
        const MARKER: &str = "#[cfg(test)]\nmod tests {";
        let source = include_str!("claude.rs");
        source
            .find(MARKER)
            .map(|idx| &source[..idx])
            .expect("test module marker not found — did the module header change?")
    }

    fn credentials_json(token: &str) -> String {
        format!(
            r#"{{
  "claudeAiOauth": {{
    "accessToken": "{token}",
    "refreshToken": "refresh-token",
    "expiresAt": 1777084603000,
    "scopes": ["org:create_api_key"],
    "subscriptionType": "max",
    "rateLimitTier": "claude_max"
  }}
}}"#
        )
    }

    #[test]
    fn reads_access_token_from_credentials_file_payload() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(".credentials.json");
        fs::write(&path, credentials_json("file-access-token")).unwrap();

        let token = read_token_from_credentials_path(&path).unwrap();

        assert_eq!(token, "file-access-token");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn oauth_token_prefers_credentials_file_on_macos() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous = std::env::var_os("CLAUDE_CONFIG_DIR");

        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join(".credentials.json"),
            credentials_json("macos-file-access-token"),
        )
        .unwrap();
        std::env::set_var("CLAUDE_CONFIG_DIR", tmp.path());

        let token = read_token_from_credentials_file().unwrap();

        assert_eq!(token, "macos-file-access-token");

        if let Some(value) = previous {
            std::env::set_var("CLAUDE_CONFIG_DIR", value);
        } else {
            std::env::remove_var("CLAUDE_CONFIG_DIR");
        }
    }

    // Missing-item and round-trip behaviour of the `security` wrapper is
    // covered in `platform::macos::keychain`, which owns those calls now.

    /// Claude Code stores its credentials with `security add-generic-password`,
    /// so reading them back through the same binary must succeed silently even
    /// though the in-process Security.framework read is denied with
    /// `errSecAuthFailed`. Ignored by default because it needs a real login
    /// keychain with Claude Code signed in.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires a login keychain with Claude Code signed in"]
    fn live_security_cli_reads_claude_code_credentials() {
        let token = read_token_via_security_cli().unwrap();
        assert!(!token.is_empty());
    }

    /// The refreshed payload is written straight back into Claude Code's own
    /// Keychain item, so every field Claude Code stores has to survive the
    /// round-trip — dropping one would corrupt Claude Code's credentials.
    #[cfg(target_os = "macos")]
    #[test]
    fn refresh_rewrite_preserves_every_unrelated_credential_field() {
        let original = r#"{
  "claudeAiOauth": {
    "accessToken": "old-access",
    "refreshToken": "old-refresh",
    "expiresAt": 1785833947766,
    "refreshTokenExpiresAt": 1793609947766,
    "scopes": ["user:inference", "user:profile"],
    "subscriptionType": "max",
    "rateLimitTier": "claude_max_20x"
  },
  "mcpOAuth": { "some-server": { "accessToken": "keep-me" } }
}"#;

        let updated = update_credentials_with_refresh(
            original,
            "new-access",
            Some("new-refresh"),
            Some(3600),
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&updated).unwrap();
        let oauth = &parsed["claudeAiOauth"];

        assert_eq!(oauth["accessToken"], "new-access");
        assert_eq!(oauth["refreshToken"], "new-refresh");
        assert!(oauth["expiresAt"].as_u64().unwrap() > 1785833947766);
        // Untouched fields must come through byte-identical.
        assert_eq!(oauth["refreshTokenExpiresAt"], 1793609947766u64);
        assert_eq!(oauth["subscriptionType"], "max");
        assert_eq!(oauth["rateLimitTier"], "claude_max_20x");
        assert_eq!(oauth["scopes"][0], "user:inference");
        assert_eq!(parsed["mcpOAuth"]["some-server"]["accessToken"], "keep-me");
    }

    /// A refresh with no rotated refresh token in the response must keep the
    /// existing one rather than dropping the field.
    #[cfg(target_os = "macos")]
    #[test]
    fn refresh_rewrite_keeps_existing_refresh_token_when_not_rotated() {
        let updated = update_credentials_with_refresh(
            &credentials_json("old-access"),
            "new-access",
            None,
            None,
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&updated).unwrap();

        assert_eq!(parsed["claudeAiOauth"]["accessToken"], "new-access");
        assert_eq!(parsed["claudeAiOauth"]["refreshToken"], "refresh-token");
        assert_eq!(parsed["claudeAiOauth"]["expiresAt"], 1777084603000u64);
    }

    /// Regression guard for the recurring "enter your keychain password"
    /// panel.
    ///
    /// `SecKeychain::set_generic_password` is a find-then-modify when the item
    /// already exists — a decrypt authorization plus a modify authorization
    /// against the item's legacy ACL. Because this app is ad-hoc signed, its
    /// code identity changes on every build and release, so that ACL check
    /// fails and macOS shows a modal panel. It fired from the background
    /// refresh loop with no user gesture. Nothing in this file may write the
    /// Keychain in-process again: Claude Code's item is written through
    /// `/usr/bin/security`, and no other item is ours to keep.
    ///
    /// Deletes are exempt — `purge_legacy_owned_keychain_item` uses the
    /// unified `SecItemDelete`, which does not consult the legacy ACL.
    #[cfg(target_os = "macos")]
    #[test]
    fn no_in_process_keychain_writes_remain() {
        for forbidden in ["set_generic_password", "add_generic_password"] {
            let hits = production_source()
                .lines()
                .enumerate()
                .filter(|(_, line)| line.contains(forbidden))
                .filter(|(_, line)| !line.trim_start().starts_with("///"))
                .map(|(i, line)| format!("  line {}: {}", i + 1, line.trim()))
                .collect::<Vec<_>>();

            assert!(
                hits.is_empty(),
                "in-process Keychain write reintroduced — this pops a modal password panel \
                 from the background refresh loop. Use /usr/bin/security instead.\n{}",
                hits.join("\n")
            );
        }
    }

    /// Every in-process Keychain *read* must go through
    /// `platform::macos::keychain::with_ui_suppressed`, which both disables
    /// the process-wide interaction flag and serialises against other readers.
    /// Taking the flag ad hoc is not enough: the crate's guard restores it to
    /// *allowed* on drop, so an overlapping read would unmask the ACL panel.
    #[cfg(target_os = "macos")]
    #[test]
    fn in_process_keychain_reads_go_through_the_shared_guard() {
        let source = production_source();

        let searches = source
            .match_indices("ItemSearchOptions::new()")
            .map(|(idx, _)| source[..idx].lines().count())
            .collect::<Vec<_>>();
        let guards = source
            .match_indices("with_ui_suppressed(|")
            .map(|(idx, _)| source[..idx].lines().count())
            .collect::<Vec<_>>();

        assert!(
            !searches.is_empty(),
            "expected at least one in-process Keychain search to still exist"
        );
        for line in &searches {
            assert!(
                guards.iter().any(|g| *g < *line && line - g < 12),
                "Keychain search near line {line} is not inside a with_ui_suppressed() \
                 closure (closures open at {guards:?})"
            );
        }

        // Taking the raw flag here again would reintroduce the unserialised
        // pattern the shared helper exists to prevent. Prose may still name
        // it — only executable lines matter.
        let raw_flag = source
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim_start().starts_with("//"))
            .filter(|(_, line)| line.contains("disable_user_interaction()"))
            .map(|(i, line)| format!("  line {}: {}", i + 1, line.trim()))
            .collect::<Vec<_>>();
        assert!(
            raw_flag.is_empty(),
            "use platform::macos::keychain::with_ui_suppressed instead of taking \
             the process-wide flag directly\n{}",
            raw_flag.join("\n")
        );
    }

    /// Exercises the full 401 → refresh → write-back path against the live
    /// account. Anthropic rotates the refresh token, so this also proves the
    /// write-back landed: if it did not, Claude Code would be signed out.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    #[ignore = "rotates the live Claude refresh token"]
    async fn live_refresh_rotates_and_writes_back_to_claude_code_item() {
        let before = read_raw_credentials_via_security_cli().unwrap();

        let outcome = debug_force_refresh().await;
        assert_eq!(outcome, "refreshed", "refresh did not succeed");

        let after = read_raw_credentials_via_security_cli().unwrap();
        assert_ne!(before, after, "Claude Code's Keychain item was not updated");
        assert_ne!(
            extract_refresh_token(&before).unwrap(),
            extract_refresh_token(&after).unwrap(),
            "expected Anthropic to rotate the refresh token"
        );
    }

    #[tokio::test]
    #[ignore = "requires local Claude credentials and network access"]
    async fn live_fetches_full_claude_rate_limit_windows_from_credentials_file() {
        invalidate_access_token_cache();

        let rate_limits = fetch_claude_rate_limits().await.unwrap();
        let window_ids = rate_limits
            .windows
            .iter()
            .map(|window| window.window_id.as_str())
            .collect::<Vec<_>>();
        println!("Claude rate-limit windows: {window_ids:?}");

        assert!(window_ids.contains(&"five_hour"));
        assert!(window_ids.contains(&"seven_day"));
    }

    #[test]
    fn normalizes_claude_extra_usage_from_cents_to_usd() {
        let extra_usage = normalize_claude_extra_usage(ClaudeExtraUsageData {
            is_enabled: true,
            monthly_limit: 5000.0,
            used_credits: 710.0,
            utilization: Some(14.2),
        });

        assert!(extra_usage.is_enabled);
        assert_eq!(extra_usage.monthly_limit, 50.0);
        assert_eq!(extra_usage.used_credits, 7.1);
        assert_eq!(extra_usage.utilization, Some(14.2));
    }

    #[test]
    fn builds_windows_from_oauth_usage_payload() {
        let usage = serde_json::json!({
            "five_hour": { "utilization": 12.5, "resets_at": "2026-07-16T20:00:00Z" },
            "seven_day": { "utilization": 40.0, "resets_at": "2026-07-20T00:00:00Z" },
            "limits": [
                { "kind": "weekly_scoped", "percent": 90.0, "is_active": false,
                  "scope": { "model": { "display_name": "Opus" } } },
                { "kind": "seven_day_overage_included", "percent": 60.0, "is_active": true,
                  "resets_at": "2026-07-20T00:00:00Z",
                  "scope": { "model": { "id": null, "display_name": "Fable" } } }
            ],
            "extra_usage": {
                "is_enabled": true,
                "monthly_limit": 5000.0,
                "used_credits": 100.0,
                "utilization": 2.0
            }
        });
        let windows = claude_usage_windows(&usage);
        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0].window_id, "five_hour");
        assert_eq!(windows[0].label, "Session (5hr)");
        assert_eq!(windows[1].window_id, "seven_day");
        assert_eq!(windows[1].label, "Weekly (7 day)");
        assert_eq!(windows[2].window_id, "seven_day_fable");
        assert_eq!(windows[2].label, "Weekly Fable");
        assert_eq!(windows[2].utilization, 60.0);
    }

    #[test]
    fn omits_missing_claude_meters_and_surfaces_unknown_ones() {
        let usage = serde_json::json!({
            "seven_day": { "utilization": 10.0 },
            "bonus_pool": { "utilization": 3.0, "resets_at": "2026-07-20T00:00:00Z" }
        });
        let windows = claude_usage_windows(&usage);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].window_id, "seven_day");
        assert_eq!(windows[1].window_id, "bonus_pool");
        assert_eq!(windows[1].label, "Bonus Pool");
        assert_eq!(windows[1].utilization, 3.0);
    }

    #[test]
    fn extract_access_token_from_valid_json() {
        let json = r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat01-test","refreshToken":"rt"}}"#;
        assert_eq!(extract_access_token(json).unwrap(), "sk-ant-oat01-test");
    }

    #[test]
    fn extract_access_token_rejects_missing_field() {
        let json = r#"{"other": "data"}"#;
        assert!(extract_access_token(json).is_err());
    }

    #[test]
    fn extract_access_token_rejects_invalid_json() {
        assert!(extract_access_token("not json").is_err());
    }

    /// Serializes tests that touch the module-level token cache or the
    /// `CLAUDE_CODE_OAUTH_TOKEN` env var, both of which are global state.
    static SHARED_STATE_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn get_claude_oauth_token_reads_env_override() {
        let _guard = SHARED_STATE_LOCK.lock().unwrap();
        let json = r#"{"claudeAiOauth":{"accessToken":"sk-from-env"}}"#;
        // SAFETY: serialized via SHARED_STATE_LOCK so no other test reads or
        // writes the same env var concurrently.
        unsafe {
            std::env::set_var("CLAUDE_CODE_OAUTH_TOKEN", json);
        }
        let result = get_claude_oauth_token();
        unsafe {
            std::env::remove_var("CLAUDE_CODE_OAUTH_TOKEN");
        }
        assert_eq!(result.unwrap(), "sk-from-env");
    }

    #[test]
    fn access_token_cache_stores_and_invalidates() {
        let _guard = SHARED_STATE_LOCK.lock().unwrap();
        invalidate_access_token_cache();
        assert!(cached_access_token().is_none());

        store_access_token("sk-cached");
        assert_eq!(cached_access_token().as_deref(), Some("sk-cached"));

        invalidate_access_token_cache();
        assert!(cached_access_token().is_none());
    }

    #[test]
    fn get_claude_oauth_token_returns_cached_value_without_keychain() {
        let _guard = SHARED_STATE_LOCK.lock().unwrap();
        // Make sure the env var is not set so we exercise the cache branch.
        // SAFETY: serialized via SHARED_STATE_LOCK.
        unsafe {
            std::env::remove_var("CLAUDE_CODE_OAUTH_TOKEN");
        }
        store_access_token("sk-from-cache");
        let result = get_claude_oauth_token();
        invalidate_access_token_cache();
        assert_eq!(result.unwrap(), "sk-from-cache");
    }
}
