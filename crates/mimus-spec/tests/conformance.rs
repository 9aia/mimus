//! File-set conformance tests — tempdir git-repo fixtures.
//! Requires `git` on PATH (mimus is a dev tool; no graceful skip).
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use mimus_spec::{load_specs, repo_name, Spec, SpecSeverity, ViolationKind};

fn git(repo: &Path, args: &[&str]) {
    let st = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()
        .unwrap();
    assert!(st.success(), "git {args:?} failed");
}

/// A tempdir git repo with the given tracked files committed.
fn repo(files: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join(".git")).ok(); // ensure clean slate
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "t@t"]);
    git(root, &["config", "user.name", "t"]);
    for f in files {
        let p = root.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "x").unwrap();
    }
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "init", "--allow-empty"]);
    dir
}

fn kinds(vs: &[mimus_spec::SpecViolation]) -> Vec<ViolationKind> {
    vs.iter().map(|v| v.kind).collect()
}

#[test]
fn extends_and_include_resolve() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    // parent spec — files arrays concat child-first under
    // scarab-compose deep_merge
    std::fs::write(
        d.join("base.toml"),
        r#"
family = "base"
files = ["LICENSE", "README.md"]
dirs  = ["docs"]

[path."LICENSE"]
contains = "MIT"
"#,
    )
    .unwrap();
    std::fs::write(
        d.join("child.toml"),
        r#"
extends = ["base.toml"]
family = "child"
files  = ["Cargo.toml"]
severity = "warn"
"#,
    )
    .unwrap();
    let s = Spec::load(&d.join("child.toml")).unwrap();
    assert_eq!(s.family, "child"); // child wins scalars
    assert_eq!(s.severity, SpecSeverity::Warn);
    // arrays concatenate — union coverage
    assert!(s.files.contains(&"Cargo.toml".to_string()));
    assert!(s.files.contains(&"LICENSE".to_string()));
    assert!(s.dirs.contains(&"docs".to_string()));
    // parent's path table survives the merge
    assert!(s.path.contains_key("LICENSE"));
}

#[test]
fn include_fragment_merges() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    // include: fragments parse as YAML regardless of extension
    // (scarab-compose resolve_include_dict) — use .yaml here.
    std::fs::write(d.join("frag.yaml"), "files:\n  - A.md\n").unwrap();
    std::fs::write(
        d.join("spec.toml"),
        "family = \"inc\"\ninclude = \"frag.yaml\"\nfiles = [\"B.md\"]\n",
    )
    .unwrap();
    let s = Spec::load(&d.join("spec.toml")).unwrap();
    // include merges under local keys — frag's files concat with
    // the local ones
    assert!(s.files.contains(&"A.md".to_string()));
    assert!(s.files.contains(&"B.md".to_string()));
}

#[test]
fn missing_and_unlisted() {
    let r = repo(&["Cargo.toml", "README.md", "src/lib.rs", "src/main.rs"]);
    let s = Spec::parse(
        r#"
family = "t"
files  = ["Cargo.toml", "README.md", "LICENSE", "src/**/*.rs"]
dirs   = ["src"]
"#,
    )
    .unwrap();
    let vs = s.check_repo(r.path()).unwrap();
    assert_eq!(kinds(&vs), vec![ViolationKind::Missing]);
    assert_eq!(vs[0].path, "LICENSE");
    assert_eq!(vs[0].severity, mimus_core::Severity::Error);
    assert_eq!(vs[0].spec, "t");
}

#[test]
fn unlisted_flags_uncovered_tracked_file() {
    let r = repo(&["Cargo.toml", "README.md", "src/lib.rs", "docs/extra.md"]);
    let s =
        Spec::parse("family='t'\nfiles=['Cargo.toml','README.md','src/**/*.rs']\ndirs=['src']\n")
            .unwrap();
    let vs = s.check_repo(r.path()).unwrap();
    assert_eq!(vs.len(), 1);
    assert_eq!(vs[0].kind, ViolationKind::Unlisted);
    assert_eq!(vs[0].path, "docs/extra.md");
}

#[test]
fn dirs_cover_nested_files() {
    let r = repo(&["Cargo.toml", "xtask/src/main.rs", "xtask/Cargo.toml"]);
    let s = Spec::parse("family='t'\nfiles=['Cargo.toml']\ndirs=['xtask']\n").unwrap();
    // xtask dir itself exists; everything beneath is covered
    assert!(s.check_repo(r.path()).unwrap().is_empty());
}

