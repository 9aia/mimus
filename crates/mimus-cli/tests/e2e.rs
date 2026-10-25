//! CLI smoke: help renders, `init` scaffolds, `check` on a fixture
//! repo flags a dead path claim — plus per-subcommand coverage via
//! temp-dir fixture repos, a stub ACP agent, and sqlite indexes.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn mimus(args: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mimus"))
        .args(args)
        .current_dir(dir)
        .env_remove("MIMUS_CONFIG")
        .output()
        .unwrap()
}

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mimus-e2e-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write(dir: &Path, rel: &str, content: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

/// mimus.toml in `dir`; `check`/`sources` extras appended verbatim.
fn cfg(dir: &Path, extra: &str) {
    write(
        dir,
        "mimus.toml",
        &format!(
            "[sources]\ndocs = [\"docs/**/*.md\", \"README.md\"]\ncomments = false\n[check]\n{extra}[rules]\ndir = \"rules\"\n"
        ),
    );
}

fn sqlite(path: &Path, sql: &str) {
    let o = Command::new("sqlite3").arg(path).arg(sql).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}

fn out(o: &Output) -> (String, String) {
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

#[test]
fn help_and_stub() {
    let tmp = std::env::temp_dir();
    let out = mimus(&["--help"], &tmp);
    assert!(out.status.success());
    // audit is report-tier — no cards is not an error
    let out = mimus(&["audit"], &tmp);
    assert!(out.status.success());
    // a bad --config path is a usage failure
    let out = mimus(
        &["check", "--config", "/definitely/missing/mimus.toml"],
        &tmp,
    );
    assert_eq!(out.status.code(), Some(2), "usage failure exit code");
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

// ---------- init ----------

#[test]
fn init_variants() {
    // already exists → 1
    let d = dir("init-exists");
    write(&d, "mimus.toml", "");
    let o = mimus(&["init"], &d);
    assert_eq!(o.status.code(), Some(1));
    // config path whose parent is a regular file → write fails
    let d = dir("init-badcfg");
    write(&d, "afile", "x");
    let cfgp = d.join("afile/mimus.toml");
    let o = mimus(&["init", "--config", cfgp.to_str().unwrap()], &d);
    assert_eq!(o.status.code(), Some(1));
    // `rules` exists as a file → mkdir fails
    let d = dir("init-rules");
    write(&d, "rules", "x");
    let o = mimus(&["init"], &d);
    assert_eq!(o.status.code(), Some(1));
    let (so, se) = out(&o);
    assert!(se.contains("mkdir rules"), "{so} {se}");
}

// ---------- check / report ----------

#[test]
fn check_json_and_clean() {
    let d = dir("json");
    cfg(&d, "");
    write(&d, "docs/a.md", "see `nonexistent/dead.rs` now");
    let o = mimus(&["check", "--format", "json"], &d);
    let (so, _) = out(&o);
    assert!(
        so.contains("\"nonexistent/dead.rs\"") || so.contains("nonexistent/dead.rs"),
        "{so}"
    );
    assert_eq!(o.status.code(), Some(1));
    // no claims at all → clean
    let d = dir("clean");
    cfg(&d, "");
    write(&d, "docs/a.md", "no claims here");
    let o = mimus(&["check"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("no violations"), "{so}");
    assert_eq!(o.status.code(), Some(0));
    // unloadable config → collect() bails → usage error
    let o = mimus(
        &["check", "--config", "/definitely/missing/x.toml"],
        &dir("check-badcfg"),
    );
    assert_eq!(o.status.code(), Some(2));
}

#[test]
fn check_kinds_and_prefix_filters() {
    let d = dir("filters");
    cfg(
        &d,
        "kinds = [\"path\"]\nignore_prefixes = [\"nonexistent\"]\n",
    );
    write(
        &d,
        "docs/a.md",
        "see `nonexistent/dead.rs` and `MySymbol_x` and `realf/miss.rs`",
    );
    let o = mimus(&["check"], &d);
    let (so, _) = out(&o);
    // prefix-filtered claim gone; symbol claim filtered by kinds;
    // realf/miss.rs survives → error
    assert!(!so.contains("nonexistent/dead.rs"), "{so}");
    assert!(!so.contains("MySymbol_x"), "{so}");
    assert!(so.contains("realf/miss.rs"), "{so}");
    assert_eq!(o.status.code(), Some(1));
}

#[test]
fn check_warns_and_no_cmd() {
    let d = dir("warn");
    cfg(&d, "");
    write(&d, "docs/a.md", "run `cargo test -q` now");
    // normal PATH → binary resolves → verified
    let o = mimus(&["check"], &d);
    assert_eq!(o.status.code(), Some(0));
    // stripped PATH → `cargo` missing → warn, exit 0
    let o = Command::new(env!("CARGO_BIN_EXE_mimus"))
        .args(["check"])
        .current_dir(&d)
        .env_remove("MIMUS_CONFIG")
        .env("PATH", "/definitely-empty")
        .output()
        .unwrap();
    let (so, _) = out(&o);
    assert!(so.contains("warn"), "{so}");
    assert!(so.contains("cargo"), "{so}");
    assert_eq!(o.status.code(), Some(0));
    // --no-cmd skips the cmd checker entirely
    let o = mimus(&["check", "--no-cmd"], &d);
    assert_eq!(o.status.code(), Some(0));
    // declared prerequisite → silence even with a stripped PATH
    let d = dir("declared");
    cfg(&d, "binaries = [\"cargo\"]\n");
    write(&d, "docs/a.md", "run `cargo test -q` now");
    let o = Command::new(env!("CARGO_BIN_EXE_mimus"))
        .args(["check"])
        .current_dir(&d)
        .env_remove("MIMUS_CONFIG")
        .env("PATH", "/definitely-empty")
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0));
    let (so, _) = out(&o);
    assert!(so.contains("no violations"), "{so}");
}

#[test]
fn check_comment_claims_and_noise() {
    let d = dir("comments");
    write(
        &d,
        "mimus.toml",
        "[sources]\ndocs = [\"docs/**/*.md\"]\ncomments = true\n[rules]\ndir = \"rules\"\n",
    );
    write(&d, "docs/a.md", "plain prose, nothing to see");
    // assertive comment → Behavior claim (no checker → silence)
    write(
        &d,
        "src/code.rs",
        "// The engine uses sqlite for all persistence work today\nfn f() {}\n",
    );
    // noise dirs are pruned from comment scanning
    write(
        &d,
        "tests/t.rs",
        "// The tests use sqlite for all persistence work today\n",
    );
    write(
        &d,
        "node_modules/x.ts",
        "// The lib uses sqlite for all persistence work today\n",
    );
    // an unreadable source file is skipped, not fatal
    write(
        &d,
        "src/locked.rs",
        "// The store uses sqlite for all persistence work today\n",
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            d.join("src/locked.rs"),
            std::fs::Permissions::from_mode(0o000),
        )
        .unwrap();
    }
    let o = mimus(&["claims"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("uses sqlite for all persistence"), "{so}");
    assert_eq!(o.status.code(), Some(0));
}

#[test]
fn check_doc_noise_pruning() {
    let d = dir("noise");
    write(
        &d,
        "mimus.toml",
        "[sources]\ndocs = [\"**/*.md\"]\ncomments = false\n[rules]\ndir = \"rules\"\n",
    );
    write(&d, "docs/a.md", "see `realf/miss.md` here");
    write(&d, "target/n.md", "see `deadtwo/x.md` here");
    write(&d, "LICENSE-notes.md", "see `deadthree/x.md` here");
    write(&d, "build.files/n.md", "see `deadfour/x.md` here");
    write(&d, "docs/unreadable.md", "see `deadfive/x.md` here");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            d.join("docs/unreadable.md"),
            std::fs::Permissions::from_mode(0o000),
        )
        .unwrap();
    }
    let o = mimus(&["check"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("realf/miss.md"), "{so}");
    for dead in ["deadtwo", "deadthree", "deadfour", "deadfive"] {
        assert!(!so.contains(dead), "{dead} in {so}");
    }
    assert_eq!(o.status.code(), Some(1));
}

#[test]
fn check_symbol_checker() {
    let d = dir("sym");
    cfg(&d, "kuda_index = \"idx.sqlite\"\n");
    sqlite(
        &d.join("idx.sqlite"),
        "CREATE TABLE symbols(name TEXT, kind TEXT, file TEXT, line INTEGER, module TEXT);\
         INSERT INTO symbols VALUES('MyFunc','fn','src/lib.rs',5,'mimus');",
    );
    write(
        &d,
        "docs/a.md",
        "see `MyFunc` and `src::MyFunc` and `elsewhere::MyFunc` and `x::NoSym`",
    );
    let o = mimus(&["check"], &d);
    let (so, _) = out(&o);
    assert_eq!(o.status.code(), Some(1));
    assert!(so.contains("elsewhere::MyFunc"), "{so}");
    assert!(so.contains("x::NoSym"), "{so}");
    assert!(so.contains("fix:"), "{so}");
    // the verified claims produce no violation lines of their own
    assert!(!so.contains("`MyFunc` —"), "{so}");
    assert!(!so.contains("`src::MyFunc` —"), "{so}");

    // schema-broken index → per-claim checker error on stderr
    let d = dir("symerr");
    cfg(&d, "kuda_index = \"idx.sqlite\"\n");
    sqlite(&d.join("idx.sqlite"), "CREATE TABLE other(x);");
    write(&d, "docs/a.md", "see `MyFunc` here");
    let o = mimus(&["check"], &d);
    let (_, se) = out(&o);
    assert!(se.contains("checker:"), "{se}");
    assert_eq!(o.status.code(), Some(0));
}

#[test]
fn check_rules_eval() {
    let d = dir("rules");
    cfg(&d, "");
    write(
        &d,
        "rules/no-todo.toml",
        "name = \"no-todo\"\nmessage = \"resolve TODOs\"\nseverity = \"error\"\nfiles = \"src/**/*.rs\"\n[match]\nkind = \"source-pattern\"\npattern = \"TODO\"\n",
    );
    write(&d, "src/a.rs", "fn f() {} // TODO later\n");
    write(&d, "docs/a.md", "nothing claimed here");
    // an unreadable file matched by the rule glob is skipped,
    // not fatal — the read_to_string Err arm
    write(&d, "src/locked.rs", "// TODO here too\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            d.join("src/locked.rs"),
            std::fs::Permissions::from_mode(0o000),
        )
        .unwrap();
    }
    let o = mimus(&["check"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("resolve TODOs"), "{so}");
    assert_eq!(o.status.code(), Some(1));
}

#[test]
fn report_lists_violations() {
    let d = dir("report");
    cfg(&d, "");
    write(&d, "docs/a.md", "see `nonexistent/dead.rs` now");
    let o = mimus(&["report"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("nonexistent/dead.rs"), "{so}");
    assert_eq!(o.status.code(), Some(1));
}

// ---------- claims ----------

#[test]
fn claims_dump_with_cards() {
    let d = dir("claims");
    cfg(&d, "");
    write(&d, "docs/a.md", "see `README.md` here");
    write(
        &d,
        ".kuda/modules/m.yaml",
        "name: m\npurpose: 'the module does real things'\nnotes:\n  - it stores things carefully\n  second indented note\nnext: done\n",
    );
    write(&d, ".kuda/modules/empty.yaml", "name: e\npurpose: ''\n");
    // unreadable card → skipped by card_prose_claims
    write(
        &d,
        ".kuda/modules/opaque.yaml",
        "name: o\npurpose: 'hidden'\n",
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            d.join(".kuda/modules/opaque.yaml"),
            std::fs::Permissions::from_mode(0o000),
        )
        .unwrap();
    }
    let o = mimus(&["claims", "--all"], &d);
    let (so, se) = out(&o);
    assert!(so.contains("[Path] README.md"), "{so}");
    assert!(so.contains("the module does real things"), "{so}");
    assert!(so.contains("it stores things carefully"), "{so}");
    assert!(so.contains("second indented note"), "{so}");
    assert!(!so.contains("hidden"), "{so}");
    assert!(se.contains("claim(s)"), "{se}");
    assert_eq!(o.status.code(), Some(0));
    // no config → usage error
    let d = dir("claims-nocfg");
    let o = mimus(&["claims"], &d);
    assert_eq!(o.status.code(), Some(1));
}

// ---------- coverage ----------

#[test]
fn coverage_report() {
    // unloadable config → usage error before anything runs
    let o = mimus(
        &["coverage", "--config", "/definitely/missing/x.toml"],
        &dir("cov-badcfg"),
    );
    assert_eq!(o.status.code(), Some(1));
    // no kuda_index configured → advisory error
    let d = dir("cov-noidx");
    cfg(&d, "");
    let o = mimus(&["coverage"], &d);
    let (_, se) = out(&o);
    assert!(se.contains("kuda_index"), "{se}");
    assert_eq!(o.status.code(), Some(1));
    // index path unreadable → error
    let d = dir("cov-badidx");
    cfg(&d, "kuda_index = \".kuda/index.sqlite\"\n");
    let o = mimus(&["coverage"], &d);
    assert_eq!(o.status.code(), Some(1));
    // real index → named/unnamed accounting
    let d = dir("cov-ok");
    cfg(&d, "kuda_index = \"idx.sqlite\"\n");
    let mut rows = String::from(
        "CREATE TABLE symbols(name TEXT, kind TEXT, file TEXT, line INTEGER, module TEXT);\
         INSERT INTO symbols VALUES('DocumentedFunc','fn','src/lib.rs',1,'mymod');\
         INSERT INTO symbols VALUES('SubStr','fn','src/a.rs',2,'mymod');\
         INSERT INTO symbols VALUES('abc','fn','src/a.rs',3,'mymod');\
         INSERT INTO symbols VALUES('new','fn','src/a.rs',4,'mymod');\
         INSERT INTO symbols VALUES('helper_test','fn','src/tests/t.rs',5,'mymod');\
         INSERT INTO symbols VALUES('thing_x','fn','src/a_test.rs',6,'mymod');\
         INSERT INTO symbols VALUES('dup_name','fn','src/a.rs',7,'m1');\
         INSERT INTO symbols VALUES('dup_name','fn','src/b.rs',8,'m2');",
    );
    for i in 0..13 {
        rows.push_str(&format!(
            "INSERT INTO symbols VALUES('uncov_sym{i}','fn','src/u.rs',{i},'mymod');"
        ));
    }
    sqlite(&d.join("idx.sqlite"), &rows);
    write(
        &d,
        "docs/a.md",
        "DocumentedFunc is documented; xxSubStryy inline",
    );
    let o = mimus(&["coverage"], &d);
    let (so, _) = out(&o);
    assert_eq!(o.status.code(), Some(0));
    assert!(so.contains("2/16 symbols named in docs (12%)"), "{so}");
    assert!(so.contains("mymod (fn)"), "{so}");
    assert!(so.contains("+1 more"), "{so}");
    assert!(so.contains("dup_name"), "{so}");
    assert!(!so.contains("helper_test"), "{so}");

    // every symbol filtered out (generic/short/test-file) →
    // total == 0 → the `0%` arm, not a divide-by-zero
    let d = dir("cov-zero");
    cfg(&d, "kuda_index = \"idx.sqlite\"\n");
    sqlite(
        &d.join("idx.sqlite"),
        "CREATE TABLE symbols(name TEXT, kind TEXT, file TEXT, line INTEGER, module TEXT);\
         INSERT INTO symbols VALUES('new','fn','src/lib.rs',1,'m');\
         INSERT INTO symbols VALUES('abc','fn','src/lib.rs',2,'m');\
         INSERT INTO symbols VALUES('helper_fn','fn','src/tests/t.rs',3,'m');",
    );
    write(&d, "docs/a.md", "nothing indexed here");
    // an unreadable doc is skipped by the corpus loop, not fatal
    write(&d, "docs/unreadable.md", "secret stuff here");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            d.join("docs/unreadable.md"),
            std::fs::Permissions::from_mode(0o000),
        )
        .unwrap();
    }
    let o = mimus(&["coverage"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("0/0 symbols named in docs (0%)"), "{so}");
    assert_eq!(o.status.code(), Some(0));
}

