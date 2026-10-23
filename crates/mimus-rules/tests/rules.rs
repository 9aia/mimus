//! Tests for data-driven convention rules: load, eval, glob.

use mimus_rules::{eval_rule, load_rules, matching_files, RuleDef, RuleMatch};
use mimus_core::Severity;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mimus-rules-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn rule() -> RuleDef {
    RuleDef {
        name: "no-todo".into(),
        message: "resolve TODOs".into(),
        severity: Severity::Error,
        files: "src/**/*.rs".into(),
        match_: RuleMatch::SourcePattern {
            pattern: "TODO".into(),
        },
    }
}

#[test]
fn load_rules_missing_dir_is_empty() {
    assert!(load_rules(&scratch("none").join("nope")).unwrap().is_empty());
}

#[test]
fn load_rules_parses_toml_and_skips_other_ext() {
    let dir = scratch("load");
    std::fs::write(
        dir.join("r.toml"),
        "name = \"r\"\nmessage = \"m\"\nfiles = \"*\"\n[match]\nkind = \"source-pattern\"\npattern = \"x\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("not-a-rule.txt"), "name = \"x\"\n").unwrap();
    let rules = load_rules(&dir).unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].name, "r");
    assert_eq!(rules[0].severity, Severity::Warn); // default
}

#[test]
fn load_rules_bad_toml_errors() {
    let dir = scratch("bad");
    std::fs::write(dir.join("r.toml"), "[[[").unwrap();
    assert!(load_rules(&dir).unwrap_err().contains("parse"));
}

#[test]
fn eval_source_pattern_flags_matching_lines() {
    let v = eval_rule(&rule(), PathBuf::from("src/a.rs").as_path(), "ok\n// TODO fix\nfn f(){} // TODO\n");
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].claim.line, 2);
    assert_eq!(v[0].severity, Severity::Error);
    assert_eq!(v[0].evidence, "resolve TODOs");
}

#[test]
fn eval_bad_regex_is_silence() {
    let mut r = rule();
    r.match_ = RuleMatch::SourcePattern { pattern: "([unclosed".into() };
    assert!(eval_rule(&r, PathBuf::from("x").as_path(), "TODO").is_empty());
}

#[test]
fn matching_files_globs_under_root() {
    let dir = scratch("glob");
    std::fs::create_dir_all(dir.join("src/deep")).unwrap();
    std::fs::write(dir.join("src/a.rs"), "").unwrap();
    std::fs::write(dir.join("src/deep/b.rs"), "").unwrap();
    std::fs::write(dir.join("top.md"), "").unwrap();
    let hits = matching_files(&dir, "src/**/*.rs");
    assert_eq!(hits.len(), 2);
    assert!(matching_files(&dir, "*.nothing").is_empty());
}
