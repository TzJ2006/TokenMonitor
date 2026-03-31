//! Linux-specific platform code.

use tauri::{Manager, WebviewWindow};
use tauri_plugin_positioner::{Position, WindowExt};

/// Position the window in the top-right corner of the work area.
///
/// On Linux the system tray typically lives in a top panel (GNOME, KDE, etc.),
/// so we default to top-right positioning instead of trying to detect the tray
/// rect (which is unreliable across desktop environments).
pub fn position_top_right(window: &WebviewWindow) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let ok = catch_unwind(AssertUnwindSafe(|| window.move_window(Position::TopRight)))
        .map(|r| r.is_ok())
        .unwrap_or(false);
    if !ok {
        tracing::debug!("TopRight positioner failed, falling back to manual calculation");
        position_top_right_manual(window);
    }
}

/// Manual top-right positioning using monitor dimensions.
///
/// Falls back to primary monitor when `current_monitor()` returns `None`
/// (common on first show before the window manager has realized the window).
fn position_top_right_manual(window: &WebviewWindow) {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
        .or_else(|| window.available_monitors().ok().and_then(|m| m.into_iter().next()));
    let Some(monitor) = monitor else {
        tracing::debug!("position_top_right_manual: no monitor found, skipping");
        return;
    };
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

    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}
