use mimus_dotenv::DotenvSecrets;
use mimus_secret::Secrets;

fn tmp(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("mimus-env-{}-{tag}", std::process::id()))
}

#[test]
fn set_get_update() {
    let f = tmp("a");
    let s = DotenvSecrets { path: f.clone() };
    s.set("A", "1").unwrap();
    s.set("B", "2").unwrap();
    s.set("A", "3").unwrap(); // update in place
    assert_eq!(s.get("A").as_deref(), Some("3"));
    assert_eq!(s.get("B").as_deref(), Some("2"));
    assert_eq!(s.get("MISSING"), None);
    let text = std::fs::read_to_string(&f).unwrap();
    assert_eq!(text.matches("A=").count(), 1);
    let _ = std::fs::remove_file(&f);
}

#[test]
fn creates_missing_parent_dirs() {
    let f = tmp("nested").join("a/b/env");
    let s = DotenvSecrets { path: f.clone() };
    s.set("K", "v").unwrap();
    assert_eq!(s.get("K").as_deref(), Some("v"));
    let _ = std::fs::remove_dir_all(tmp("nested"));
}

#[test]
fn set_without_parent_dir_errors() {
    // empty path → parent() is None → mkdir skipped, write fails
    let s = DotenvSecrets {
        path: std::path::PathBuf::new(),
    };
    assert!(s.set("K", "v").is_err());
    // parent exists as a *file* → create_dir_all fails
    let dir = tmp("blocked");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("afile"), "x").unwrap();
    let s = DotenvSecrets {
        path: dir.join("afile/x.env"),
    };
    assert!(s.set("K", "v").is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[cfg(unix)]
fn perms_600() {
    use std::os::unix::fs::PermissionsExt;
    let f = tmp("b");
    let s = DotenvSecrets { path: f.clone() };
    s.set("K", "v").unwrap();
    assert_eq!(
        std::fs::metadata(&f).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let _ = std::fs::remove_file(&f);
}
