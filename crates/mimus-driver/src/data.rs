//! data — one read of a repo's mimus state per wire op.
//!
//! mimus is unusual among the 9aia tools: it keeps **no persistent
//! state**. There is no `~/.local/share/mimus`, no verdict log, no
//! history — `mimus.toml` plus the repo's own sources *are* the
//! state, and every `mimus` verb recomputes on read. The only
//! on-disk artifacts are caches: `.mimus/triples.db` (prose-triple
//! extraction cache, ADR-0002 ph3) and the kuda index the symbol/
//! graph checkers read (`[check].kuda_index`).
//!
//! So this driver's "snapshot" is resolution, not loading:
//!
//! - `mimus:status`, `mimus:rules`, `mimus:audit` read in-process —
//!   config via `mimus-config`'s `FileSource` (the same source the
//!   CLI uses), rules via `mimus-rules::load_rules`, card trust scores
//!   via the same `.kuda/modules/*.yaml` provenance-hash walk
//!   `mimus audit` performs.
//! - `mimus:violations` is the expensive op: it shells out to
//!   `mimus report --format json` (the full check pipeline —
//!   claim extraction, fs/cmd/symbol checkers, rules, graph
//!   buckets). Reimplementing that pipeline here would fork the
//!   critic; the CLI is the authority.
//!
//! Root/config resolution mirrors the `mimus` CLI exactly
//! (`mimus-cli/src/main.rs` `root()` + `mimus-config`'s
//! `FileSource`):
//!
//! 1. Request params — `config` names a config file directly;
//!    `dir`/`root`/`repo` name the repo dir the config search
//!    starts from (the CLI's `./mimus.toml` becomes
//!    `<dir>/mimus.toml`).
//! 2. `$MIMUS_CONFIG` — same env var the CLI honors.
//! 3. `<search-dir>/mimus.toml` — `./mimus.toml` for the CLI.
//! 4. `~/.config/mimus/mimus.toml` — the global fallback.
//!
//! The repo root is the config file's parent directory (the CLI's
//! `root()`), or `.`/the `dir` param when no config exists — the
//! same place `mimus audit`/`mimus doctor` would look.

use std::path::{Path, PathBuf};
use std::time::Duration;

use mimus_config::ConfigSource;
use mimus_core::Config;
use serde_json::{Map, Value};

/// Everything the views need, resolved once per op.
pub struct Snapshot {
    /// Repo root — config file's parent, `dir` param, or `.`.
    pub root: PathBuf,
    /// The `mimus.toml` that loaded, display string; `None` when
    /// the default search found nothing.
    pub config_path: Option<String>,
    /// Parsed config — `None` when no mimus.toml exists (mimus
    /// verbs that need it degrade; `audit`/`doctor` don't).
    pub config: Option<Config>,
    /// `mimus` executable found on PATH (for the violations view).
    pub mimus_bin: Option<PathBuf>,
    /// Per-source load problems — surfaced, never fatal.
    pub warnings: Vec<String>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Snapshot {
            root: PathBuf::from("."),
            config_path: None,
            config: None,
            mimus_bin: None,
            warnings: Vec::new(),
        }
    }
}

/// `params` → `(repo_dir, config_path)` overrides. `dir`/`root`/
/// `repo` all name the search dir; `dir` wins (mirrors the CLI
/// globals' precedence and sturn-driver's convention).
fn overrides(params: &Map<String, Value>) -> (Option<PathBuf>, Option<String>) {
    let dir = ["dir", "root", "repo"]
        .iter()
        .find_map(|k| params.get(*k).and_then(|v| v.as_str()))
        .map(PathBuf::from);
    let config = params
        .get("config")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    (dir, config)
}

/// The config-file candidates in CLI order, given the explicit
/// path/env override and the directory `./mimus.*` resolves
/// against — `mimus_config::candidates_in` is the same scarab
/// discovery the CLI's `FileSource` runs, rooted at `search_dir`.
/// Returns the first that exists.
fn find_config(explicit: Option<String>, search_dir: &Path) -> Option<PathBuf> {
    mimus_config::candidates_in(search_dir, explicit)
        .into_iter()
        .find(|p| p.is_file())
}

