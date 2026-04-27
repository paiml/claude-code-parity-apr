//! Records Claude Code as `.ccpa-trace.jsonl` by parsing Anthropic
//! Messages-API exchanges into [`Record`](ccpa_trace::Record) values.
//!
//! This crate is **pure-parser-first**. The HTTPS proxy that captures
//! live exchanges from `ANTHROPIC_BASE_URL` lands in a follow-up PR; the
//! parsing core here is fully testable without network IO and is what
//! `FALSIFY-CCPA-001` asserts roundtrips byte-identical against the trace
//! schema.
//!
//! Source-of-truth contract:
//! `contracts/claude-code-parity-apr-v1.yaml § trace_schema`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod parse;

pub use parse::{parse_messages_request, ParseError};