#[test]
fn missing_dir_flagged() {
    let r = repo(&["Cargo.toml"]);
    let s = Spec::parse("family='t'\nfiles=['Cargo.toml']\ndirs=['tests']\n").unwrap();
    let vs = s.check_repo(r.path()).unwrap();
    assert_eq!(kinds(&vs), vec![ViolationKind::Missing]);
    assert_eq!(vs[0].path, "tests");
}

#[test]
fn warn_severity_maps() {
    let r = repo(&["Cargo.toml", "stray.txt"]);
    let s = Spec::parse("family='t'\nseverity='warn'\nfiles=['Cargo.toml']\n").unwrap();
    let vs = s.check_repo(r.path()).unwrap();
    assert_eq!(vs.len(), 1);
    assert_eq!(vs[0].severity, mimus_core::Severity::Warn);
}

#[test]
fn untracked_files_are_invisible() {
    // git ls-files sees only tracked files — an untracked stray
    // must NOT produce `unlisted` (and doesn't satisfy `missing`
    // either way since missing checks disk presence).
    let r = repo(&["Cargo.toml"]);
    std::fs::write(r.path().join("stray.txt"), "x").unwrap();
    let s = Spec::parse("family='t'\nfiles=['Cargo.toml']\n").unwrap();
    assert!(s.check_repo(r.path()).unwrap().is_empty());
}

#[test]
fn non_git_dir_falls_back_to_walk() {
    // no .git — filesystem walk counts every file (untracked too)
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "x").unwrap();
    std::fs::create_dir_all(dir.path().join(".git/objects")).unwrap();
    std::fs::write(dir.path().join(".git/HEAD"), "ref").unwrap();
    let s = Spec::parse("family='t'\nfiles=['a.txt']\n").unwrap();
    let vs = s.check_repo(dir.path()).unwrap();
    // walk sees a.txt (covered) and skips .git entirely
    assert!(vs.is_empty(), "{vs:?}");
}

#[test]
fn repo_name_and_load_specs_dir() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    std::fs::write(
        d.join("engine-a.toml"),
        "family='a'\nmatch=['engine-*']\nfiles=['x']\n",
    )
    .unwrap();
    std::fs::write(d.join("other.yaml"), "family: b\nfiles: [y]\n").unwrap();
    std::fs::write(d.join("skip.txt"), "not a spec").unwrap();

    let specs = load_specs(d).unwrap();
    assert_eq!(specs.len(), 2);

    // repo-name matching picks the right spec — name a dir
    // `engine-foo` so repo_name() has a stable subject
    let named = tempfile::tempdir().unwrap();
    let engine = named.path().join("engine-foo");
    std::fs::create_dir_all(&engine).unwrap();
    assert_eq!(repo_name(&engine), "engine-foo");
    let matched: Vec<_> = specs
        .iter()
        .filter(|s| s.matches_repo(&repo_name(&engine)))
        .map(|s| s.family.as_str())
        .collect();
    // engine-*.toml matches; other.yaml has no `match` → applies to all
    assert_eq!(matched.len(), 2);
}

#[test]
fn bad_glob_errors() {
    let s = Spec::parse("family='t'\nfiles=['[unclosed']\n").unwrap();
    let r = repo(&["a"]);
    assert!(s.check_repo(r.path()).is_err());
}

// --- phase-2 predicate tier -------------------------------------

fn predicate_violations(vs: &[mimus_spec::SpecViolation]) -> Vec<&mimus_spec::SpecViolation> {
    vs.iter()
        .filter(|v| v.kind == ViolationKind::Predicate)
        .collect()
}

#[test]
fn path_contains_hit_and_miss() {
    let r = repo(&["LICENSE", "README.md"]);
    std::fs::write(r.path().join("LICENSE"), "MIT License\n").unwrap();
    let s = Spec::parse(
        r#"
family = "t"
files  = ["LICENSE", "README.md"]

[path."LICENSE"]
contains = "MIT"

[path."README.md"]
contains = "nonexistent-substring"
"#,
    )
    .unwrap();
    let vs = s.check_repo(r.path()).unwrap();
    let pv = predicate_violations(&vs);
    assert_eq!(pv.len(), 1, "{vs:?}");
    assert_eq!(pv[0].path, "README.md");
    assert_eq!(pv[0].severity, mimus_core::Severity::Error);
    assert_eq!(pv[0].spec, "t");
}

