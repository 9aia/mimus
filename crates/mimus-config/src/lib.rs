//! Config port + file impl — `mimus.{yaml,yml,toml}` loading through
//! scarab-config's file discovery (P-017): explicit `--config` →
//! `$MIMUS_CONFIG` → `./mimus.*` (YAML preferred) →
//! `~/.config/mimus/`. The generic machinery is scarab's
//! (`scarab-config-file`); this crate is only the wiring plus the
//! [`ConfigSource`] port front-ends bind. The retired
//! `mimus-config-toml` crate's hand-rolled search lives here now.

use std::path::{Path, PathBuf};

use scarab_config::{user_config_dir, Discovery};
use scarab_config_file::FileSource as ScarabSource;

/// A config source — returns the loaded `Config`.
pub trait ConfigSource: Send + Sync {
    /// Load + validate the config; Err carries a readable message.
    fn load(&self) -> Result<mimus_core::Config, String>;
}

/// The file-backed source — explicit `--config` path wins over
/// `$MIMUS_CONFIG`, then the conventional discovery.
pub struct FileSource {
    /// Explicit `--config` path, or None for the default search.
    pub path: Option<String>,
}

/// The underlying scarab source — `for_app("mimus")` conventions
/// (`$MIMUS_CONFIG`, `./`, `~/.config/mimus/`) with the standard
/// YAML-preferred formats.
fn source() -> ScarabSource<mimus_core::Config> {
    ScarabSource::for_app("mimus")
}

impl FileSource {
    fn with_explicit(&self) -> ScarabSource<mimus_core::Config> {
        let mut src = source();
        src.explicit = self.path.clone();
        src
    }

    /// Candidate config paths in search order — for status/hint
    /// views that report which file the chain would read. The
    /// explicit/env path heads the list even when missing (the
    /// loader turns that into an error, not a skip).
    #[must_use]
    pub fn candidates(&self) -> Vec<PathBuf> {
        self.with_explicit()
            .candidates_with(&|k| std::env::var(k).ok())
    }
}

impl ConfigSource for FileSource {
    fn load(&self) -> Result<mimus_core::Config, String> {
        self.with_explicit().load()
    }
}

/// Candidate config paths for `explicit`/`$MIMUS_CONFIG` probing
/// `dir` first, then the user dir — the driver's param-dir variant
/// of [`FileSource`]'s cwd search (engine ops resolve against a
/// caller-supplied dir, not the process cwd).
#[must_use]
pub fn candidates_in(dir: &Path, explicit: Option<String>) -> Vec<PathBuf> {
    let mut src = ScarabSource::<mimus_core::Config>::new(
        Discovery::bare("mimus")
            .env_var("MIMUS_CONFIG")
            .dirs([dir.to_path_buf(), user_config_dir("mimus")]),
    );
    src.explicit = explicit;
    src.candidates_with(&|k| std::env::var(k).ok())
}

