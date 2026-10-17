use mimus_config::ConfigSource;
use mimus_config_toml::TomlSource;

fn tmp(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("mimus-cfg-{}-{tag}.toml", std::process::id()))
}

const CFG: &str = r#"
[sources]
docs = ["README.md"]
[check]
mechanical = true
[rules]
dir = "rules"
"#;

#[test]
fn loads_via_env_var() {
    let f = tmp("a");
    std::fs::write(&f, CFG).unwrap();
    unsafe {
        std::env::set_var("MIMUS_CONFIG", &f);
    }
    let c = TomlSource { path: None }.load().unwrap();
    assert!(c.check.mechanical);
    assert_eq!(c.rules.dir.to_string_lossy(), "rules");
    unsafe {
        std::env::remove_var("MIMUS_CONFIG");
    }
    let _ = std::fs::remove_file(&f);
}

#[test]
fn explicit_path_and_missing_error() {
    let f = tmp("b");
    std::fs::write(&f, CFG).unwrap();
    let c = TomlSource {
        path: Some(f.to_string_lossy().into()),
    }
    .load()
    .unwrap();
    assert_eq!(c.sources.docs[0], "README.md");
    assert!(TomlSource {
        path: Some("/nonexistent/x".into())
    }
    .load()
    .is_err());
    let _ = std::fs::remove_file(&f);
}
