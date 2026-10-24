//! FsChecker branches not exercised by the inline unit tests:
//! http/env/placeholder silence, glob claims, base_dirs.

use mimus_check::Checker;
use mimus_check_fs::FsChecker;
use mimus_core::{Claim, ClaimKind, Severity};
use std::path::PathBuf;

fn claim(text: &str) -> Claim {
    Claim {
        kind: ClaimKind::Path,
        text: text.into(),
        source: PathBuf::from("docs/d.md"),
        line: 1,
    }
}

#[test]
fn http_and_empty_are_silence() {
    let c = FsChecker::new(".");
    assert_eq!(c.handles(), &[ClaimKind::Path]);
    assert!(c.check(&claim("")).unwrap().is_none());
    assert!(c.check(&claim("``")).unwrap().is_none());
    assert!(c.check(&claim("http://example.com/x")).unwrap().is_none());
    assert!(c.check(&claim("https://example.com")).unwrap().is_none());
}

#[test]
fn env_and_placeholder_are_silence() {
    let c = FsChecker::new(".");
    assert!(c.check(&claim("~/some/path")).unwrap().is_none());
    assert!(c.check(&claim("$HOME/x")).unwrap().is_none());
    assert!(c.check(&claim("src/<name>.rs")).unwrap().is_none());
    assert!(c.check(&claim("a/>b")).unwrap().is_none());
}

#[test]
fn glob_claims_verify_by_expansion() {
    let dir = std::env::temp_dir().join(format!("mimus-fs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("crates/x")).unwrap();
    std::fs::write(dir.join("crates/x/lib.rs"), "").unwrap();
    let c = FsChecker::new(&dir);
    // matching glob → verified
    assert!(c.check(&claim("crates/*/lib.rs")).unwrap().is_none());
    // no match → error
    let v = c.check(&claim("crates/*/missing.rs")).unwrap().unwrap();
    assert_eq!(v.severity, Severity::Error);
    assert!(v.evidence.contains("no files match"), "{}", v.evidence);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `[`/`?` glob metachar claims take the expansion path — a
/// pattern glob can't even parse unwraps to "no match" → dead.
#[test]
fn glob_metachars_and_glob_errors() {
    let c = FsChecker::new(".");
    // unclosed `[` → glob::glob errs → unwrap_or(false) → dead
    let v = c.check(&claim("bad[/glob")).unwrap().unwrap();
    assert_eq!(v.severity, Severity::Error);
    // `?` wildcard, no match → dead
    assert!(c.check(&claim("zz?zz/x.md")).unwrap().is_some());
    // `[…]` wildcard, no match → dead
    assert!(c.check(&claim("z[0-9]z/y.md")).unwrap().is_some());
}

/// route_shaped's guard arms: exactly-two-lowercase-segments only.
#[test]
fn route_guard_case_and_arity() {
    let c = FsChecker::new(".");
    // uppercase in second segment → not a route → real dead claim
    assert!(c.check(&claim("tools/List")).unwrap().is_some());
    // uppercase in first segment → not a route → real dead claim
    assert!(c.check(&claim("Tools/list")).unwrap().is_some());
    // three lowercase segments → arity fails → real dead claim
    assert!(c.check(&claim("a/b/c")).unwrap().is_some());
    // `@/` — empty first segment after the scope strip → not a
    // route → real dead claim
    assert!(c.check(&claim("@/x")).unwrap().is_some());
}

/// Leading-slash and route-shaped claims short-circuit to silence
/// before the dead-path verdict — the unit-test copy covers these,
/// the linked lib record needs them here.
#[test]
fn leading_slash_and_route_claims_are_silence() {
    let c = FsChecker::new(".");
    // `/…` reads as a URL route or host-absolute path — unprovable
    for t in ["/channel", "/etc/passwd"] {
        assert!(c.check(&claim(t)).unwrap().is_none(), "{t}");
    }
    // two lowercase segments, no extension — route/MIME/method shape
    for t in ["application/pdf", "tools/list", "resources/read"] {
        assert!(c.check(&claim(t)).unwrap().is_none(), "{t}");
    }
}

#[test]
fn source_without_parent_dir() {
    // claim.source "/" has no parent — the doc-dir base is skipped
    let c = FsChecker::new(".");
    let mut cl = claim("definitely-dead-9f3a.md");
    cl.source = "/".into();
    assert!(c.check(&cl).unwrap().is_some());
}

#[test]
fn trailing_slash_and_backticks_trimmed() {
    let c = FsChecker::new(".");
    // `d.md/` → "d.md" — exists in this crate dir? no → but 'd.md'
    // is two segments? no — single segment, real claim → dead.
    // Use a path that exists: this crate's own Cargo.toml.
    assert!(c.check(&claim("Cargo.toml/")).unwrap().is_none());
    assert!(c.check(&claim("`Cargo.toml`")).unwrap().is_none());
}
