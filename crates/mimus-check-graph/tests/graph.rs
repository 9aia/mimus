//! Index-load + index_age against real sqlite files (fixtures
//! built via the `sqlite3` CLI — the index schema is the
//! contract), plus the run_set pass end-to-end on a loaded Graph.

use mimus_check::GraphChecker;
use mimus_check_graph::{index_age, load_graph, IndexEdgeChecker};
use mimus_core::{Claim, ClaimKind, EdgeKind, Severity};
use std::path::{Path, PathBuf};
use std::process::Command;

fn tmp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("mimus-graph-{}-{tag}", std::process::id()))
}

fn sqlite(path: &Path, sql: &str) {
    let o = Command::new("sqlite3").arg(path).arg(sql).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}

/// The kuda schema (subset the graph pass reads) + row sets.
const SCHEMA: &str =
    "CREATE TABLE symbols(name TEXT, kind TEXT, file TEXT, line INTEGER, module TEXT);\
    CREATE TABLE edges(from_mod TEXT, to_mod TEXT, kind TEXT);\
    CREATE TABLE meta(k TEXT PRIMARY KEY, v TEXT);";

#[test]
fn load_full_index() {
    let dir = tmp("full");
    std::fs::create_dir_all(&dir).unwrap();
    let idx = dir.join("i.sqlite");
    sqlite(
        &idx,
        &format!(
            "{SCHEMA}\
             INSERT INTO symbols VALUES('F','fn','a.rs',1,'alpha');\
             INSERT INTO symbols VALUES('G','fn','b.rs',1,'beta');\
             INSERT INTO edges VALUES('alpha','beta','imports');\
             INSERT INTO edges VALUES('alpha','serde','package-dep');\
             INSERT INTO edges VALUES('alpha','w','workspace-member');\
             INSERT INTO edges VALUES('alpha','z','calls');\
             INSERT INTO edges VALUES('alpha','x','bogus-kind');\
             INSERT INTO meta VALUES('generated_at','1');"
        ),
    );
    let g = load_graph(&idx).unwrap();
    assert_eq!(g.index, idx);
    assert_eq!(g.edges.len(), 5);
    assert_eq!(g.edges[0].kind, EdgeKind::Imports);
    assert_eq!(g.edges[1].kind, EdgeKind::PackageDep);
    assert_eq!(g.edges[2].kind, EdgeKind::WorkspaceMember);
    assert_eq!(g.edges[3].kind, EdgeKind::Calls);
    assert_eq!(g.edges[4].kind, EdgeKind::Other); // unknown kind
    assert!(g.modules.contains("alpha") && g.modules.contains("beta"));
    assert!(g.symbols.contains("F") && g.symbols.contains("G"));
    // NULL columns → row skipped by flatten(), not fatal
    let dir = tmp("nulls");
    std::fs::create_dir_all(&dir).unwrap();
    let idx = dir.join("i.sqlite");
    sqlite(
        &idx,
        "CREATE TABLE edges(from_mod TEXT, to_mod TEXT, kind TEXT);\
         CREATE TABLE symbols(name TEXT, module TEXT);\
         INSERT INTO edges VALUES('a','b',NULL);\
         INSERT INTO edges VALUES('a',NULL,'imports');\
         INSERT INTO edges VALUES(NULL,'b','imports');\
         INSERT INTO edges VALUES('a','b','imports');\
         INSERT INTO symbols VALUES(NULL,'m');\
         INSERT INTO symbols VALUES('s',NULL);\
         INSERT INTO symbols VALUES('s','m');",
    );
    let g = load_graph(&idx).unwrap();
    assert_eq!(g.edges.len(), 1);
    assert_eq!(g.symbols.len(), 1);
    assert_eq!(g.modules.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_partial_and_broken_indexes() {
    // symbols-only index (older/minimal schema) → usable graph,
    // no edge checks
    let dir = tmp("partial");
    std::fs::create_dir_all(&dir).unwrap();
    let idx = dir.join("i.sqlite");
    sqlite(
        &idx,
        "CREATE TABLE symbols(name TEXT, kind TEXT, file TEXT, line INTEGER, module TEXT);\
         INSERT INTO symbols VALUES('F','fn','a.rs',1,'alpha');",
    );
    let g = load_graph(&idx).unwrap();
    assert!(g.edges.is_empty());
    assert!(g.modules.contains("alpha"));
    // edges table with wrong columns → prepare errors
    let idx = dir.join("bad-edges.sqlite");
    sqlite(&idx, "CREATE TABLE edges(x);");
    assert!(load_graph(&idx).is_err());
    // symbols table with wrong columns → prepare errors
    let idx = dir.join("bad-syms.sqlite");
    sqlite(&idx, "CREATE TABLE symbols(x);");
    assert!(load_graph(&idx).is_err());
    // a non-sqlite file → first query (table_exists) fails
    let idx = dir.join("garbage.sqlite");
    std::fs::write(&idx, "not a database").unwrap();
    assert!(load_graph(&idx).is_err());
    // missing file → read-only open fails without creating it
    let idx = dir.join("gone.sqlite");
    assert!(load_graph(&idx).is_err());
    assert!(!idx.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// run_set over a loaded graph: stale-reference (error) +
/// unsupported-claim (warn) from real index rows.
#[test]
fn run_set_on_loaded_graph() {
    let dir = tmp("runset");
    std::fs::create_dir_all(&dir).unwrap();
    let idx = dir.join("i.sqlite");
    sqlite(
        &idx,
        &format!(
            "{SCHEMA}\
             INSERT INTO symbols VALUES('F','fn','a.rs',1,'app-cli');\
             INSERT INTO symbols VALUES('G','fn','b.rs',1,'app-core');\
             INSERT INTO symbols VALUES('H','fn','c.rs',1,'app-web');\
             INSERT INTO edges VALUES('app-cli','app-core','imports');\
             INSERT INTO edges VALUES('app-web','app-gone','imports');\
             INSERT INTO edges VALUES('ghost','app-core','imports');"
        ),
    );
    let g = load_graph(&idx).unwrap();
    let claims = [
        // asserts an existing edge → silence
        Claim {
            kind: ClaimKind::Behavior,
            text: "app-cli imports app-core for everything".into(),
            source: PathBuf::from("d.md"),
            line: 1,
        },
        // asserts a missing edge → warn
        Claim {
            kind: ClaimKind::Behavior,
            text: "app-web depends on app-core too".into(),
            source: PathBuf::from("d.md"),
            line: 2,
        },
    ];
    let v = IndexEdgeChecker.run_set(&claims, &g);
    // stale: app-gone (internal 'app-' family) + ghost source
    let errs: Vec<_> = v.iter().filter(|v| v.severity == Severity::Error).collect();
    let warns: Vec<_> = v.iter().filter(|v| v.severity == Severity::Warn).collect();
    assert_eq!(errs.len(), 2, "{v:?}");
    assert_eq!(warns.len(), 1, "{v:?}");
    assert!(errs.iter().any(|v| v.evidence.contains("app-gone")));
    assert!(errs.iter().any(|v| v.evidence.contains("ghost")));
    assert!(warns[0].evidence.contains("app-core"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn index_age_variants() {
    let dir = tmp("age");
    std::fs::create_dir_all(&dir).unwrap();
    // meta.generated_at present → age from the timestamp
    let idx = dir.join("meta.sqlite");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    sqlite(
        &idx,
        &format!(
            "CREATE TABLE meta(k TEXT PRIMARY KEY, v TEXT);\
                  INSERT INTO meta VALUES('generated_at','{}');",
            now - 3600
        ),
    );
    let age = index_age(&idx).unwrap();
    assert!(age.as_secs() >= 3600 && age.as_secs() < 7200, "{age:?}");
    // non-numeric generated_at → falls back to mtime
    let idx = dir.join("badmeta.sqlite");
    sqlite(
        &idx,
        "CREATE TABLE meta(k TEXT PRIMARY KEY, v TEXT);\
         INSERT INTO meta VALUES('generated_at','soon');",
    );
    let age = index_age(&idx).unwrap();
    assert!(age.as_secs() < 3600, "mtime fallback: {age:?}");
    // no meta table → mtime fallback
    let idx = dir.join("nometa.sqlite");
    sqlite(&idx, "CREATE TABLE symbols(x);");
    assert!(index_age(&idx).unwrap().as_secs() < 3600);
    // missing file → None
    assert!(index_age(&dir.join("gone.sqlite")).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
