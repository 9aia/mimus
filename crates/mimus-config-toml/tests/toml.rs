//! TomlSource: explicit path, search-order error, parse errors,
//! and the Config defaults model.

use mimus_config::ConfigSource;
use mimus_config_toml::TomlSource;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mimus-toml-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn explicit_path_loads_with_defaults() {
    let p = scratch("ok").join("mimus.toml");
    std::fs::write(&p, "").unwrap();
    let cfg = TomlSource { path: Some(p.to_string_lossy().into_owned()) }.load().unwrap();
    assert!(cfg.check.mechanical);
    assert!(!cfg.check.semantic);
    assert!(cfg.sources.comments);
    assert_eq!(cfg.report.format, "human");
    assert_eq!(cfg.rules.dir, PathBuf::from("rules"));
}

#[test]
fn overrides_apply() {
    let p = scratch("full").join("mimus.toml");
    std::fs::write(
        &p,
        "[check]\nsemantic = true\nkinds = [\"path\", \"link\"]\nignore_prefixes = [\"//\"]\nbinaries = [\"bazel\"]\n\
         [sources]\ndocs = [\"DOC.md\"]\ncomments = false\ncards = \"cards/*.yaml\"\n\
         [rules]\ndir = \"my-rules\"\nstrict = true\n\
         [report]\nformat = \"json\"\nmin_confidence = \"all\"\n",
    ).unwrap();
    let cfg = TomlSource { path: Some(p.to_string_lossy().into_owned()) }.load().unwrap();
    assert!(cfg.check.semantic);
    assert_eq!(cfg.check.kinds.len(), 2);
    assert_eq!(cfg.check.binaries, vec!["bazel"]);
    assert_eq!(cfg.sources.docs, vec!["DOC.md"]);
    assert!(!cfg.sources.comments);
    assert!(cfg.rules.strict);
    assert_eq!(cfg.report.format, "json");
}

#[test]
fn missing_and_malformed_err() {
    let p = scratch("m").join("nope.toml");
    assert!(TomlSource { path: Some(p.to_string_lossy().into_owned()) }
        .load().unwrap_err().contains("no mimus.toml"));
    let p = scratch("b").join("mimus.toml");
    std::fs::write(&p, "[[[").unwrap();
    assert!(TomlSource { path: Some(p.to_string_lossy().into_owned()) }
        .load().unwrap_err().contains("parse"));
    let p = scratch("u").join("mimus.toml");
    std::fs::write(&p, "bogus = 1").unwrap();
    assert!(TomlSource { path: Some(p.to_string_lossy().into_owned()) }.load().is_err());
}
