use super::tray::{patch_tray_utilization, sync_tray_title, tray_utilization_from_rate_limits};
use super::{AppState, UsageDebugReport};
use crate::models::*;
use tauri::State;

/// Apply native window surface adjustments.
/// On Windows: sets DWM rounded corners. On other platforms: noop.
#[tauri::command]
pub async fn set_window_surface(
    _app: tauri::AppHandle,
    _state: State<'_, AppState>,
    _surface: serde_json::Value,
    _corner_radius: Option<f64>,
) -> Result<(), String> {
    Ok(())
}

/// Noop -- glass effects removed for cross-platform compatibility.
#[tauri::command]
pub async fn set_glass_effect(
    _app: tauri::AppHandle,
    _state: State<'_, AppState>,
    _enabled: bool,
) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub async fn set_refresh_interval(interval: u64, state: State<'_, AppState>) -> Result<(), String> {
    let mut current = state.refresh_interval.write().await;
    *current = interval;
    Ok(())
}

/// Enable or disable rate-limit fetching.
///
/// Pre-rewrite this gated the macOS Keychain probe; post-rewrite the fetch
/// only touches the statusline event file and the JSONL parser cache, so
/// the toggle is effectively cosmetic — kept so existing settings.json
/// files continue to round-trip and so users can hide the rate-limit row
/// if they want.
#[tauri::command]
pub async fn set_rate_limits_enabled(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .rate_limits_enabled
        .store(enabled, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

/// Enable or disable local Claude/Codex session-log reads.
///
/// Brand-new installs keep this off until the welcome disclosure has been
/// dismissed, so any macOS TCC prompt caused by unusual log locations is
/// preceded by app-owned context.
#[tauri::command]
pub async fn set_usage_access_enabled(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .usage_access_enabled
        .store(enabled, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

/// Result of an App Data TCC probe. We can't query macOS directly for
/// the user's recorded TCC decision, so we infer it from a `read_dir`
/// outcome: success means access was granted (or never required because
/// the directory doesn't exist on this machine); a permission error means
/// it was denied (or never asked).
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case", tag = "status")]
#[allow(dead_code)]
pub enum AppDataAccessState {
    /// At least one root is readable, *or* none of the roots exist (so no
    /// prompt would ever fire — treat as a no-op grant).
    Granted,
    /// All existing roots returned a permission error. The user either
    /// previously denied the prompt or it has never been answered. Either
    /// way, the next step is System Settings — no further `read_dir` will
    /// re-fire the sheet.
    Denied,
    /// macOS Sequoia (App Data TCC) doesn't apply on this OS.
    NotApplicable,
}

/// Probe Claude Code / Codex CLI session-log roots to determine the App
/// Data TCC state without firing the user-facing prompt — the prompt only
/// fires on the *first* `read_dir` after a fresh install / TCC reset.
/// After that, this call returns the cached decision silently.
///
/// macOS only; other OSes return `NotApplicable` because there's no App
/// Data TCC layer for them to deny.
#[tauri::command]
pub async fn check_app_data_access() -> Result<AppDataAccessState, String> {
    #[cfg(target_os = "macos")]
    {
        use std::fs;
        use std::io::ErrorKind;

        let mut roots: Vec<std::path::PathBuf> = crate::paths::claude_project_roots_default();
        if let Some(p) = crate::paths::codex_sessions_default() {
            roots.push(p);
        }

        let mut any_existing = false;
        let mut any_readable = false;
        let mut any_permission_denied = false;

        for root in &roots {
            // `metadata()` doesn't trigger AppData TCC — it only checks the
            // path's *existence*. If the path doesn't exist, no permission
            // question applies.
            if !root.exists() {
                continue;
            }
            any_existing = true;
            match fs::read_dir(root) {
                Ok(_) => {
                    any_readable = true;
                }
                Err(err) if matches!(err.kind(), ErrorKind::PermissionDenied) => {
                    any_permission_denied = true;
                }
                Err(_) => {
                    // Other errors (EIO, ENOTDIR, etc.) — don't infer from these.
                }
            }
        }

        if !any_existing {
            return Ok(AppDataAccessState::Granted);
        }
        if any_readable {
            return Ok(AppDataAccessState::Granted);
        }
        if any_permission_denied {
            return Ok(AppDataAccessState::Denied);
        }
        // No clear signal — treat as Denied so the UI surfaces the action.
        Ok(AppDataAccessState::Denied)
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(AppDataAccessState::NotApplicable)
    }
}

/// Force a `read_dir` against Claude Code / Codex CLI session-log roots.
/// On Sequoia this triggers the App Data TCC prompt the *first* time it's
/// called for a given app/path pair. After the user answers, this call
/// becomes a noop and the answer is read from
/// [`check_app_data_access`].
#[tauri::command]
pub async fn request_app_data_access() -> Result<u32, String> {
    use std::fs;

    let mut probed: u32 = 0;
    let mut roots: Vec<std::path::PathBuf> = crate::paths::claude_project_roots_default();
    if let Some(p) = crate::paths::codex_sessions_default() {
        roots.push(p);
    }

    for root in roots {
        let _ = fs::read_dir(&root);
        probed += 1;
        tracing::debug!(path = %root.display(), "Requested App Data TCC for path");
    }

    Ok(probed)
}

/// Open the macOS System Settings pane where the user can manage App Data
/// permissions. Used when [`check_app_data_access`] returns `Denied` —
/// the OS won't re-fire the prompt, so the user has to flip the switch
/// themselves. macOS only.
#[tauri::command]
pub async fn open_app_data_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        // The "App Management" pane on Sequoia covers App Data access.
        // Apple doesn't expose a deeper anchor, so we land on the Privacy
        // & Security root if the App-Management URL fails.
        let urls = [
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_AppBundles",
            "x-apple.systempreferences:com.apple.preference.security?Privacy",
        ];
        for url in urls {
            if Command::new("open").arg(url).status().is_ok() {
                return Ok(());
            }
        }
        Err("Failed to open System Settings".to_string())
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}

/// Set Dock icon visibility (macOS only). Noop on other platforms.
#[tauri::command]
pub async fn set_dock_icon_visible(app: tauri::AppHandle, visible: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use tauri::Manager;
        // Changing the activation policy may deactivate the app, causing the
        // main window to lose focus.  Suppress the resulting auto-hide.
        app.state::<AppState>()
            .suppress_auto_hide
            .store(true, std::sync::atomic::Ordering::SeqCst);

        crate::platform::macos::set_dock_icon_visible(&app, visible)?;

        // Re-focus main window after the policy change so it stays visible,
        // but only if it was already showing — avoid pulling a hidden window
        // to the center of the screen on startup.
        if let Some(win) = app.get_webview_window("main") {
            if win.is_visible().unwrap_or(false) {
                let _ = win.show();
                let _ = win.set_focus();
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    let _ = (app, visible);

    Ok(())
}

#[tauri::command]
pub async fn clear_cache(state: State<'_, AppState>) -> Result<(), String> {
    state.parser.clear_cache();
    *state.cached_rate_limits.write().await = None;
    *state.last_usage_debug.write().await = None;
    Ok(())
}

#[tauri::command]
pub async fn clear_payload_cache(state: State<'_, AppState>) -> Result<(), String> {
    state.parser.clear_payload_cache();
    Ok(())
}

#[tauri::command]
pub async fn clear_usage_view_cache(state: State<'_, AppState>) -> Result<(), String> {
    state.parser.clear_payload_cache_prefix("usage-view:");
    Ok(())
}

/// Reposition window so its bottom edge aligns with the work area bottom (taskbar top).
/// Called from the frontend after every window resize.
#[tauri::command]
pub async fn reposition_window(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if let Some(window) = app.get_webview_window("main") {
        #[cfg(target_os = "windows")]
        {
            crate::platform::windows::window::align_to_work_area(&window);
        }
        #[cfg(not(target_os = "windows"))]
        {
            crate::platform::clamp_window_to_work_area(&window);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn set_window_size_and_align(
    app: tauri::AppHandle,
    width: f64,
    height: f64,
) -> Result<(), String> {
    use tauri::{LogicalSize, Manager, Size};
    if let Some(window) = app.get_webview_window("main") {
        #[cfg(target_os = "windows")]
        {
            if let Some(monitor) = window.current_monitor().ok().flatten() {
                let scale = monitor.scale_factor();
                let physical_width = (width * scale).round() as u32;
                let physical_height = (height * scale).round() as u32;
                crate::platform::windows::window::set_size_and_align(
                    &window,
                    physical_width,
                    physical_height,
                );
            } else {
                let _ = window.set_size(Size::Logical(LogicalSize::new(width, height)));
                crate::platform::windows::window::align_to_work_area(&window);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            // Capture position before resize so we can keep the anchored edge fixed.
            let old_pos = window.outer_position().ok();
            let old_size = window.outer_size().ok();

            let _ = window.set_size(Size::Logical(LogicalSize::new(width, height)));

            // For bottom-anchored windows, move upward so the bottom edge stays put.
            // macOS/Linux keep top-left fixed after set_size, so top-anchored is free.
            if let (Some(pos), Some(old_sz)) = (old_pos, old_size) {
                let old_bottom = pos.y + old_sz.height as i32;
                if let Some(monitor) = window.current_monitor().ok().flatten() {
                    let work_top = monitor.position().y;
                    let work_bottom = monitor.position().y + monitor.size().height as i32;
                    let top_gap = (pos.y - work_top).abs();
                    let bottom_gap = (work_bottom - old_bottom).abs();
                    if top_gap > bottom_gap {
                        let new_size = window.outer_size().unwrap_or(old_sz);
                        let new_y = (old_bottom - new_size.height as i32).max(work_top);
                        if new_y != pos.y {
                            let _ = window.set_position(tauri::PhysicalPosition::new(pos.x, new_y));
                        }
                    }
                }
            }
            crate::platform::clamp_window_to_work_area(&window);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn get_rate_limits(
    provider: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<RateLimitsPayload, String> {
    let selection = match provider.as_deref() {
        None | Some("all") => crate::rate_limits::RateLimitSelection::All,
        Some("claude") => crate::rate_limits::RateLimitSelection::Claude,
        Some("codex") => crate::rate_limits::RateLimitSelection::Codex,
        Some(other) => return Err(format!("Invalid provider for rate limits: {other}")),
    };

    if !state
        .usage_access_enabled
        .load(std::sync::atomic::Ordering::SeqCst)
    {
        return Ok(state
            .cached_rate_limits
            .read()
            .await
            .clone()
            .unwrap_or(RateLimitsPayload {
                claude: None,
                codex: None,
            }));
    }

    let codex_dir = state.parser.codex_dir().to_path_buf();
    let plan = *state.claude_plan_tier.read().await;
    let cached = state.cached_rate_limits.read().await.clone();
    let fresh = crate::rate_limits::fetch_selected_rate_limits(
        std::sync::Arc::clone(&state.parser),
        codex_dir,
        plan,
        selection,
        cached.as_ref(),
    )
    .await;

    let merged = crate::rate_limits::merge_rate_limits(fresh, cached.as_ref());

    *state.cached_rate_limits.write().await = Some(merged.clone());
    patch_tray_utilization(&state, tray_utilization_from_rate_limits(Some(&merged))).await;

    sync_tray_title(&app, &state).await;

    Ok(merged)
}

#[tauri::command]
pub async fn get_last_usage_debug(
    state: State<'_, AppState>,
) -> Result<Option<UsageDebugReport>, String> {
    Ok(state.last_usage_debug.read().await.clone())
}