// ---------- audit ----------

#[test]
fn audit_scores_cards() {
    let d = dir("audit");
    cfg(&d, "");
    Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .arg(&d)
        .output()
        .unwrap();
    write(&d, "f.rs", "fn f() {}\n");
    let hash = String::from_utf8(
        Command::new("git")
            .arg("-C")
            .arg(&d)
            .args(["hash-object", "f.rs"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    write(
        &d,
        ".kuda/modules/fresh.yaml",
        &format!("name: fresh-mod\nprovenance:\n  files: [\"f.rs\"]\n  hashes: [\"{hash}\"]\n"),
    );
    write(
        &d,
        ".kuda/modules/drifted.yaml",
        "name: drifted-mod\nprovenance:\n  files: [\"f.rs\"]\n  hashes: [\"deadbeef\"]\n",
    );
    write(
        &d,
        ".kuda/modules/contra.yaml",
        "name: contra-mod\nprovenance:\n  files: [\"gone.rs\"]\n  hashes: [\"whatever\"]\n",
    );
    write(&d, ".kuda/modules/broken.yaml", "[[[ not yaml");
    write(&d, ".kuda/modules/bare.yaml", "other: fields only\n");
    let o = mimus(&["audit"], &d);
    let (so, _) = out(&o);
    assert_eq!(o.status.code(), Some(0));
    assert!(so.contains("fresh-mod"), "{so}");
    assert!(so.contains("drifted-mod"), "{so}");
    assert!(so.contains("contradicted contra-mod"), "{so}");
    assert!(so.contains("?"), "{so}");
    assert!(so.contains("2 fresh, 1 drifted, 1 contradicted"), "{so}");

    // git itself unrunnable → hash falls back to "" → drifted
    let d = dir("audit-nogit");
    cfg(&d, "");
    write(&d, "f.rs", "fn f() {}\n");
    write(
        &d,
        ".kuda/modules/x.yaml",
        "name: x\nprovenance:\n  files: [\"f.rs\"]\n  hashes: [\"abc\"]\n",
    );
    let o = Command::new(env!("CARGO_BIN_EXE_mimus"))
        .args(["audit"])
        .current_dir(&d)
        .env_remove("MIMUS_CONFIG")
        .env("PATH", "/definitely-empty")
        .output()
        .unwrap();
    let (so, _) = out(&o);
    assert!(so.contains("drifted"), "{so}");
}

// ---------- doctor ----------

#[test]
fn doctor_variants() {
    // no config at all
    let d = dir("doc-nocfg");
    let o = mimus(&["doctor"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("mimus doctor"), "{so}");
    assert!(so.contains("config   --"), "{so}");
    assert!(so.contains("adapters"), "{so}");
    assert_eq!(o.status.code(), Some(0));
    // config ok, no acp_cmd
    let d = dir("doc-ok");
    cfg(&d, "");
    let o = mimus(&["doctor"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("config   ok"), "{so}");
    // diversity warning: verifier cmd matches the summarizer's
    let d = dir("doc-diverse");
    cfg(&d, "acp_cmd = \"same-agent --acp\"\n");
    write(&d, "kuda.toml", "acp_cmd = \"same-agent --acp\"\n");
    let o = mimus(&["doctor"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("diversity"), "{so}");
    assert!(so.contains("acp        !!"), "{so}"); // stub missing → probe fails
                                                   // a kuda.toml naming a *different* agent → no diversity warning
    let d = dir("doc-divok");
    cfg(&d, "acp_cmd = \"mimus-agent --acp\"\n");
    write(&d, "kuda.toml", "acp_cmd = \"kuda-agent --acp\"\n");
    let o = mimus(&["doctor"], &d);
    let (so, _) = out(&o);
    assert!(!so.contains("diversity"), "{so}");
    // same string but under .kuda/kuda.toml — second search path
    let d = dir("doc-divnested");
    cfg(&d, "acp_cmd = \"same-agent --acp\"\n");
    write(&d, ".kuda/kuda.toml", "acp_cmd = \"same-agent --acp\"\n");
    let o = mimus(&["doctor"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("diversity"), "{so}");
}

// ---------- fix ----------

#[test]
fn fix_mechanical_rewrites() {
    let d = dir("fix");
    cfg(&d, "");
    write(&d, "docs/a.md", "see `old/moved.rs` and `gone/missing.md`");
    write(&d, "sub/moved.rs", "fn moved() {}\n");
    // decoys in pruned dirs must not become candidates
    write(&d, ".git/missing.md", "x");
    write(&d, "node_modules/missing.md", "x");
    // ambiguous basename → flagged
    write(&d, "docs/b.md", "see `old/dup.rs` too");
    write(&d, "a/dup.rs", "");
    write(&d, "b/dup.rs", "");
    // parent-dir-name narrowing picks the unique candidate
    write(&d, "docs/c.md", "see `p1/x.rs` here");
    write(&d, "w/p1/x.rs", "");
    write(&d, "w/p2/x.rs", "");
    // single-segment claim (no parent dir in the claim) with
    // several same-basename hits → can't narrow → flagged
    write(&d, "docs/d.md", "see `dupse.rs` alone");
    write(&d, "a/dupse.rs", "");
    write(&d, "b/dupse.rs", "");
    // `dead/x/..` — a dead path claim with no normal last
    // component → file_name()? gives up early, flagged not fixed
    write(&d, "docs/e.md", "see `dead/x/..` here");
    // a locked dir inside the walk — read_dir errs → skipped
    write(&d, "locked/f.rs", "");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(d.join("locked"), std::fs::Permissions::from_mode(0o000)).unwrap();
    }
    // dry first — plan printed, nothing written
    let o = mimus(&["fix", "--dry"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("`old/moved.rs` -> `sub/moved.rs`"), "{so}");
    assert!(so.contains("(dry)"), "{so}");
    assert!(so.contains("flagged"), "{so}");
    assert!(
        String::from_utf8(std::fs::read(d.join("docs/a.md")).unwrap())
            .unwrap()
            .contains("old/moved.rs")
    );
    // real run — rewrites applied
    let o = mimus(&["fix"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("`old/moved.rs` -> `sub/moved.rs`"), "{so}");
    assert!(so.contains("`p1/x.rs` -> `w/p1/x.rs`"), "{so}");
    assert!(so.contains("flagged for human"), "{so}"); // dupse.rs ambiguous
    assert!(so.contains("applied"), "{so}");
    assert!(
        String::from_utf8(std::fs::read(d.join("docs/a.md")).unwrap())
            .unwrap()
            .contains("sub/moved.rs")
    );
    assert!(
        String::from_utf8(std::fs::read(d.join("docs/c.md")).unwrap())
            .unwrap()
            .contains("w/p1/x.rs")
    );
    // bad config → usage failure
    let d = dir("fix-nocfg");
    let o = mimus(&["fix"], &d);
    assert_eq!(o.status.code(), Some(2));
}

// ---------- semantic tier (fake ACP agent) ----------

/// Minimal JSON-RPC ACP stub — see mimus-check-acp tests.
const ACP_STUB: &str = r#"import json, sys

queue = sys.argv[1]

def send(o):
    sys.stdout.write(json.dumps(o) + "\n")
    sys.stdout.flush()

def pop():
    try:
        data = open(queue).read()
    except OSError:
        sys.exit(0)
    if not data.strip():
        sys.exit(0)
    line, _, rest = data.partition("\n")
    open(queue, "w").write(rest)
    return line.replace("\\n", "\n")

victim = sys.argv[2] if len(sys.argv) > 2 else None
prompts = 0

for line in sys.stdin:
    try:
        msg = json.loads(line)
    except ValueError:
        continue
    mid, method = msg.get("id"), msg.get("method")
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": mid, "result": {"protocolVersion": 1}})
    elif method == "session/new":
        send({"jsonrpc": "2.0", "id": mid, "result": {"sessionId": "s1"}})
    elif method == "session/prompt":
        prompts += 1
        if victim and prompts == 1:
            import os
            try:
                os.unlink(victim)  # gone by the time `fix` reads it back
            except OSError:
                pass
        ans = pop()
        if ans != "NORESULT":
            send({"jsonrpc": "2.0", "method": "session/update",
                  "params": {"update": {"content": {"text": ans}}}})
        send({"jsonrpc": "2.0", "id": mid, "result": {}})
"#;

/// Write stub + queue; returns the acp_cmd string. `victim`, when
/// given, is unlinked by the agent on its second prompt — the
/// window where `fix` re-reads a claim's source.
fn acp_agent(dir: &Path, answers: &[&str]) -> String {
    acp_agent_victim(dir, answers, None)
}

fn acp_agent_victim(dir: &Path, answers: &[&str], victim: Option<&Path>) -> String {
    let stub = dir.join("agent.py");
    std::fs::write(&stub, ACP_STUB).unwrap();
    let queue = dir.join("queue");
    std::fs::write(&queue, answers.join("\n") + "\n").unwrap();
    match victim {
        Some(v) => format!(
            "python3 {} {} {}",
            stub.display(),
            queue.display(),
            v.display()
        ),
        None => format!("python3 {} {}", stub.display(), queue.display()),
    }
}

/// `semantic = true` with no `acp_cmd` — the tier skips silently.
#[test]
fn check_all_without_acp_cmd() {
    let d = dir("sem-nocmd");
    cfg(&d, "semantic = true\n");
    write(&d, "docs/a.md", "plain prose here");
    write(
        &d,
        ".kuda/modules/m.yaml",
        "name: m\npurpose: 'the module stores real things'\n",
    );
    let o = mimus(&["check", "--all"], &d);
    let (so, se) = out(&o);
    assert_eq!(o.status.code(), Some(0), "{so} {se}");
    assert!(so.contains("no violations"), "{so}");
}

/// A malformed rules/*.toml → load_rules errs → rules skipped,
/// check still completes on the mechanical tier.
#[test]
fn check_bad_rules_toml_skipped() {
    let d = dir("badrules");
    cfg(&d, "");
    write(&d, "rules/bad.toml", "[[[ not toml");
    write(&d, "docs/a.md", "nothing claimed here");
    let o = mimus(&["check"], &d);
    assert_eq!(o.status.code(), Some(0));
}

#[test]
fn check_all_runs_semantic_tier() {
    let d = dir("semantic");
    let agent = acp_agent(&d, &["1 VERIFIED"]);
    cfg(&d, &format!("semantic = true\nacp_cmd = \"{agent}\"\n"));
    write(&d, "docs/a.md", "plain prose here");
    write(
        &d,
        ".kuda/modules/m.yaml",
        "name: m\npurpose: 'the module stores real things'\n",
    );
    let o = mimus(&["check", "--all"], &d);
    let (so, se) = out(&o);
    assert_eq!(o.status.code(), Some(0), "{so} {se}");
    assert!(so.contains("no violations"), "{so}");
    // doctor probes the live agent
    let d = dir("doc-probe");
    let agent = acp_agent(&d, &["ok"]);
    cfg(&d, &format!("acp_cmd = \"{agent}\"\n"));
    let o = mimus(&["doctor"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("acp        ok"), "{so}");
}

#[test]
fn fix_semantic_proposes_rewrites() {
    let d = dir("fixsem");
    write(&d, "doc.md", "x"); // grounding target for refute evidence
    let agent = acp_agent(
        &d,
        &[
            "1 REFUTED: doc.md contradicts the first\\n2 REFUTED: doc.md contradicts the second",
            "the module stores cached things",
            "NORESULT",
        ],
    );
    cfg(&d, &format!("semantic = true\nacp_cmd = \"{agent}\"\n"));
    write(&d, "docs/a.md", "nothing here");
    // one card, two claims — order is file order, deterministic:
    // purpose first, then the notes line
    write(&d, ".kuda/modules/m.yaml",
        "name: m\npurpose: 'the module stores real things'\nnotes:\n  - a second false assertion here\n");
    let o = mimus(&["fix", "--semantic"], &d);
    let (so, se) = out(&o);
    assert_eq!(o.status.code(), Some(0), "{so} {se}");
    // first refuted claim → corrected line proposed + applied
    assert!(
        so.contains("`the module stores real things` -> `the module stores cached things`"),
        "{so}"
    );
    assert!(
        String::from_utf8(std::fs::read(d.join(".kuda/modules/m.yaml")).unwrap())
            .unwrap()
            .contains("the module stores cached things")
    );
    // second → agent proposed nothing → error, flagged for human
    assert!(se.contains("propose"), "{se}");
    assert!(so.contains("flagged for human"), "{so}");

    // dry semantic run — one card claim → single-claim path →
    // unnumbered verdict; plan printed with (dry), no write happens
    let d = dir("fixsemdry");
    write(&d, "doc.md", "x");
    let agent = acp_agent(
        &d,
        &[
            "REFUTED: doc.md contradicts it",
            "the module stores cached things",
        ],
    );
    cfg(&d, &format!("semantic = true\nacp_cmd = \"{agent}\"\n"));
    write(&d, "docs/a.md", "nothing here");
    write(
        &d,
        ".kuda/modules/m.yaml",
        "name: m\npurpose: 'the module stores real things'\n",
    );
    let o = mimus(&["fix", "--dry", "--semantic"], &d);
    let (so, se) = out(&o);
    assert_eq!(o.status.code(), Some(0), "{so} {se}");
    assert!(so.contains("(dry)"), "{so}");
    assert!(
        String::from_utf8(std::fs::read(d.join(".kuda/modules/m.yaml")).unwrap())
            .unwrap()
            .contains("the module stores real things")
    );

    // agent proposes a fix but the claim's source vanished between
    // extraction and apply — the re-read fails, no write attempted
    let d = dir("fixsemgone");
    write(&d, "doc.md", "x");
    let card = d.join(".kuda/modules/m.yaml");
    let agent = acp_agent_victim(
        &d,
        &[
            "REFUTED: doc.md contradicts it",
            "the module stores cached things",
        ],
        Some(&card),
    );
    cfg(&d, &format!("semantic = true\nacp_cmd = \"{agent}\"\n"));
    write(&d, "docs/a.md", "nothing here");
    write(
        &d,
        ".kuda/modules/m.yaml",
        "name: m\npurpose: 'the module stores real things'\n",
    );
    let o = mimus(&["fix", "--semantic"], &d);
    let (so, se) = out(&o);
    assert_eq!(o.status.code(), Some(0), "{so} {se}");
    assert!(so.contains("-> `the module stores cached things`"), "{so}");
    assert!(!card.exists());
}

/// `fix --semantic` without acp_cmd — a Warn+Behavior violation
/// (from a rules match) hits the no-agent branch, gets flagged.
#[test]
fn fix_semantic_without_acp_cmd() {
    let d = dir("fixsem-nocmd");
    cfg(&d, "");
    write(
        &d,
        "rules/lint.toml",
        "name = \"lint\"\nmessage = \"resolve this\"\nfiles = \"src/*.rs\"\n[match]\nkind = \"source-pattern\"\npattern = \"TODO\"\n",
    );
    write(&d, "src/a.rs", "fn f() {} // TODO\n");
    write(&d, "docs/a.md", "nothing claimed");
    let o = mimus(&["fix", "--semantic"], &d);
    let (so, se) = out(&o);
    assert_eq!(o.status.code(), Some(0), "{so} {se}");
    assert!(so.contains("flagged for human"), "{so}");
}

#[test]
fn gen_cli_docs() {
    let d = dir("gendocs");
    let o = mimus(&["gen-cli-docs"], &d);
    let (so, _) = out(&o);
    assert_eq!(o.status.code(), Some(0));
    assert!(so.contains("mimus"), "{so}");
}

/// `check --diff SPEC` — only claims in `git diff`-changed files
/// are checked; a dead claim in an untouched file is suppressed.
#[test]
fn check_diff_scopes_to_changed_files() {
    let d = dir("diff");
    cfg(&d, "");
    write(&d, "docs/a.md", "nothing claimed");
    write(&d, "docs/b.md", "see `dead.rs` now");
    let git = |args: &[&str]| {
        let o = Command::new("git")
            .arg("-C")
            .arg(&d)
            .args(args)
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["-c", "user.email=t@t", "-c", "user.name=t", "add", "."]);
    git(&[
        "-c",
        "user.email=t@t",
        "-c",
        "user.name=t",
        "commit",
        "-qm",
        "init",
    ]);
    // working-tree change to a.md only — b.md's dead claim is
    // untouched by the diff and must not be reported.
    write(&d, "docs/a.md", "see `gone.rs` too");
    let o = mimus(&["check", "--diff", "HEAD"], &d);
    let (so, se) = out(&o);
    assert_eq!(o.status.code(), Some(1), "{so} {se}");
    assert!(so.contains("gone.rs"), "{so}");
    assert!(!so.contains("dead.rs"), "{so}");
    // without --diff both fire
    let o = mimus(&["check"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("gone.rs") && so.contains("dead.rs"), "{so}");
}

/// `--diff` outside a git repo is a usage failure, not a crash.
#[test]
fn check_diff_requires_git_repo() {
    let d = dir("diff-nogit");
    cfg(&d, "");
    write(&d, "docs/a.md", "nothing claimed");
    let o = mimus(&["check", "--diff", "HEAD"], &d);
    let (_, se) = out(&o);
    assert_eq!(o.status.code(), Some(2), "{se}");
    assert!(se.contains("git repo"), "{se}");
}

/// A spec git itself rejects is also a usage failure.
#[test]
fn check_diff_bad_spec() {
    let d = dir("diff-badspec");
    cfg(&d, "");
    write(&d, "docs/a.md", "nothing claimed");
    let o = Command::new("git")
        .arg("-C")
        .arg(&d)
        .args(["init", "-q", "-b", "main"])
        .output()
        .unwrap();
    assert!(o.status.success());
    let o = mimus(&["check", "--diff", "bogus..spec"], &d);
    let (_, se) = out(&o);
    assert_eq!(o.status.code(), Some(2), "{se}");
    assert!(se.contains("git diff"), "{se}");
}

/// `git` missing from PATH — the spawn error surfaces cleanly.
#[test]
fn check_diff_no_git_binary() {
    let d = dir("diff-nogitbin");
    cfg(&d, "");
    write(&d, "docs/a.md", "nothing claimed");
    let o = Command::new(env!("CARGO_BIN_EXE_mimus"))
        .args(["check", "--diff", "HEAD"])
        .current_dir(&d)
        .env("PATH", "")
        .env_remove("MIMUS_CONFIG")
        .output()
        .unwrap();
    let (_, se) = out(&o);
    assert_eq!(o.status.code(), Some(2), "{se}");
    assert!(se.contains("git"), "{se}");
}

// ---------- graph tier (ADR-0002) ----------

/// ADR-0002 ph2 — the contradiction pass needs no index: one doc
/// asserting two objects for the same slot warns reflexively with
/// both sites; across two docs it's the cross-source class; and
/// `[check].aliases` folds differently-spelled subjects into one
/// bucket. Warn tier — never gates.
#[test]
fn check_prose_contradictions() {
    // same doc, two objects for `auth|uses` → self-contradiction
    let d = dir("contra-refl");
    cfg(&d, ""); // no kuda_index — the pass runs anyway
    write(
        &d,
        "docs/a.md",
        "The auth service uses jwt for every request today.\n\n\
         The auth service uses sessions for every request today.\n",
    );
    let o = mimus(&["check"], &d);
    let (so, _) = out(&o);
    assert_eq!(o.status.code(), Some(0), "{so}"); // warn never gates
    assert!(so.contains("[warn]"), "{so}");
    assert!(so.contains("self-contradiction"), "{so}");
    assert!(so.contains("auth|uses"), "{so}");
    assert!(so.contains("jwt"), "{so}");
    assert!(so.contains("sessions"), "{so}");
    assert!(so.contains("a.md:1") && so.contains("a.md:3"), "{so}");

    // two docs disagreeing → cross-source contradiction
    let d = dir("contra-cross");
    cfg(&d, "");
    write(
        &d,
        "docs/a.md",
        "The auth service uses jwt for every request today.\n",
    );
    write(
        &d,
        "docs/b.md",
        "The auth service uses sessions for every request today.\n",
    );
    let o = mimus(&["check"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("contradiction"), "{so}");
    assert!(!so.contains("self-contradiction"), "{so}");
    assert!(so.contains("a.md:1") && so.contains("b.md:1"), "{so}");

    // differently-spelled subjects stay silent until an alias
    // folds them into the same bucket
    let d = dir("contra-alias");
    cfg(&d, "[check.aliases]\n\"login flow\" = \"auth\"\n");
    write(
        &d,
        "docs/a.md",
        "The login flow uses jwt for every request today.\n",
    );
    write(
        &d,
        "docs/b.md",
        "The auth service uses sessions for every request today.\n",
    );
    let o = mimus(&["check"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("contradiction"), "{so}");
    assert!(so.contains("auth|uses"), "{so}");
    // no aliases → two buckets → silence
    let d = dir("contra-noalias");
    cfg(&d, "");
    write(
        &d,
        "docs/a.md",
        "The login flow uses jwt for every request today.\n",
    );
    write(
        &d,
        "docs/b.md",
        "The auth service uses sessions for every request today.\n",
    );
    let o = mimus(&["check"], &d);
    let (so, _) = out(&o);
    assert!(!so.contains("contradiction"), "{so}");
}

/// ADR-0002 ph5 — the semantic tier's intake is the mechanical
/// pass's fuzzy residue: two near-duplicate subjects asserting
/// different objects for one relation become a *candidate* claim
/// the ACP checker adjudicates. Not a bucket conflict — the canon
/// subjects differ, so nothing gates.
#[test]
fn check_semantic_candidate_intake() {
    let d = dir("sem-intake");
    write(&d, "doc.md", "x"); // grounding target for refute evidence
                              // one queue line per prompt — the numbered verdicts are escaped
                              // and un-escaped by the stub. Claim 3 is the candidate.
    let agent = acp_agent(
        &d,
        &["1 VERIFIED\\n2 VERIFIED\\n3 REFUTED: doc.md contradicts it"],
    );
    cfg(&d, &format!("semantic = true\nacp_cmd = \"{agent}\"\n"));
    write(
        &d,
        "docs/a.md",
        "The auth flow uses jwt for every request today.\n\n\
         The auth service uses sessions for every request today.\n",
    );
    let o = mimus(&["check", "--all"], &d);
    let (so, se) = out(&o);
    assert_eq!(o.status.code(), Some(0), "{so} {se}");
    // one candidate, right relation, both original spellings, and
    // the ask the agent answers — warn tier, so the run still exits 0
    assert!(so.contains("candidate — uses:"), "{so}");
    assert!(so.contains("The auth service says sessions"), "{so}");
    assert!(so.contains("The auth flow says jwt"), "{so}");
    assert!(so.contains("verify which side the code supports"), "{so}");
    assert!(so.contains("[warn]"), "{so}");
    // the buckets stayed silent: `auth flow` ≠ `auth`, so this pair
    // never became a contradiction — only the agent saw it
    assert!(!so.contains("bucket"), "{so}");
}

/// ADR-0002 ph3 — `--diff` keeps the contradiction buckets
/// whole: the unchanged side of a conflict still counts (the
/// warm cache serves its triples), and unchanged↔unchanged
/// drift stays silent. An unwritable `.mimus/` degrades to a
/// stderr note, never a failure.
#[test]
fn check_diff_prose_warm_cache() {
    let d = dir("diff-cache");
    cfg(&d, "");
    write(
        &d,
        "docs/a.md",
        "The auth service uses jwt for every request today.\n",
    );
    write(
        &d,
        "docs/b.md",
        "The auth service uses sessions for every request today.\n",
    );
    write(
        &d,
        "docs/c.md",
        "The pool service uses postgres everywhere.\n",
    );
    write(&d, "docs/e.md", "The pool service uses redis everywhere.\n");
    let git = |args: &[&str]| {
        let o = Command::new("git")
            .arg("-C")
            .arg(&d)
            .args(args)
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["-c", "user.email=t@t", "-c", "user.name=t", "add", "."]);
    git(&[
        "-c",
        "user.email=t@t",
        "-c",
        "user.name=t",
        "commit",
        "-qm",
        "init",
    ]);
    // warm run — populates .mimus/triples.db
    let o = mimus(&["check"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("contradiction"), "{so}");
    assert!(d.join(".mimus/triples.db").exists());
    // change only a.md: the a↔b conflict emits (jwt→oauth2 proves
    // a.md re-extracted; b.md:1 cited proves the unchanged side
    // still feeds the bucket — via the cache). c↔e is untouched
    // drift → silent.
    write(
        &d,
        "docs/a.md",
        "The auth service uses oauth2 for every request today.\n",
    );
    let o = mimus(&["check", "--diff", "HEAD"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("contradiction"), "{so}");
    assert!(so.contains("oauth2"), "{so}");
    assert!(so.contains("sessions"), "{so}");
    assert!(so.contains("a.md:1 [warn]"), "{so}"); // anchor = changed side
    assert!(so.contains("b.md:1"), "{so}"); // unchanged side cited
    assert!(!so.contains("postgres"), "{so}");
    assert!(!so.contains("redis"), "{so}");

    // .mimus as a file → cache open fails → stderr note + the
    // run completes uncached
    let d = dir("cache-blocked");
    cfg(&d, "");
    write(&d, ".mimus", "x");
    write(&d, "docs/a.md", "nothing claimed here");
    let o = mimus(&["check"], &d);
    let (so, se) = out(&o);
    assert!(se.contains("triples cache"), "{se}");
    assert!(so.contains("no violations"), "{so}");
    assert_eq!(o.status.code(), Some(0));
}

/// doctor reports the triples cache: row count when populated,
/// `--` when absent — and never creates the db itself.
#[test]
fn doctor_triples_cache() {
    let d = dir("doc-cache");
    cfg(&d, "");
    write(
        &d,
        "docs/a.md",
        "The auth service uses jwt for every request today.\n",
    );
    let _ = mimus(&["check"], &d); // populates .mimus/triples.db
    let o = mimus(&["doctor"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("triples cache ok"), "{so}");
    // no db → '--', and the probe doesn't create one
    let d = dir("doc-nocache");
    cfg(&d, "");
    let o = mimus(&["doctor"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("triples cache --"), "{so}");
    assert!(!d.join(".mimus/triples.db").exists());
}

/// `[check].kuda_index` wires the graph pass: a stale index edge
/// gates (error), an unsupported doc claim warns.
#[test]
fn check_graph_violations() {
    let d = dir("graph");
    cfg(&d, "kuda_index = \"idx.sqlite\"\n");
    sqlite(
        &d.join("idx.sqlite"),
        "CREATE TABLE symbols(name TEXT, kind TEXT, file TEXT, line INTEGER, module TEXT);\
         CREATE TABLE edges(from_mod TEXT, to_mod TEXT, kind TEXT);\
         INSERT INTO symbols VALUES('F','fn','a.rs',1,'app-cli');\
         INSERT INTO symbols VALUES('G','fn','b.rs',1,'app-core');\
         INSERT INTO edges VALUES('app-cli','app-gone','imports');",
    );
    // a Behavior claim asserting a missing edge — needs a
    // multi-word assertion sentence for the extractor
    write(
        &d,
        "docs/a.md",
        "app-core uses app-cli in every request path today",
    );
    let o = mimus(&["check"], &d);
    let (so, _) = out(&o);
    assert_eq!(o.status.code(), Some(1), "{so}"); // stale gates
    assert!(so.contains("app-gone"), "{so}");
    assert!(so.contains("[error]"), "{so}");
    assert!(so.contains("app-core"), "{so}");
    assert!(so.contains("[warn]"), "{so}");
    // an unreadable index → stderr note, check still completes
    let d = dir("graph-badidx");
    cfg(&d, "kuda_index = \"idx.sqlite\"\n");
    std::fs::write(d.join("idx.sqlite"), "not a database").unwrap();
    write(&d, "docs/a.md", "nothing claimed here");
    let o = mimus(&["check"], &d);
    let (_, se) = out(&o);
    assert!(se.contains("graph:"), "{se}");
    assert_eq!(o.status.code(), Some(0));
}

/// ADR-0002 ph4 — reverse coverage: a module other modules depend
/// on that no document names warns (never gates); `--diff` reports
/// only modules whose own indexed sources changed.
#[test]
fn check_graph_undocumented_fact() {
    let d = dir("graph-undoc");
    cfg(&d, "kuda_index = \"idx.sqlite\"\n");
    sqlite(
        &d.join("idx.sqlite"),
        "CREATE TABLE symbols(name TEXT, kind TEXT, file TEXT, line INTEGER, module TEXT);\
         CREATE TABLE edges(from_mod TEXT, to_mod TEXT, kind TEXT);\
         INSERT INTO symbols VALUES('F','fn','src/cli.rs',1,'app-cli');\
         INSERT INTO symbols VALUES('G','fn','src/core.rs',1,'app-core');\
         INSERT INTO edges VALUES('app-cli','app-core','imports');",
    );
    write(&d, "src/cli.rs", "fn main() {}\n");
    write(&d, "src/core.rs", "fn core() {}\n");
    // docs name the CLI only — `app-core` is the code→doc gap
    write(
        &d,
        "docs/a.md",
        "app-cli parses every flag before dispatch.\n",
    );
    let o = mimus(&["check"], &d);
    let (so, _) = out(&o);
    assert_eq!(o.status.code(), Some(0), "warn tier never gates: {so}");
    assert!(so.contains("named by no claim"), "{so}");
    assert!(so.contains("app-core"), "{so}");
    assert!(so.contains("src/core.rs:0"), "{so}"); // anchored on code

    // a doc naming the module silences the class
    write(
        &d,
        "docs/b.md",
        "app-core owns the domain model and the index snapshot.\n",
    );
    let o = mimus(&["check"], &d);
    let (so, _) = out(&o);
    assert!(!so.contains("named by no claim"), "{so}");
    // drop the mention again, then commit: --diff scopes the class
    // to modules whose own sources changed
    std::fs::remove_file(d.join("docs/b.md")).unwrap();
    let git = |args: &[&str]| {
        let o = Command::new("git")
            .arg("-C")
            .arg(&d)
            .args(args)
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["-c", "user.email=t@t", "-c", "user.name=t", "add", "."]);
    git(&[
        "-c",
        "user.email=t@t",
        "-c",
        "user.name=t",
        "commit",
        "-qm",
        "init",
    ]);
    // nothing changed → the untouched module stays silent
    let o = mimus(&["check", "--diff", "HEAD"], &d);
    let (so, _) = out(&o);
    assert!(!so.contains("named by no claim"), "{so}");
    // app-core's own source moved → the fact reports
    write(&d, "src/core.rs", "fn core() -> u8 { 1 }\n");
    let o = mimus(&["check", "--diff", "HEAD"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("named by no claim"), "{so}");
    assert!(so.contains("app-core"), "{so}");
}

/// doctor reports index age: fresh → ok, >7d → stale warning,
/// unreadable → '--'.
#[test]
fn doctor_index_age() {
    let d = dir("doc-idx-ok");
    cfg(&d, "kuda_index = \"idx.sqlite\"\n");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    sqlite(
        &d.join("idx.sqlite"),
        &format!(
            "CREATE TABLE meta(k TEXT PRIMARY KEY, v TEXT);\
                  INSERT INTO meta VALUES('generated_at','{now}');"
        ),
    );
    let o = mimus(&["doctor"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("kuda index ok"), "{so}");
    // stale — generated 10 days ago → '!!' warning
    let d = dir("doc-idx-stale");
    cfg(&d, "kuda_index = \"idx.sqlite\"\n");
    sqlite(
        &d.join("idx.sqlite"),
        &format!(
            "CREATE TABLE meta(k TEXT PRIMARY KEY, v TEXT);\
                  INSERT INTO meta VALUES('generated_at','{}');",
            now - 10 * 24 * 3600
        ),
    );
    let o = mimus(&["doctor"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("kuda index !!"), "{so}");
    assert!(so.contains("kuda sync"), "{so}");
    // configured but unreadable → '--' line
    let d = dir("doc-idx-bad");
    cfg(&d, "kuda_index = \".kuda/index.sqlite\"\n");
    let o = mimus(&["doctor"], &d);
    let (so, _) = out(&o);
    assert!(so.contains("kuda index --"), "{so}");
}
