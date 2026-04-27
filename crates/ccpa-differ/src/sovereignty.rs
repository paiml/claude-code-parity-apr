//! `sovereignty_on_replay` — FALSIFY-CCPA-006 (algorithm-level).
//!
//! The contract's primary enforcement is at the network-namespace level
//! (drop all egress except 127.0.0.1 in the CI replay container — see
//! contract § sovereignty.enforcement). This module ships the
//! belt-and-suspenders algorithm: given a list of hostnames the
//! replayer *attempted* to connect to, reject any that match the
//! contract's `forbidden_replay_egress` list.
//!
//! Pure function. The runtime hostname-collector lives in
//! `ccpa-replayer` (M3); this module is what the collector calls into
//! at the end of a replay run.

/// Hostnames the contract forbids during replay.
///
/// Mirrors `contracts/claude-code-parity-apr-v1.yaml § sovereignty.forbidden_replay_egress`.
pub const FORBIDDEN_REPLAY_EGRESS: &[&str] = &["api.anthropic.com", "*.anthropic.com"];

/// One sovereignty violation found by [`assert_replay_sovereignty`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SovereigntyViolation {
    /// The hostname the replayer attempted to reach.
    pub host: String,
    /// Which `FORBIDDEN_REPLAY_EGRESS` pattern matched it.
    pub matched_pattern: String,
}

/// Walk an iterator of hostnames the replayer tried to reach during a
/// session and return any that violate the sovereignty allowlist.
///
/// Empty result == zero forbidden egress attempted == sovereignty held.
#[must_use]
pub fn check_sovereignty<'a, I>(observed_hosts: I) -> Vec<SovereigntyViolation>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut violations = Vec::new();
    for host in observed_hosts {
        if let Some(pattern) = matched_pattern(host) {
            violations.push(SovereigntyViolation {
                host: host.to_owned(),
                matched_pattern: pattern.to_owned(),
            });
        }
    }
    violations
}

/// Boolean wrapper: `true` iff [`check_sovereignty`] is empty.
#[must_use]
pub fn replay_is_sovereign<'a, I>(observed_hosts: I) -> bool
where
    I: IntoIterator<Item = &'a str>,
{
    check_sovereignty(observed_hosts).is_empty()
}

fn matched_pattern(host: &str) -> Option<&'static str> {
    for pattern in FORBIDDEN_REPLAY_EGRESS {
        if host_matches(host, pattern) {
            return Some(*pattern);
        }
    }
    None
}

fn host_matches(host: &str, pattern: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix("*.") {
        // `*.anthropic.com` matches any host with that suffix EXCLUDING
        // the bare `anthropic.com` (matches contract semantics:
        // `api.anthropic.com` is enumerated separately).
        host.ends_with(&format!(".{suffix}")) && host != suffix
    } else {
        host == pattern
    }
}