/// Parse a config file through the same `FileSource` the CLI
/// wires — explicit path only, so the search semantics stay here.
fn parse_config(path: &Path) -> Result<Config, String> {
    mimus_config::FileSource {
        path: Some(path.display().to_string()),
    }
    .load()
}

/// `which mimus` — the violations view shells out; record whether
/// the binary exists so status can say so honestly.
fn find_mimus() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join("mimus");
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

/// `(dir, config)` for an engine-op child — the resolution `load`
/// uses, minus the snapshot: `dir` is the cwd the re-exec'd `run`
/// inherits (config's parent, else the search dir), `config` is
/// the `mimus.toml` the child gets as an absolute `--config`.
/// `None` config leaves the child's own search in charge.
pub fn resolve(params: &Map<String, Value>) -> (PathBuf, Option<String>) {
    let (param_dir, param_config) = overrides(params);
    let search_dir = param_dir.unwrap_or_else(|| PathBuf::from("."));
    let found = find_config(param_config, &search_dir);
    let dir = found
        .as_ref()
        .and_then(|p| p.parent())
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or(search_dir);
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let config = found.map(|p| {
        std::fs::canonicalize(&p)
            .map(|a| a.to_string_lossy().into_owned())
            .unwrap_or_else(|_| p.display().to_string())
    });
    (dir, config)
}

/// Resolve + load. `params` are the wire op's caller params —
/// `dir`/`root`/`repo`/`config` steer resolution, the rest pass
/// through to the query resolvers untouched.
pub fn load(params: &Map<String, Value>) -> Snapshot {
    let (param_dir, param_config) = overrides(params);
    let search_dir = param_dir.clone().unwrap_or_else(|| PathBuf::from("."));

    let mut snap = Snapshot {
        mimus_bin: find_mimus(),
        ..Snapshot::default()
    };

    let found = find_config(param_config, &search_dir);
    snap.config_path = found.as_ref().map(|p| p.display().to_string());

    match &found {
        Some(path) => {
            // The CLI's root() — the config file's parent dir.
            snap.root = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            match parse_config(path) {
                Ok(cfg) => snap.config = Some(cfg),
                Err(e) => snap
                    .warnings
                    .push(format!("config {}: {e}", path.display())),
            }
        }
        None => {
            // No mimus.toml anywhere — the CLI would fail `check`/
            // `report` here, but `audit`/`doctor` still run against
            // "."; keep that honest degradation.
            snap.root = search_dir;
        }
    }
    snap
}

/// Run `mimus report --format json` in the resolved root and parse
/// the violations array. This is the one expensive op — everything
/// else reads files in-process. A hard timeout keeps a configured
/// semantic tier (`[check].acp_cmd`) from hanging the wire forever.
pub fn violations_json(snap: &Snapshot) -> Result<Vec<Value>, String> {
    let Some(bin) = &snap.mimus_bin else {
        return Err(
            "mimus CLI not found on PATH — install mimus to populate mimus:violations".into(),
        );
    };
    if snap.config_path.is_none() {
        return Err(
            "no mimus.toml found (searched $MIMUS_CONFIG, <dir>/mimus.toml, ~/.config/mimus/)"
                .into(),
        );
    }
    let mut cmd = std::process::Command::new(bin);
    cmd.arg("report")
        .arg("--format")
        .arg("json")
        .current_dir(&snap.root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawn {}: {e}", bin.display()))?;
    // Poll with a deadline — std has no wait-with-timeout.
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("mimus report timed out after 60s".into());
            }
            Err(e) => return Err(format!("wait mimus report: {e}")),
        }
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("read mimus report: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    // `mimus report` exits 1 when error-severity violations exist —
    // the JSON array is still the payload; only a non-JSON stdout
    // (config error, crash) is a real failure.
    match serde_json::from_str::<Vec<Value>>(&text) {
        Ok(v) => Ok(v),
        Err(e) => {
            let err = String::from_utf8_lossy(&out.stderr);
            let msg: String = err
                .lines()
                .next()
                .unwrap_or("mimus report produced no JSON")
                .to_owned();
            Err(format!("mimus report: {msg} ({e})"))
        }
    }
}
