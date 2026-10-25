//! CmdChecker: PATH probing, builtins, declared binaries.

use mimus_check::Checker;
use mimus_check_cmd::CmdChecker;
use mimus_core::{Claim, ClaimKind, Severity};
use std::path::PathBuf;

fn claim(text: &str) -> Claim {
    Claim {
        kind: ClaimKind::Command,
        text: text.into(),
        source: PathBuf::from("d.md"),
        line: 1,
    }
}

#[test]
fn handles_and_default() {
    let c = CmdChecker::default();
    assert_eq!(c.handles(), &[ClaimKind::Command]);
    assert!(!c.allow_exec);
    assert!(c.declared.is_empty());
}

#[test]
fn empty_and_builtins_are_silence() {
    let c = CmdChecker::new();
    assert!(c.check(&claim("   ")).unwrap().is_none());
    for t in [
        "cd x",
        "echo hi",
        "export A=1",
        "set -e",
        "if x",
        "for f",
        "while x",
        "test -f a",
        "true",
        "false",
    ] {
        assert!(c.check(&claim(t)).unwrap().is_none(), "{t}");
    }
    // backtick-wrapped builtin token is trimmed before matching
    assert!(c.check(&claim("`cd`")).unwrap().is_none());
}

#[test]
fn declared_binary_skips_probe() {
    let mut c = CmdChecker::new();
    c.declared = vec!["definitely-not-a-real-bin-xyz".into()];
    assert!(c
        .check(&claim("definitely-not-a-real-bin-xyz run"))
        .unwrap()
        .is_none());
}

#[test]
fn real_and_missing_binaries() {
    let c = CmdChecker::new();
    // `sh` exists on any unix PATH
    assert!(c.check(&claim("sh -c true")).unwrap().is_none());
    let v = c
        .check(&claim("definitely-not-a-real-bin-xyz --flag"))
        .unwrap()
        .unwrap();
    assert_eq!(v.severity, Severity::Warn);
    assert!(v.evidence.contains("definitely-not-a-real-bin-xyz"));
}

#[test]
fn slash_binary_resolves_as_path() {
    let c = CmdChecker::new();
    // absolute path that exists
    assert!(c.check(&claim("/bin/sh -c true")).unwrap().is_none());
    // absolute path that doesn't
    let v = c
        .check(&claim("/definitely/missing/bin-x run"))
        .unwrap()
        .unwrap();
    assert_eq!(v.severity, Severity::Warn);
}

/// PATH mutation serializes within this binary (cargo test threads).
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn missing_path_env_is_silence_not_panic() {
    let _g = ENV_LOCK.lock().unwrap();
    // binary_on_path with no PATH variable → unwrap_or(false)
    let dir = std::env::temp_dir().join(format!("mimus-cmd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let stub = dir.join("onlybin");
    std::fs::write(&stub, "#!/bin/sh\n").unwrap();
    let old = std::env::var_os("PATH");
    std::env::set_var("PATH", &dir);
    let c = CmdChecker::new();
    // present in the (tiny) PATH → verified
    assert!(c.check(&claim("onlybin x")).unwrap().is_none());
    // absent → violation
    assert!(c.check(&claim("otherbin x")).unwrap().is_some());
    std::env::remove_var("PATH");
    // PATH unset → false → violation
    assert!(c.check(&claim("onlybin x")).unwrap().is_some());
    match old {
        Some(p) => std::env::set_var("PATH", p),
        None => {}
    }
    let _ = std::fs::remove_dir_all(&dir);
}
