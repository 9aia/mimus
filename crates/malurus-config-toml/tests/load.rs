use malurus_config::ConfigSource;
use malurus_config_toml::TomlSource;

fn tmp(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("malurus-cfg-{}-{tag}.toml", std::process::id()))
}

#[test]
fn loads_via_env_var() {
    let f = tmp("a");
    std::fs::write(&f, "[trigger]\nkind = \"github\"\n[runner]\nkind = \"shell\"\ncmd = \"true\"\n[status]\nkind = \"log\"\n[[repo]]\nname = \"o/r\"\ndir = \"/tmp/r\"\n").unwrap();
    unsafe {
        std::env::set_var("MALURUS_CONFIG", &f);
    }
    let c = TomlSource { path: None }.load().unwrap();
    assert_eq!(c.repos[0].name, "o/r");
    unsafe {
        std::env::remove_var("MALURUS_CONFIG");
    }
    let _ = std::fs::remove_file(&f);
}

#[test]
fn explicit_path_and_missing_error() {
    let f = tmp("b");
    std::fs::write(&f, "[trigger]\nkind = \"github\"\n[runner]\nkind = \"shell\"\ncmd = \"true\"\n[status]\nkind = \"log\"\n[[repo]]\nname = \"o/r\"\ndir = \"/tmp/r\"\n").unwrap();
    let c = TomlSource {
        path: Some(f.to_string_lossy().into()),
    }
    .load()
    .unwrap();
    assert_eq!(c.repos[0].name, "o/r");
    assert!(TomlSource {
        path: Some("/nonexistent/x".into())
    }
    .load()
    .is_err());
    let _ = std::fs::remove_file(&f);
}
