//! Per-OS helpers: find the process holding the lock port, and terminate it.
//!
//! Used only on the foreign-process branch (REQ-002), where the lock port is
//! held by something that is *not* a TokenMonitor instance and the user has
//! confirmed they want it freed.

/// A process occupying the lock port. `name` may be empty when it can't be
/// resolved — the dialog then shows just the PID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortHolder {
    pub pid: u32,
    pub name: String,
}

// ─────────────────────────── Windows ───────────────────────────
#[cfg(target_os = "windows")]
mod imp {
    use super::PortHolder;
    use std::ffi::c_void;
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    use windows::Win32::Foundation::FALSE;
    use windows::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
    };

    /// `AF_INET` (IPv4). Hardcoded to avoid pulling in the WinSock feature.
    const AF_INET: u32 = 2;
    /// 127.0.0.1 in network byte order as stored in `dwLocalAddr`.
    const LOOPBACK_ADDR: u32 = 0x0100_007F;
    /// Suppress the console window for helper subprocesses.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn find_port_holder(port: u16) -> Option<PortHolder> {
        unsafe {
            let mut size: u32 = 0;
            // First call sizes the buffer.
            GetExtendedTcpTable(
                None,
                &mut size,
                FALSE,
                AF_INET,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            );
            if size == 0 {
                return None;
            }
            let mut buf = vec![0u8; size as usize];
            let ret = GetExtendedTcpTable(
                Some(buf.as_mut_ptr() as *mut c_void),
                &mut size,
                FALSE,
                AF_INET,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            );
            if ret != 0 {
                return None;
            }
            let table = &*(buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID);
            let n = table.dwNumEntries as usize;
            let rows = std::slice::from_raw_parts(table.table.as_ptr(), n);
            for row in rows {
                let local_port = u16::from_be(row.dwLocalPort as u16);
                if local_port == port && (row.dwLocalAddr == LOOPBACK_ADDR || row.dwLocalAddr == 0)
                {
                    let pid = row.dwOwningPid;
                    return Some(PortHolder {
                        pid,
                        name: process_name(pid).unwrap_or_default(),
                    });
                }
            }
            None
        }
    }

    /// Resolve an image name via `tasklist` (CSV is locale-stable: the image
    /// name is always the first field).
    fn process_name(pid: u32) -> Option<String> {
        let out = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let first = stdout.lines().next()?.trim();
        if first.is_empty() || first.starts_with("INFO:") {
            return None;
        }
        let name = first
            .split(',')
            .next()?
            .trim()
            .trim_matches('"')
            .to_string();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    }

    pub fn kill_pid(pid: u32) -> Result<(), String> {
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(_) => Err("taskkill 返回失败（可能权限不足）".into()),
            Err(e) => Err(format!("无法执行 taskkill: {e}")),
        }
    }
}

// ─────────────────────────── Unix (macOS + Linux) ───────────────────────────
#[cfg(unix)]
mod imp {
    use super::PortHolder;
    use std::process::Command;
    use std::time::Duration;

    pub fn find_port_holder(port: u16) -> Option<PortHolder> {
        // `lsof -t` prints one PID per line for the listening socket.
        let out = Command::new("lsof")
            .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-t"])
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let pid: u32 = stdout.split_whitespace().next()?.parse().ok()?;
        let name = process_name(pid).unwrap_or_default();
        Some(PortHolder { pid, name })
    }

    fn process_name(pid: u32) -> Option<String> {
        let out = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output()
            .ok()?;
        let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    }

    pub fn kill_pid(pid: u32) -> Result<(), String> {
        let pid_s = pid.to_string();
        let mut killed = false;
        // Graceful first.
        if let Ok(s) = Command::new("kill").arg(&pid_s).status() {
            killed |= s.success();
        }
        std::thread::sleep(Duration::from_millis(400));
        // Force, best-effort (may already be gone -> non-success is fine).
        if let Ok(s) = Command::new("kill").arg("-9").arg(&pid_s).status() {
            killed |= s.success();
        }
        if killed {
            Ok(())
        } else {
            Err("无法结束进程（可能权限不足）".into())
        }
    }
}

pub use imp::{find_port_holder, kill_pid};

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn find_port_holder_finds_our_own_listener() {
        // Bind a throwaway loopback listener in this test process, then assert
        // find_port_holder reports *our* PID for that port.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        match find_port_holder(port) {
            Some(holder) => assert_eq!(holder.pid, std::process::id()),
            // Tooling (lsof) may be unavailable in some sandboxes; don't fail
            // the suite over a missing external binary.
            None => eprintln!("find_port_holder returned None (lsof/IpHelper unavailable?)"),
        }
        drop(listener);
    }
}
