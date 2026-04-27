//! FALSIFY-CCPA-006 — `sovereignty_on_replay` (algorithm-level).
//!
//! Asserts the hostname-allowlist algorithm. Network-namespace egress
//! drop is a CI-level concern enforced separately (see contract
//! § sovereignty.enforcement).

#![allow(clippy::expect_used, clippy::panic)]

use ccpa_differ::{
    check_sovereignty, replay_is_sovereign, SovereigntyViolation, FORBIDDEN_REPLAY_EGRESS,
};

#[test]
fn empty_observed_hosts_is_sovereign() {
    assert!(replay_is_sovereign(std::iter::empty()));
    assert!(check_sovereignty(std::iter::empty()).is_empty());
}

#[test]
fn allowed_hosts_are_sovereign() {
    let hosts = ["127.0.0.1", "localhost", "10.0.0.1", "fixtures.local"];
    assert!(replay_is_sovereign(hosts.iter().copied()));
}

#[test]
fn exact_api_anthropic_match_is_violation() {
    let v = check_sovereignty(["api.anthropic.com"].iter().copied());
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].host, "api.anthropic.com");
    assert_eq!(v[0].matched_pattern, "api.anthropic.com");
}

#[test]
fn console_anthropic_subdomain_match_is_violation() {
    let v = check_sovereignty(["console.anthropic.com"].iter().copied());
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].matched_pattern, "*.anthropic.com");
}

#[test]
fn bare_anthropic_com_does_not_match_wildcard_alone() {
    // *.anthropic.com per contract excludes bare anthropic.com (which
    // would only match if explicitly listed). Currently it isn't.
    let v = check_sovereignty(["anthropic.com"].iter().copied());
    assert!(v.is_empty());
}

#[test]
fn forbidden_constant_matches_contract() {
    assert_eq!(
        FORBIDDEN_REPLAY_EGRESS,
        &["api.anthropic.com", "*.anthropic.com"]
    );
}

#[test]
fn mixed_observed_hosts_report_only_violations() {
    let hosts = [
        "127.0.0.1",
        "api.anthropic.com", // forbidden
        "github.com",
        "claude.anthropic.com", // forbidden
        "huggingface.co",
    ];
    let v = check_sovereignty(hosts.iter().copied());
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].host, "api.anthropic.com");
    assert_eq!(v[1].host, "claude.anthropic.com");
    assert!(!replay_is_sovereign(hosts.iter().copied()));
}

#[test]
fn host_with_anthropic_substring_only_does_not_match() {
    // `anthropicfoo.com` is NOT a subdomain of anthropic.com.
    let v = check_sovereignty(["anthropicfoo.com"].iter().copied());
    assert!(v.is_empty());
}

#[test]
fn nested_subdomain_still_matches_wildcard() {
    let v = check_sovereignty(["staging.api.anthropic.com"].iter().copied());
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].matched_pattern, "*.anthropic.com");
}

#[test]
fn violation_struct_clone_eq_debug() {
    let v = SovereigntyViolation {
        host: "api.anthropic.com".into(),
        matched_pattern: "api.anthropic.com".into(),
    };
    let v2 = v.clone();
    assert_eq!(v, v2);
    let _ = format!("{v:?}");
}
