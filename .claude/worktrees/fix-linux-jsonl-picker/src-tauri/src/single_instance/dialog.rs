//! Native, pre-Tauri confirm/error dialogs.
//!
//! These run *before* `tauri::Builder` — a declined new instance exits without
//! ever starting the event loop — so they must not depend on Tauri's event
//! loop. Each backend spins its own modal loop (Windows `MessageBoxW`) or is a
//! separate process (macOS `osascript`, Linux `zenity`/`kdialog`).

use super::port::PortHolder;

const TITLE: &str = "TokenMonitor";

/// REQ-001: another TokenMonitor is already running. Returns `true` to quit the
/// old instance and continue with the new one.
pub fn confirm_replace_old_instance() -> bool {
    confirm(
        "检测到已有一个 TokenMonitor 正在运行。\n\n\
         是否退出旧实例，并以当前启动的实例继续？\n\n\
         选择「否」将退出本次新启动的实例。",
    )
}

/// REQ-002: the lock port is held by a foreign process. Returns `true` to
/// terminate that process and free the port.
pub fn confirm_free_port(holder: Option<&PortHolder>, port: u16) -> bool {
    let who = match holder {
        Some(h) if !h.name.is_empty() => format!("进程「{}」(PID {})", h.name, h.pid),
        Some(h) => format!("进程 PID {}", h.pid),
        None => "一个未知进程".to_string(),
    };
    confirm(&format!(
        "端口 {port} 正被{who}占用，TokenMonitor 需要它来防止重复启动。\n\n\
         是否结束该进程以释放端口？"
    ))
}

/// Terminal error shown when take-over fails (rebind timeout / kill failure).
pub fn show_error(msg: &str) {
    error(msg);
}

// ─────────────────────────── Windows ───────────────────────────
#[cfg(target_os = "windows")]
fn confirm(text: &str) -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDYES, MB_ICONWARNING, MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO,
    };
    let wtext = to_wide(text);
    let wtitle = to_wide(TITLE);
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(wtext.as_ptr()),
            PCWSTR(wtitle.as_ptr()),
            MB_YESNO | MB_ICONWARNING | MB_SETFOREGROUND | MB_TOPMOST,
        ) == IDYES
    }
}

#[cfg(target_os = "windows")]
fn error(text: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MB_TOPMOST,
    };
    let wtext = to_wide(text);
    let wtitle = to_wide(TITLE);
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(wtext.as_ptr()),
            PCWSTR(wtitle.as_ptr()),
            MB_OK | MB_ICONERROR | MB_SETFOREGROUND | MB_TOPMOST,
        );
    }
}

#[cfg(target_os = "windows")]
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// ─────────────────────────── macOS ───────────────────────────
#[cfg(target_os = "macos")]
fn confirm(text: &str) -> bool {
    use std::process::Command;
    // NSAlert is unsafe here (NSApplication isn't initialized yet, pre-Builder).
    // osascript is a separate process with its own event loop.
    let script = format!(
        "display dialog {} buttons {{\"取消\", \"确定\"}} default button \"确定\" \
         with title \"{TITLE}\" with icon caution",
        applescript_quote(text)
    );
    match Command::new("osascript").arg("-e").arg(&script).output() {
        // Cancel -> user-canceled error -> non-zero exit -> false (safe default).
        Ok(out) => out.status.success() && String::from_utf8_lossy(&out.stdout).contains("确定"),
        Err(_) => false,
    }
}

#[cfg(target_os = "macos")]
fn error(text: &str) {
    use std::process::Command;
    let script = format!(
        "display dialog {} buttons {{\"好\"}} default button \"好\" \
         with title \"{TITLE}\" with icon stop",
        applescript_quote(text)
    );
    let _ = Command::new("osascript").arg("-e").arg(&script).output();
}

#[cfg(target_os = "macos")]
fn applescript_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

// ─────────────────────────── Linux ───────────────────────────
#[cfg(target_os = "linux")]
fn confirm(text: &str) -> bool {
    use std::process::Command;
    if command_exists("zenity") {
        return Command::new("zenity")
            .args([
                "--question",
                "--no-wrap",
                &format!("--title={TITLE}"),
                &format!("--text={text}"),
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }
    if command_exists("kdialog") {
        return Command::new("kdialog")
            .args(["--title", TITLE, "--yesno", text])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }
    // No dialog tool (headless / minimal). Decline is the safe default — never
    // gtk_init ourselves here (it conflicts with tao's later GTK init).
    tracing::warn!("zenity/kdialog unavailable; declining single-instance prompt");
    false
}

#[cfg(target_os = "linux")]
fn error(text: &str) {
    use std::process::Command;
    if command_exists("zenity") {
        let _ = Command::new("zenity")
            .args([
                "--error",
                "--no-wrap",
                &format!("--title={TITLE}"),
                &format!("--text={text}"),
            ])
            .status();
    } else if command_exists("kdialog") {
        let _ = Command::new("kdialog")
            .args(["--title", TITLE, "--error", text])
            .status();
    } else {
        tracing::error!("{text}");
    }
}

#[cfg(target_os = "linux")]
fn command_exists(name: &str) -> bool {
    use std::process::Command;
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {name}"))
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}

#[cfg(test)]
#[cfg(target_os = "macos")]
mod tests {
    use super::applescript_quote;

    #[test]
    fn applescript_quote_escapes() {
        assert_eq!(applescript_quote("a\"b"), "\"a\\\"b\"");
        assert_eq!(applescript_quote("a\\b"), "\"a\\\\b\"");
        assert_eq!(applescript_quote("a\nb"), "\"a\\nb\"");
        assert_eq!(applescript_quote("a\r\nb"), "\"a\\nb\"");
    }
}
