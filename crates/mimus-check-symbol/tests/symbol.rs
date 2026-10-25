//! SymbolChecker against a real sqlite kuda index (fixture built
//! via the `sqlite3` CLI — the index schema is the contract).

use mimus_check::Checker;
use mimus_check_symbol::{all_symbols, SymbolChecker};
use mimus_core::{Claim, ClaimKind, Severity};
use std::path::{Path, PathBuf};
use std::process::Command;

fn tmp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("mimus-sym-{}-{tag}", std::process::id()))
}

fn claim(text: &str) -> Claim {
    Claim {
        kind: ClaimKind::Symbol,
        text: text.into(),
        source: PathBuf::from("d.md"),
        line: 1,
    }
}

/// Build `.kuda`-shaped index at `path` with the given rows.
fn index(path: &Path, rows: &[&str]) {
    let sql = format!(
        "CREATE TABLE symbols(name TEXT, kind TEXT, file TEXT, line INTEGER, module TEXT);{}",
        rows.iter()
            .map(|r| format!("INSERT INTO symbols VALUES({r});"))
            .collect::<String>()
    );
    let o = Command::new("sqlite3")
        .arg(path)
        .arg(&sql)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}

#[test]
fn no_index_is_silence() {
    let c = SymbolChecker::new(tmp("gone").join("no-parent/index.sqlite"));
    assert!(!c.index_present());
    assert!(c.check(&claim("x::AnySym")).unwrap().is_none());
}

#[test]
fn all_symbols_and_index_present() {
    let dir = tmp("ok");
    std::fs::create_dir_all(&dir).unwrap();
    let idx = dir.join("index.sqlite");
    index(
        &idx,
        &[
            "'MyFunc','fn','src/lib.rs',5,'mimus'",
            "'other_sym','fn','src/util.rs',9,'mimus-core'",
        ],
    );
    let c = SymbolChecker::new(&idx);
    assert!(c.index_present());
    assert_eq!(c.handles(), &[ClaimKind::Symbol]);
    let syms = all_symbols(&idx).unwrap();
    assert_eq!(syms.len(), 2);
    assert!(syms.contains(&(
        "MyFunc".into(),
        "fn".into(),
        "src/lib.rs".into(),
        "mimus".into()
    )));
    // all_symbols on an unreadable path errors
    assert!(all_symbols(&dir.join("nope/x.sqlite")).is_err());
    // a db without the symbols table errors on prepare
    let bad = dir.join("bad.sqlite");
    let o = Command::new("sqlite3")
        .arg(&bad)
        .arg("CREATE TABLE other(x)")
        .output()
        .unwrap();
    assert!(o.status.success());
    assert!(all_symbols(&bad).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bare_ident_found_and_absent() {
    let dir = tmp("bare");
    std::fs::create_dir_all(&dir).unwrap();
    let idx = dir.join("i.sqlite");
    index(&idx, &["'MyFunc','fn','src/lib.rs',5,'mimus'"]);
    let c = SymbolChecker::new(&idx);
    // bare ident found → verified
    assert!(c.check(&claim("MyFunc")).unwrap().is_none());
    // whitespace around the text is trimmed
    assert!(c.check(&claim("  MyFunc  ")).unwrap().is_none());
    // bare ident absent → silence (could be external)
    assert!(c.check(&claim("NoSuchSym99")).unwrap().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn namespaced_paths() {
    let dir = tmp("ns");
    std::fs::create_dir_all(&dir).unwrap();
    let idx = dir.join("i.sqlite");
    index(&idx, &["'MyFunc','fn','src/lib.rs',5,'mimus-core'"]);
    let c = SymbolChecker::new(&idx);
    // ns matches the file path → verified
    assert!(c.check(&claim("src::MyFunc")).unwrap().is_none());
    // ns matches the module name (dash → underscore) → verified
    assert!(c.check(&claim("mimus_core::MyFunc")).unwrap().is_none());
    // ns disagrees → refuted with the real location + fix
    let v = c.check(&claim("elsewhere::MyFunc")).unwrap().unwrap();
    assert_eq!(v.severity, Severity::Error);
    assert!(v.evidence.contains("src/lib.rs:5"), "{}", v.evidence);
    assert_eq!(v.fix.as_deref(), Some("`src::lib::MyFunc`"));
    // namespaced and absent → refuted
    let v = c.check(&claim("x::NoSuchSym")).unwrap().unwrap();
    assert_eq!(v.severity, Severity::Error);
    assert!(v.evidence.contains("not found"), "{}", v.evidence);
    assert!(v.fix.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

/// NULL column values hit every `r.get(n)?` arm — collect() stops
/// at the first row error, so each column needs its own index.
#[test]
fn null_columns_error_row_decoding() {
    for (tag, row) in [
        ("n0", "NULL,'fn','f',1,'m'"),
        ("n1", "'a',NULL,'f',1,'m'"),
        ("n2", "'a','fn',NULL,1,'m'"),
        ("n3", "'a','fn','f',1,NULL"),
    ] {
        let dir = tmp(tag);
        std::fs::create_dir_all(&dir).unwrap();
        let idx = dir.join("i.sqlite");
        index(&idx, &[row]);
        assert!(all_symbols(&idx).is_err(), "{tag}");
        let _ = std::fs::remove_dir_all(&dir);
    }
    // check()'s decode path: flatten() skips bad rows rather than
    // erroring — three NULL-column rows for one name cover every
    // r.get? arm and still leave `x::S` "not found" → refuted
    let dir = tmp("nullhit");
    std::fs::create_dir_all(&dir).unwrap();
    let idx = dir.join("i.sqlite");
    index(
        &idx,
        &[
            "'S','fn',NULL,1,'m'",
            "'S','fn','f',NULL,'m'",
            "'S','fn','f',1,NULL",
        ],
    );
    let c = SymbolChecker::new(&idx);
    assert!(c.check(&claim("x::S")).unwrap().is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unusable_index_errors_per_claim() {
    let dir = tmp("bad");
    std::fs::create_dir_all(&dir).unwrap();
    let idx = dir.join("i.sqlite");
    let o = Command::new("sqlite3")
        .arg(&idx)
        .arg("CREATE TABLE other(x)")
        .output()
        .unwrap();
    assert!(o.status.success());
    let c = SymbolChecker::new(&idx);
    assert!(c.index_present()); // opens fine…
    assert!(c.check(&claim("SomeSym")).is_err()); // …but has no schema
    let _ = std::fs::remove_dir_all(&dir);
}
