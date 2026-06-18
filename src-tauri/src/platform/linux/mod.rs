//! Linux-specific platform code.

use std::time::Duration;
use tauri::WebviewWindow;

/// Calculate the target top-right position for the window in physical pixels.
///
/// Returns `None` if no monitor can be determined.
fn target_top_right(window: &WebviewWindow) -> Option<(i32, i32)> {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
        .or_else(|| {
            window
                .available_monitors()
                .ok()
                .and_then(|m| m.into_iter().next())
        })?;

    let scale = monitor.scale_factor();
    let mon_pos = monitor.position();
    let mon_size = monitor.size();
    let margin = (8.0 * scale) as i32;

    let win_size = window.outer_size().unwrap_or_else(|_| {
        // Window not yet realized — use config dimensions scaled to physical.
        tauri::PhysicalSize::new((340.0 * scale) as u32, (300.0 * scale) as u32)
    });

    let x = mon_pos.x + mon_size.width as i32 - win_size.width as i32 - margin;
    let y = mon_pos.y + margin;

    tracing::debug!(
        mon_w = mon_size.width,
        mon_h = mon_size.height,
        win_w = win_size.width,
        win_h = win_size.height,
        scale,
        x,
        y,
        "target_top_right: calculated position"
    );
    Some((x, y))
}

/// Position the window in the top-right corner of the work area.
///
/// Always uses manual calculation — the positioner plugin's `Position::TopRight`
/// returns `Ok` on Linux but frequently places the window at the wrong position
/// (especially on first show), so we bypass it entirely.
pub fn position_top_right(window: &WebviewWindow) {
    let Some((x, y)) = target_top_right(window) else {
        tracing::debug!("position_top_right: no monitor found, skipping");
        return;
    };
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

/// Re-snap the window to the top-right anchor, but ONLY if the window manager
/// has drifted it away from there (beyond a small tolerance).
///
/// X11/Wayland window managers freely reposition top-level windows on their own
/// — e.g. when another window (a file manager) opens, the WM can shift/re-stack
/// the popover. The frontend resize/focus loop calls `set_window_size_and_align`
/// very frequently; routing it through here re-anchors the popover top-right on
/// the next tick instead of preserving the WM-drifted position (which made the
/// window appear to "follow" the file manager). Gating on actual drift avoids a
/// `set_position` on every tick — some WMs render redundant moves as jitter — so
/// this is a no-op in the common (already-anchored) case.
pub fn reanchor_top_right_if_drifted(window: &WebviewWindow) {
    let Some((tx, ty)) = target_top_right(window) else {
        return;
    };
    // Tolerance in physical px. The work-area clamp keeps the window an 8px*scale
    // margin off the edge, so anything past ~24px is a genuine WM-driven move.
    const DRIFT_TOLERANCE: i32 = 24;
    if let Ok(pos) = window.outer_position() {
        let anchored =
            (pos.x - tx).abs() <= DRIFT_TOLERANCE && (pos.y - ty).abs() <= DRIFT_TOLERANCE;
        if anchored {
            // Already anchored — leave it alone (no extra X11 round-trip).
            return;
        }
    }
    let _ = window.set_position(tauri::PhysicalPosition::new(tx, ty));
    super::clamp_window_to_work_area(window);
}

/// Spawn a deferred re-position loop that retries with increasing delays.
///
/// Linux window managers process `show()` asynchronously — on the very first
/// show the WM may not have realized the window yet, causing `set_position` to
/// be ignored or overridden.  This retries several times with increasing back-off
/// and stops early once the position is confirmed correct.
pub fn deferred_reposition(window: WebviewWindow) {
    std::thread::spawn(move || {
        // Progressive delays: most X11 WMs settle within 100-200ms;
        // some compositors (Mutter, KWin) may need longer.
        let delays_ms: &[u64] = &[100, 200, 350, 500, 800];

        for (attempt, &delay) in delays_ms.iter().enumerate() {
            std::thread::sleep(Duration::from_millis(delay));

            // If the window is already close to the target, stop retrying.
            if let (Some((ex, ey)), Ok(pos)) = (target_top_right(&window), window.outer_position())
            {
                if (pos.x - ex).abs() <= 50 && (pos.y - ey).abs() <= 50 {
                    tracing::debug!(
                        attempt = attempt + 1,
                        "deferred_reposition: position correct, done"
                    );
                    return;
                }
            }

            position_top_right(&window);
            super::clamp_window_to_work_area(&window);

            tracing::debug!(attempt = attempt + 1, delay, "deferred_reposition: retried");
        }

        // Log if position still didn't stick (likely Wayland limitation).
        if let (Some((ex, ey)), Ok(pos)) = (target_top_right(&window), window.outer_position()) {
            if (pos.x - ex).abs() > 50 || (pos.y - ey).abs() > 50 {
                tracing::warn!(
                    expected_x = ex,
                    expected_y = ey,
                    actual_x = pos.x,
                    actual_y = pos.y,
                    "deferred_reposition: position incorrect after all retries \
                     (Wayland compositors may ignore set_position for top-level windows)"
                );
            }
        }
    });
}
