//! KeyringSecrets through a PATH-stubbed `secret-tool` — the real
//! binary needs dbus; the stub exercises set/get/failure paths.

use mimus_keyring::KeyringSecrets;
use mimus_secret::Secrets;
use std::process::Command;

fn stub_dir() -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("mimus-kr-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    // lookup → echo $STUB_VAL; store → swallow stdin. Exit = $STUB_EXIT.
    std::fs::write(
        d.join("secret-tool"),
        "#!/bin/sh\nif [ \"$1\" = lookup ]; then printf '%s' \"${STUB_VAL:-}\"; fi\ncat >/dev/null 2>&1\nexit ${STUB_EXIT:-0}\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            d.join("secret-tool"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    d
}

/// env mutation serializes within this binary (cargo test threads).
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn stubbed_get_and_set_paths() {
    let _g = ENV_LOCK.lock().unwrap();
    let d = stub_dir();
    let old = std::env::var_os("PATH");
    let mut paths = vec![d.clone()];
    if let Some(p) = &old {
        paths.extend(std::env::split_paths(p));
    }
    std::env::set_var("PATH", std::env::join_paths(paths).unwrap());
    let s = KeyringSecrets {
        service: "mimus-test".into(),
    };

    // get: success + non-empty stdout → Some
    std::env::set_var("STUB_VAL", "s3cr3t");
    assert_eq!(s.get("k").as_deref(), Some("s3cr3t"));
    // get: success but empty stdout → None
    std::env::set_var("STUB_VAL", "");
    assert_eq!(s.get("k"), None);
    // get: non-zero exit → None
    std::env::set_var("STUB_EXIT", "1");
    assert_eq!(s.get("k"), None);
    std::env::remove_var("STUB_EXIT");

    // set: success → Ok
    s.set("k", "v").unwrap();
    // set: non-zero exit → io error
    std::env::set_var("STUB_EXIT", "1");
    assert!(s.set("k", "v").is_err());
    std::env::remove_var("STUB_EXIT");
    std::env::remove_var("STUB_VAL");

    // secret-tool missing entirely → set errors, get misses
    let empty = d.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    std::env::set_var("PATH", &empty);
    assert!(s.get("k").is_none());
    assert!(s.set("k", "v").is_err());

    match old {
        Some(p) => std::env::set_var("PATH", p),
        None => std::env::remove_var("PATH"),
    }
    let _ = Command::new("true").output(); // sanity: PATH restored is caller's job
    let _ = std::fs::remove_dir_all(&d);
}

/// `store` against an agent that closes stdin instantly — a write
/// bigger than the pipe buffer hits EPIPE and `set` errors instead
/// of blocking forever.
#[test]
fn set_epipe_on_dead_stdin() {
    let _g = ENV_LOCK.lock().unwrap();
    let d = std::env::temp_dir().join(format!("mimus-kr-epipe-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("secret-tool"), "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            d.join("secret-tool"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    let old = std::env::var_os("PATH");
    let mut paths = vec![d.clone()];
    if let Some(p) = &old {
        paths.extend(std::env::split_paths(p));
    }
    std::env::set_var("PATH", std::env::join_paths(paths).unwrap());
    let s = KeyringSecrets {
        service: "mimus-test".into(),
    };
    // 8 MiB ≫ the 64 KiB pipe buffer — the write blocks until the
    // dead child's read end closes → EPIPE → Err
    let big = "x".repeat(8 * 1024 * 1024);
    assert!(s.set("k", &big).is_err());
    match old {
        Some(p) => std::env::set_var("PATH", p),
        None => std::env::remove_var("PATH"),
    }
    let _ = std::fs::remove_dir_all(&d);
}
