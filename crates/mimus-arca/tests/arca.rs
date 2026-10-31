//! ArcaSecrets through a PATH-stubbed `arca` — the real binary
//! needs an identity + vault; the stub exercises get/set/miss/
//! failure paths and proves stdin-only writes.

use mimus_arca::ArcaSecrets;
use mimus_secret::Secrets;

fn stub_dir() -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("mimus-arca-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    // get → {"value":"$STUB_VAL"}; put → stdin tee'd to $STUB_OUT;
    // anything else (--version) → exit $STUB_EXIT.
    std::fs::write(
        d.join("arca"),
        "#!/bin/sh\n\
         [ -n \"$STUB_ARGV\" ] && printf '%s\\n' \"$*\" >>\"$STUB_ARGV\"\n\
         for a in \"$@\"; do\n\
         if [ \"$a\" = get ]; then printf '{\"value\":\"%s\"}' \"${STUB_VAL:-}\"; exit ${STUB_EXIT:-0}; fi\n\
         if [ \"$a\" = put ]; then cat >\"${STUB_OUT:-/dev/null}\"; exit ${STUB_EXIT:-0}; fi\n\
         done\n\
         exit ${STUB_EXIT:-0}\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(d.join("arca"), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    d
}

/// env mutation serializes within this binary (cargo test threads).
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn stubbed() -> (std::path::PathBuf, ArcaSecrets) {
    let d = stub_dir();
    let old = std::env::var_os("PATH");
    let mut paths = vec![d.clone()];
    if let Some(p) = &old {
        paths.extend(std::env::split_paths(p));
    }
    std::env::set_var("PATH", std::env::join_paths(paths).unwrap());
    (d, ArcaSecrets::default())
}

#[test]
fn get_hit_miss_and_failure() {
    let _g = ENV_LOCK.lock().unwrap();
    let (_d, s) = stubbed();
    assert!(s.available());

    std::env::set_var("STUB_VAL", "s3cr3t");
    assert_eq!(s.get("ANTHROPIC_API_KEY").as_deref(), Some("s3cr3t"));
    // empty value → None
    std::env::set_var("STUB_VAL", "");
    assert_eq!(s.get("ANTHROPIC_API_KEY"), None);
    // non-zero exit → None
    std::env::set_var("STUB_EXIT", "1");
    assert_eq!(s.get("ANTHROPIC_API_KEY"), None);
    std::env::remove_var("STUB_EXIT");
}

#[test]
fn set_pipes_stdin_never_argv() {
    let _g = ENV_LOCK.lock().unwrap();
    let (d, s) = stubbed();
    let out = d.join("put-stdin");
    std::env::set_var("STUB_OUT", &out);
    s.set("OPENAI_API_KEY", "sk-fake").unwrap();
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "sk-fake");
    // failure surfaces as an io error
    std::env::set_var("STUB_EXIT", "1");
    assert!(s.set("OPENAI_API_KEY", "sk-fake").is_err());
    std::env::remove_var("STUB_EXIT");
    std::env::remove_var("STUB_OUT");
}

#[test]
fn missing_binary_misses_everything() {
    let s = ArcaSecrets {
        bin: "arca-definitely-not-on-path".into(),
        ..ArcaSecrets::default()
    };
    assert!(!s.available());
    assert_eq!(s.get("ANYTHING"), None);
    assert!(s.set("K", "v").is_err());
}

/// The ref the adapter builds — namespaced under the mimus tree,
/// value never in argv (stdin only for writes).
#[test]
fn ref_shape_and_invocation() {
    let _g = ENV_LOCK.lock().unwrap();
    let (d, s) = stubbed();
    let argv = d.join("argv");
    std::env::set_var("STUB_ARGV", &argv);
    std::env::set_var("STUB_VAL", "v");
    let _ = s.get("ANTHROPIC_API_KEY");
    s.set("OPENAI_API_KEY", "sk-fake").unwrap();
    let logged = std::fs::read_to_string(&argv).unwrap();
    assert!(
        logged.contains("get arca://mimus/ANTHROPIC_API_KEY"),
        "get invokes `arca get arca://mimus/<NAME>` — got: {logged}"
    );
    assert!(
        logged.contains("put arca://mimus/OPENAI_API_KEY"),
        "put invokes `arca put arca://mimus/<NAME>` — got: {logged}"
    );
    assert!(
        !logged.contains("sk-fake"),
        "value must never appear in argv — got: {logged}"
    );
    std::env::remove_var("STUB_ARGV");
    std::env::remove_var("STUB_VAL");
}
