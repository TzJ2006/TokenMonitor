//! Pure wire-protocol logic for the single-instance lock — no I/O.
//!
//! The lock is a loopback TCP listener on [`LOCK_PORT`]. A launching instance
//! that fails to bind connects and speaks this tiny line protocol to decide
//! whether the holder is *our own* already-running instance or some *foreign*
//! process. Keeping the parsing/formatting/decision logic here (free of
//! sockets) makes it fully unit-testable.

/// Default loopback lock port. Sits in the dynamic/ephemeral range
/// (49152–65535) and is not a known service port. Overridable at runtime via
/// the `TM_LOCK_PORT` environment variable (see [`parse_lock_port_override`]).
pub const LOCK_PORT: u16 = 53217;

/// First token of a PROBE reply. Guards against mistaking an unrelated process
/// (that merely happens to hold the port) for our own instance.
pub const MAGIC: &str = "TOKENMONITOR";

/// A request sent by a launching instance to the running owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// "Are you a TokenMonitor instance?" — owner replies with [`format_probe_reply`].
    Probe,
    /// "Please exit." — owner replies `OK` then `app.exit(0)`.
    Quit,
    /// "Bring your window forward." — owner replies `OK` and shows its window.
    Focus,
}

/// Parse a single request line (whitespace/`\r\n` tolerant, case-insensitive).
pub fn parse_request(line: &str) -> Option<Request> {
    match line.trim().to_ascii_uppercase().as_str() {
        "PROBE" => Some(Request::Probe),
        "QUIT" => Some(Request::Quit),
        "FOCUS" => Some(Request::Focus),
        _ => None,
    }
}

/// The decoded contents of a successful PROBE reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeReply {
    pub version: String,
    pub pid: u32,
}

/// Format the owner's reply to a PROBE: `TOKENMONITOR <version> <pid>\n`.
pub fn format_probe_reply(version: &str, pid: u32) -> String {
    format!("{MAGIC} {version} {pid}\n")
}

/// Parse a PROBE reply. Returns `None` unless the line begins with [`MAGIC`]
/// and carries a parseable `<version> <pid>` — anything else is treated as a
/// foreign process.
pub fn parse_probe_reply(line: &str) -> Option<ProbeReply> {
    let mut parts = line.split_whitespace();
    if parts.next()? != MAGIC {
        return None;
    }
    let version = parts.next()?.to_string();
    let pid = parts.next()?.parse::<u32>().ok()?;
    Some(ProbeReply { version, pid })
}

/// Outcome of the bind attempt + (optional) PROBE round-trip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceDecision {
    /// We bound the lock port — nobody else is running.
    SoleInstance,
    /// The port is held by another TokenMonitor instance.
    OwnInstanceRunning(ProbeReply),
    /// The port is held by an unrelated process (no/garbled reply or refused).
    ForeignProcess,
}

/// Decide what the port state means.
///
/// * `bind_ok` — did we successfully bind the lock port?
/// * `probe` — the raw line read back from a PROBE (or `None` if the connect
///   failed / timed out / nothing was returned).
pub fn decide(bind_ok: bool, probe: Option<&str>) -> InstanceDecision {
    if bind_ok {
        return InstanceDecision::SoleInstance;
    }
    match probe.and_then(parse_probe_reply) {
        Some(reply) => InstanceDecision::OwnInstanceRunning(reply),
        None => InstanceDecision::ForeignProcess,
    }
}

/// Validate a `TM_LOCK_PORT` override value. Returns `Some(port)` only for a
/// non-zero port number; non-numeric / out-of-range / zero falls back to the
/// default at the call site.
pub fn parse_lock_port_override(raw: &str) -> Option<u16> {
    match raw.trim().parse::<u16>() {
        Ok(p) if p != 0 => Some(p),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_request_variants() {
        assert_eq!(parse_request("PROBE\n"), Some(Request::Probe));
        assert_eq!(parse_request("  quit "), Some(Request::Quit));
        assert_eq!(parse_request("Focus\r\n"), Some(Request::Focus));
        assert_eq!(parse_request("garbage"), None);
        assert_eq!(parse_request(""), None);
        assert_eq!(parse_request("PROBE EXTRA"), None);
    }

    #[test]
    fn probe_reply_roundtrips() {
        let line = format_probe_reply("0.13.6", 12345);
        assert_eq!(line, "TOKENMONITOR 0.13.6 12345\n");
        let parsed = parse_probe_reply(&line).unwrap();
        assert_eq!(parsed.version, "0.13.6");
        assert_eq!(parsed.pid, 12345);
    }

    #[test]
    fn probe_reply_tolerates_crlf_and_whitespace() {
        let parsed = parse_probe_reply("  TOKENMONITOR 1.0.0 7\r\n").unwrap();
        assert_eq!(parsed.version, "1.0.0");
        assert_eq!(parsed.pid, 7);
    }

    #[test]
    fn probe_reply_rejects_non_magic() {
        assert_eq!(parse_probe_reply("HELLO 1.0.0 7"), None);
        assert_eq!(parse_probe_reply("random garbage data"), None);
        assert_eq!(parse_probe_reply(""), None);
        // Magic present but missing/invalid fields.
        assert_eq!(parse_probe_reply("TOKENMONITOR"), None);
        assert_eq!(parse_probe_reply("TOKENMONITOR 1.0.0"), None);
        assert_eq!(parse_probe_reply("TOKENMONITOR 1.0.0 notanumber"), None);
    }

    #[test]
    fn decide_truth_table() {
        // Bound -> sole instance regardless of probe.
        assert_eq!(decide(true, None), InstanceDecision::SoleInstance);
        assert_eq!(
            decide(true, Some("anything")),
            InstanceDecision::SoleInstance
        );

        // Our own instance.
        match decide(false, Some("TOKENMONITOR 0.13.6 999")) {
            InstanceDecision::OwnInstanceRunning(r) => {
                assert_eq!(r.version, "0.13.6");
                assert_eq!(r.pid, 999);
            }
            other => panic!("expected OwnInstanceRunning, got {other:?}"),
        }

        // Foreign: garbled reply, or no reply at all.
        assert_eq!(
            decide(false, Some("garbage")),
            InstanceDecision::ForeignProcess
        );
        assert_eq!(decide(false, None), InstanceDecision::ForeignProcess);
    }

    #[test]
    fn lock_port_override_parsing() {
        assert_eq!(parse_lock_port_override("40000"), Some(40000));
        assert_eq!(parse_lock_port_override("  65535 "), Some(65535));
        assert_eq!(parse_lock_port_override("0"), None);
        assert_eq!(parse_lock_port_override("99999"), None); // out of u16 range
        assert_eq!(parse_lock_port_override("abc"), None);
        assert_eq!(parse_lock_port_override(""), None);
    }
}
