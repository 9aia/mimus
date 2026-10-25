//! Dotenv Secrets impl: reads, writes, updates, 0600.

use mimus_dotenv::DotenvSecrets;
use mimus_secret::Secrets;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mimus-dotenv-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn get_reads_and_misses() {
    let p = scratch("get").join("env");
    std::fs::write(&p, "A=1\nB= two \nEMPTY=\nnoeq\n").unwrap();
    let s = DotenvSecrets { path: p };
    assert_eq!(s.get("A").as_deref(), Some("1"));
    assert_eq!(s.get("B").as_deref(), Some("two"));
    assert_eq!(s.get("EMPTY"), None);
    assert_eq!(s.get("MISSING"), None);
    assert_eq!(
        DotenvSecrets {
            path: scratch("nf").join("e")
        }
        .get("A"),
        None
    );
}

#[test]
fn set_creates_updates_and_chmods() {
    let p = scratch("set").join("deep/env");
    let s = DotenvSecrets { path: p.clone() };
    s.set("K", "v1").unwrap();
    s.set("OTHER", "x").unwrap();
    s.set("K", "v2").unwrap();
    let text = std::fs::read_to_string(&p).unwrap();
    assert!(text.contains("K=v2") && text.contains("OTHER=x") && !text.contains("v1"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