#[test]
fn path_exists_true_and_false() {
    let r = repo(&["present.txt"]);
    std::fs::write(r.path().join("stray-untracked.txt"), "x").unwrap();
    let s = Spec::parse(
        r#"
family = "t"
files  = ["present.txt"]

[path."present.txt"]
exists = true

[path."stray-untracked.txt"]
exists = false

[path."absent.txt"]
exists = false
"#,
    )
    .unwrap();
    let vs = s.check_repo(r.path()).unwrap();
    let pv = predicate_violations(&vs);
    // exists=false fires on the untracked-but-present file; the
    // absent path is satisfied (no violation, no missing either)
    assert_eq!(pv.len(), 1, "{vs:?}");
    assert_eq!(pv[0].path, "stray-untracked.txt");
    assert!(pv.iter().all(|v| v.path != "absent.txt"));
}

#[test]
fn path_regex_require_and_forbid() {
    let r = repo(&["Cargo.toml"]);
    std::fs::write(r.path().join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
    let s = Spec::parse(
        r#"
family = "t"
files  = ["Cargo.toml", "deny.toml"]

[path."Cargo.toml"]
regex = '^\[package\]'

[path."deny.toml"]
regex.forbid = ['secret']
regex.require = ['allowed']
"#,
    )
    .unwrap();
    std::fs::write(r.path().join("deny.toml"), "allowed\n").unwrap();
    git(r.path(), &["add", "-A"]);
    git(r.path(), &["commit", "-qm", "add deny"]);

    let vs = s.check_repo(r.path()).unwrap();
    assert!(vs.is_empty(), "{vs:?}");

    // now violate both: forbid hit + require miss
    std::fs::write(r.path().join("deny.toml"), "secret\n").unwrap();
    let vs = s.check_repo(r.path()).unwrap();
    let pv = predicate_violations(&vs);
    assert_eq!(pv.len(), 2, "{vs:?}");
    assert!(pv.iter().all(|v| v.path == "deny.toml"));
}

#[test]
fn path_regex_string_form_miss() {
    let r = repo(&["a.txt"]);
    std::fs::write(r.path().join("a.txt"), "hello\n").unwrap();
    let s = Spec::parse(
        r#"
family = "t"
files  = ["a.txt"]

[path."a.txt"]
regex = '^goodbye'
"#,
    )
    .unwrap();
    let vs = s.check_repo(r.path()).unwrap();
    let pv = predicate_violations(&vs);
    assert_eq!(pv.len(), 1, "{vs:?}");
    assert_eq!(pv[0].path, "a.txt");
}

#[test]
fn path_predicates_skip_absent_files() {
    // a path-table-only absent file produces nothing — no missing,
    // no predicate noise (exists=false is satisfied by absence)
    let r = repo(&["a.txt"]);
    let s = Spec::parse(
        r#"
family = "t"
files  = ["a.txt"]
cover  = ["**"]

[path."ghost.txt"]
exists = true
contains = "x"
regex.require = ['y']
"#,
    )
    .unwrap();
    let vs = s.check_repo(r.path()).unwrap();
    assert!(vs.is_empty(), "{vs:?}");
}

#[test]
fn path_except_skips_matching_repo_name() {
    // `except` is the per-repo exemption hatch: a shared base spec's
    // path predicates skip repos whose directory name matches.
    let dir = tempfile::tempdir().unwrap();
    for name in ["docs", "app"] {
        let repo = dir.path().join(name);
        std::fs::create_dir(&repo).unwrap();
        std::fs::write(repo.join("bupha.toml"), "cmd = \"bash ci.sh\"\n").unwrap();
    }
    let s = Spec::parse(
        r#"
family = "t"
files  = ["bupha.toml"]

[path."bupha.toml"]
regex  = "peregrin campaign"
except = ["docs"]
"#,
    )
    .unwrap();
    // "docs" is exempt — the stale-runner regex never evaluates
    assert!(
        s.check_repo(&dir.path().join("docs")).unwrap().is_empty(),
        "exempt repo must report no violations"
    );
    // "app" is not — the require-regex fires
    let vs = s.check_repo(&dir.path().join("app")).unwrap();
    let pv = predicate_violations(&vs);
    assert_eq!(pv.len(), 1, "{vs:?}");
    assert_eq!(pv[0].path, "bupha.toml");
}

#[test]
fn path_except_bad_shape_errors() {
    let r = repo(&["a.txt"]);
    for table in [
        "except = \"docs\"",                // not an array
        "except = [1]",                     // non-string entry
        "except = [\"docs\"]\nunknown = 1", // unknown key still errors
    ] {
        let s = Spec::parse(&format!(
            "family = \"t\"\nfiles = [\"a.txt\"]\n\n[path.\"a.txt\"]\n{table}\n"
        ))
        .unwrap();
        assert!(s.check_repo(r.path()).is_err(), "{table}");
    }
}

#[test]
fn unknown_path_table_key_errors() {
    let r = repo(&["a.txt"]);
    let s = Spec::parse(
        r#"
family = "t"
files  = ["a.txt"]

[path."a.txt"]
manifest.license = "MIT"
"#,
    )
    .unwrap();
    // load succeeds (forward-compat) — the typo surfaces at check
    let err = s.check_repo(r.path()).unwrap_err();
    match err {
        mimus_spec::SpecError::Predicate { path, key } => {
            assert_eq!(path, "a.txt");
            assert_eq!(key, "manifest");
        }
        other => panic!("expected Predicate error, got {other}"),
    }
}

#[test]
fn unknown_regex_subkey_errors() {
    let r = repo(&["a.txt"]);
    let s = Spec::parse(
        r#"
family = "t"
files  = ["a.txt"]

[path."a.txt"]
regex.requrie = ['x']
"#,
    )
    .unwrap();
    let err = s.check_repo(r.path()).unwrap_err();
    assert!(matches!(err, mimus_spec::SpecError::Predicate { .. }));
}

#[test]
fn bad_regex_pattern_errors() {
    let r = repo(&["a.txt"]);
    let s = Spec::parse(
        r#"
family = "t"
files  = ["a.txt"]

[path."a.txt"]
regex = '[unclosed'
"#,
    )
    .unwrap();
    assert!(s.check_repo(r.path()).is_err());
}

#[test]
fn git_untracked_flags_untracked_files() {
    let r = repo(&["Cargo.toml", ".gitignore"]);
    std::fs::write(r.path().join(".gitignore"), "ignored/\n").unwrap();
    git(r.path(), &["add", "-A"]);
    git(r.path(), &["commit", "-qm", "gitignore"]);
    // untracked: matches pattern; ignored/: excluded by
    // --exclude-standard; tracked: never listed
    std::fs::write(r.path().join("notes.scratch"), "x").unwrap();
    std::fs::create_dir_all(r.path().join("ignored")).unwrap();
    std::fs::write(r.path().join("ignored/junk.txt"), "x").unwrap();

    let s = Spec::parse(
        r#"
family = "t"
files  = ["Cargo.toml", ".gitignore"]

git.untracked = ["*.scratch"]
"#,
    )
    .unwrap();
    let vs = s.check_repo(r.path()).unwrap();
    let pv = predicate_violations(&vs);
    assert_eq!(pv.len(), 1, "{vs:?}");
    assert_eq!(pv[0].path, "notes.scratch");
    assert_eq!(pv[0].kind, ViolationKind::Predicate);
}

#[test]
fn git_untracked_skips_non_git_repo() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "x").unwrap();
    let s = Spec::parse(
        r#"
family = "t"
files  = ["a.txt"]
cover  = ["**"]

git.untracked = ["*"]
"#,
    )
    .unwrap();
    // non-git → git ls-files fails → predicate tier skips silently
    let vs = s.check_repo(dir.path()).unwrap();
    assert!(vs.is_empty(), "{vs:?}");
}

#[test]
fn warn_severity_applies_to_predicates() {
    let r = repo(&["a.txt"]);
    let s = Spec::parse(
        r#"
family = "t"
severity = "warn"
files  = ["a.txt"]
cover  = ["**"]

[path."a.txt"]
contains = "absent"
"#,
    )
    .unwrap();
    let vs = s.check_repo(r.path()).unwrap();
    let pv = predicate_violations(&vs);
    assert_eq!(pv.len(), 1);
    assert_eq!(pv[0].severity, mimus_core::Severity::Warn);
}
