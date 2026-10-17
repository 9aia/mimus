//! CLI smoke: help renders, `init` scaffolds, `check` on a fixture
//! repo flags a dead path claim.

use std::process::Command;

fn mimus(args: &[&str], dir: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_mimus"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

#[test]
fn help_and_stub() {
    let tmp = std::env::temp_dir();
    let out = mimus(&["--help"], &tmp);
    assert!(out.status.success());
    let out = mimus(&["audit"], &tmp);
    assert_eq!(out.status.code(), Some(2), "stub exit code");
}

#[test]
fn check_flags_dead_path() {
    let tmp = std::env::temp_dir().join(format!("mimus-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("README.md"), "see `nonexistent/dead.rs` now").unwrap();
    assert!(mimus(&["init"], &tmp).status.success());
    let out = mimus(&["check"], &tmp);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("nonexistent/dead.rs"), "{stdout}");
    assert_eq!(out.status.code(), Some(1), "error severity gates");
    std::fs::remove_dir_all(&tmp).ok();
}
