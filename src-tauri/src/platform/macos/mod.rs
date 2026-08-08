//! macOS-specific platform code.

/// Generic-password access via `/usr/bin/security`, used in place of
/// Security.framework because this app is ad-hoc signed. See the module docs
/// for why an in-process write pops a modal password panel.
pub mod keychain;

/// Set Dock icon visibility via activation policy.
pub fn set_dock_icon_visible(app: &tauri::AppHandle, visible: bool) -> Result<(), String> {
    use tauri::ActivationPolicy;
    let policy = if visible {
        ActivationPolicy::Regular
    } else {
        ActivationPolicy::Accessory
    };
    app.set_activation_policy(policy)
        .map_err(|e| format!("Failed to set activation policy: {e}"))
}