/// Parse exactly one config file — no search, no discovery.
///
/// # Errors
/// Fails on a read error, an unclaimed extension, or a parse error.
pub fn parse_file(path: &Path) -> Result<mimus_core::Config, String> {
    source().load_path(path)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mimus-cfg-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
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
    fn explicit_path_loads() {
        let dir = scratch("load");
        let p = dir.join("mimus.toml");
        std::fs::write(&p, "").unwrap();
        let cfg = FileSource {
            path: Some(p.to_string_lossy().into_owned()),
        }
        .load()
        .unwrap();
        assert!(cfg.check.mechanical);
        assert!(!cfg.check.semantic);
        assert!(cfg.sources.comments);
        assert_eq!(cfg.report.format, "human");
        assert_eq!(cfg.rules.dir, PathBuf::from("rules"));
    }

    #[test]
    fn overrides_apply() {
        let dir = scratch("full");
        let p = dir.join("mimus.toml");
        std::fs::write(
            &p,
            "[check]\nsemantic = true\nkinds = [\"path\", \"link\"]\nignore_prefixes = [\"//\"]\nbinaries = [\"bazel\"]\n\
             [sources]\ndocs = [\"DOC.md\"]\ncomments = false\ncards = \"cards/*.yaml\"\n\
             [rules]\ndir = \"my-rules\"\nstrict = true\n\
             [report]\nformat = \"json\"\nmin_confidence = \"all\"\n",
        )
        .unwrap();
        let cfg = FileSource {
            path: Some(p.to_string_lossy().into_owned()),
        }
        .load()
        .unwrap();
        assert!(cfg.check.semantic);
        assert_eq!(cfg.check.kinds.len(), 2);
        assert_eq!(cfg.check.binaries, vec!["bazel"]);
        assert_eq!(cfg.sources.docs, vec!["DOC.md"]);
        assert!(!cfg.sources.comments);
        assert!(cfg.rules.strict);
        assert_eq!(cfg.report.format, "json");
    }

    #[test]
    fn yaml_config_loads() {
        let dir = scratch("yaml");
        let p = dir.join("mimus.yaml");
        std::fs::write(&p, "check:\n  semantic: true\n").unwrap();
        let cfg = FileSource {
            path: Some(p.to_string_lossy().into_owned()),
        }
        .load()
        .unwrap();
        assert!(cfg.check.semantic);
    }

    #[test]
    fn missing_and_malformed_err() {
        let dir = scratch("m");
        let p = dir.join("nope.toml");
        assert!(FileSource {
            path: Some(p.to_string_lossy().into_owned())
        }
        .load()
        .unwrap_err()
        .contains("config file not found"));
        let p = dir.join("mimus.toml");
        std::fs::write(&p, "[[[").unwrap();
        assert!(FileSource {
            path: Some(p.to_string_lossy().into_owned())
        }
        .load()
        .unwrap_err()
        .contains("parse"));
        let p = dir.join("unknown.toml");
        std::fs::write(&p, "bogus = 1").unwrap();
        assert!(FileSource {
            path: Some(p.to_string_lossy().into_owned())
        }
        .load()
        .is_err());
    }

    #[test]
    fn env_var_and_home_unset() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // $MIMUS_CONFIG drives the search when no explicit path.
        let dir = scratch("env");
        let f = dir.join("env.toml");
        std::fs::write(&f, CFG).unwrap();
        let saved_cfg = std::env::var("MIMUS_CONFIG").ok();
        let saved_home = std::env::var("HOME").ok();
        std::env::set_var("MIMUS_CONFIG", f.to_str().unwrap());
        let c = FileSource { path: None }.load().unwrap();
        assert!(c.check.mechanical);
        std::env::remove_var("MIMUS_CONFIG");
        // no --config, no $MIMUS_CONFIG, no $HOME → the search
        // misses everything reachable from a bare cwd.
        std::env::remove_var("HOME");
        let cwd_dir = scratch("nohome");
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&cwd_dir).unwrap();
        let e = FileSource { path: None }.load().unwrap_err();
        std::env::set_current_dir(cwd).unwrap();
        assert!(e.contains("no mimus."), "{e}");
        match saved_cfg {
            Some(v) => std::env::set_var("MIMUS_CONFIG", v),
            None => std::env::remove_var("MIMUS_CONFIG"),
        }
        match saved_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
    }

    #[test]
    fn candidates_in_probes_dir_then_user() {
        let dir = scratch("in");
        std::fs::write(dir.join("mimus.toml"), CFG).unwrap();
        let saved_cfg = std::env::var("MIMUS_CONFIG").ok();
        let saved_home = std::env::var("HOME").ok();
        std::env::remove_var("MIMUS_CONFIG");
        std::env::set_var("HOME", "/nonexistent-home");
        let c = candidates_in(&dir, None);
        match saved_cfg {
            Some(v) => std::env::set_var("MIMUS_CONFIG", v),
            None => std::env::remove_var("MIMUS_CONFIG"),
        }
        match saved_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        // dir's yaml/yml/toml candidates precede the user dir's
        assert!(c.first().unwrap().ends_with("mimus.yaml"));
        assert!(c.iter().any(|p| p.ends_with("mimus.toml")));
        assert!(c
            .iter()
            .any(|p| p.to_string_lossy().contains(".config/mimus")));
    }

    #[test]
    fn parse_file_reads_one_path() {
        let dir = scratch("file");
        let f = dir.join("mimus.toml");
        std::fs::write(&f, CFG).unwrap();
        let c = parse_file(&f).unwrap();
        assert_eq!(c.rules.dir, PathBuf::from("rules"));
    }

    /// env/cwd mutation is process-global — serialize the tests
    /// that touch them (plain `cargo test` shares the process;
    /// nextest already isolates per-test).
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
}
