use mimus_config::ConfigSource;
use mimus_config_toml::TomlSource;

/// env-mutating tests serialize on this (nextest already isolates
/// per-test; plain `cargo test` shares the process).
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
    let _g = ENV_LOCK.lock().unwrap();
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
fn home_unset_falls_back_to_dot() {
    let _g = ENV_LOCK.lock().unwrap();
    // no --config, no $MIMUS_CONFIG, no $HOME → searches ./mimus.toml
    // (absent in the crate dir) then "."/.config/... → readable error
    std::env::remove_var("MIMUS_CONFIG");
    let home = std::env::var_os("HOME");
    std::env::remove_var("HOME");
    let e = TomlSource { path: None }.load().unwrap_err();
    assert!(e.contains("no mimus.toml"), "{e}");
    if let Some(h) = home {
        std::env::set_var("HOME", h);
    }
}

/// A candidate that exists but can't be read → `read {p}: {e}`,
/// not the "no mimus.toml" fallback.
#[test]
#[cfg(unix)]
fn unreadable_candidate_errors() {
    use std::os::unix::fs::PermissionsExt;
    let f = tmp("unread");
    std::fs::write(&f, "x").unwrap();
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o000)).unwrap();
    let e = TomlSource {
        path: Some(f.to_string_lossy().into_owned()),
    }
    .load()
    .unwrap_err();
    assert!(e.contains("read"), "{e}");
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).unwrap();
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
