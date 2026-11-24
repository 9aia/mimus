//! mimus-spec — the standard tier's spec model and file-set
//! evaluator (P-001 spike).
//!
//! A *standard* is a declarative repo-conformance spec: a TOML (or
//! YAML/JSON) document naming a `family`, the repo-name `match`
//! globs it applies to, a `severity`, and the file set the repo is
//! expected to carry. Whitelist semantics: anything tracked in the
//! repo that no `files`/`dirs` entry covers is an `unlisted`
//! violation.
//!
//! Document resolution delegates to `scarab-compose`
//! ([`Composer::resolve_file`]/[`Composer::resolve_toml`]):
//! `extends:` chains and `include:`/`${include(...)}` come free,
//! anchored at the spec file's own directory. Merge semantics are
//! scarab-compose's `deep_merge` — scalars last-wins, **arrays
//! concatenate** (child entries first), tables merge per-key.
//!
//! Spike scope: only the file-set is evaluated. `[path."X"]`
//! predicate tables, `providers`, `git.*` rules, and `tamias:` refs
//! deserialize (or are ignored) but produce no findings. Unknown
//! top-level keys are tolerated for forward-compat — the model does
//! not `deny_unknown_fields`; unrecognized keys land in
//! [`Spec::extra`] untouched.
//!
//! Repo enumeration prefers `git -C <repo> ls-files` (tracked files
//! only — generated dirs like `target/` never appear, so no
//! carve-outs are needed beyond `.git/` itself). When git is absent
//! or the path isn't a repo, a filesystem walk takes over: every
//! file except `.git/` counts, *including* untracked ones — the
//! fallback is noisier by design and documented as such.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use scarab_compose::Composer;
use serde::{Deserialize, Serialize};

/// Spec-declared severity — `fail` gates (maps to
/// [`mimus_core::Severity::Error`]), `warn` is advisory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpecSeverity {
    /// Violations fail the check.
    Fail,
    /// Violations report but never gate.
    Warn,
}

impl SpecSeverity {
    /// The mimus-core severity this maps to.
    #[must_use]
    pub fn to_core(self) -> mimus_core::Severity {
        match self {
            Self::Fail => mimus_core::Severity::Error,
            Self::Warn => mimus_core::Severity::Warn,
        }
    }
}

fn default_severity() -> SpecSeverity {
    SpecSeverity::Fail
}

/// A declarative repo-conformance spec — the resolved document.
///
/// Field names mirror the TOML: `match` is a Rust keyword, so the
/// repo-name globs live on [`repo_match`](Self::repo_match) via a
/// serde rename. `path` collects the `[path."X"]` per-path predicate
/// tables; `extra` catches every other top-level key (`extends` is
/// consumed by the composer before this point, but `providers`,
/// `git`, future namespaces, … land here).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spec {
    /// Spec family name — appears on every violation for
    /// provenance (`engine-compile`, `9aia/base`, …).
    pub family: String,
    /// Repo-name globs this spec applies to (matched against the
    /// repo directory's file name). Empty = applies to every repo.
    #[serde(rename = "match", default)]
    pub repo_match: Vec<String>,
    /// Violation severity — `fail` (default) or `warn`.
    #[serde(default = "default_severity")]
    pub severity: SpecSeverity,
    /// Required/covering file entries: exact repo-relative paths
    /// (must exist → `missing` when absent) or globs (coverage
    /// only — a glob with zero matches is not itself a violation).
    #[serde(default)]
    pub files: Vec<String>,
    /// Declared directories: must exist → `missing` when absent,
    /// and every tracked file beneath them is covered (whitelist).
    /// Literal paths only — no glob semantics.
    #[serde(default)]
    pub dirs: Vec<String>,
    /// Coverage-only directories — whitelist tracked files beneath
    /// them for the `unlisted` check but never produce `missing`
    /// (the optional counterpart of `dirs`; lets a base spec name
    /// every plausible tree without requiring all of them).
    #[serde(default)]
    pub cover_dirs: Vec<String>,
    /// Coverage-only globs — whitelist tracked files for the
    /// `unlisted` check but never produce `missing` (the optional
    /// counterpart of glob `files` entries; keeps catch-alls like
    /// `*.toml` out of the required set).
    #[serde(default)]
    pub cover: Vec<String>,
    /// `[path."X"]` per-path predicate tables — deserialized for
    /// forward-compat, **not evaluated** in this spike.
    #[serde(default)]
    pub path: BTreeMap<String, serde_json::Value>,
    /// Unknown top-level keys, preserved verbatim — forward-compat
    /// for `providers`, `git`, and namespaces the spike doesn't
    /// evaluate yet.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// What the file-set diff found wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViolationKind {
    /// A declared file/dir is absent from the repo.
    Missing,
    /// A tracked file isn't covered by any `files`/`dirs` entry.
    Unlisted,
}

/// One file-set finding — spec provenance + severity attached.
#[derive(Debug, Clone, Serialize)]
pub struct SpecViolation {
    /// `missing` or `unlisted`.
    pub kind: ViolationKind,
    /// Repo-relative path at fault.
    pub path: String,
    /// The spec `family` that produced this.
    pub spec: String,
    /// The spec's mapped severity.
    pub severity: mimus_core::Severity,
    /// Human-readable evidence.
    pub detail: String,
}

/// Spec loading / evaluation failure.
#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    /// Document composition failed (parse, include, extends cycle…).
    #[error("compose: {0}")]
    Compose(#[from] scarab_compose::ComposeError),
    /// The resolved document doesn't fit the spec model.
    #[error("spec model: {0}")]
    Model(#[from] serde_json::Error),
    /// Filesystem failure (reading the spec, walking the repo).
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// A `files` entry isn't a valid glob.
    #[error("bad glob `{pattern}`: {err}")]
    Glob {
        /// The offending pattern.
        pattern: String,
        /// The globset error.
        err: globset::Error,
    },
    /// A `match` entry isn't a valid glob.
    #[error("bad match glob `{pattern}`: {err}")]
    MatchGlob {
        /// The offending pattern.
        pattern: String,
        /// The globset error.
        err: globset::Error,
    },
}

impl Spec {
    /// Load + resolve a spec file — extension dispatch like
    /// `Composer::resolve_file` (`.toml` → TOML, `.json` → JSON,
    /// else YAML). `extends:`/`include:` anchor at the file's dir.
    ///
    /// # Errors
    /// [`SpecError::Compose`] on read/parse/composition failure,
    /// [`SpecError::Model`] when the resolved doc lacks `family` or
    /// has a mistyped field.
    pub fn load(path: &Path) -> Result<Self, SpecError> {
        let value = Composer::new().resolve_file(path)?;
        Ok(serde_json::from_value(value)?)
    }

    /// Parse TOML spec text with includes anchored at `base_dir`.
    ///
    /// # Errors
    /// Same as [`Spec::load`].
    pub fn parse_toml(text: &str, base_dir: &Path) -> Result<Self, SpecError> {
        let value = Composer::new().resolve_toml(text, base_dir)?;
        Ok(serde_json::from_value(value)?)
    }

    /// Parse TOML spec text anchored at the current directory —
    /// convenience for specs with no relative includes.
    ///
    /// # Errors
    /// Same as [`Spec::load`].
    pub fn parse(text: &str) -> Result<Self, SpecError> {
        Self::parse_toml(text, Path::new("."))
    }

    /// Does this spec apply to a repo named `repo_name` (the repo
    /// directory's file name)? Empty `match` = applies to all.
    #[must_use]
    pub fn matches_repo(&self, repo_name: &str) -> bool {
        if self.repo_match.is_empty() {
            return true;
        }
        self.repo_match.iter().any(|pat| {
            Glob::new(pat)
                .map(|g| g.compile_matcher().is_match(repo_name))
                .unwrap_or(false)
        })
    }

    /// Declared-but-absent check only: exact `files` entries (no
    /// glob metachars) must exist on disk; every `dirs` entry must
    /// be a directory. Glob `files`/`cover*` entries are
    /// coverage-only — zero matches is not a violation.
    ///
    /// # Errors
    /// [`SpecError::Glob`] on a malformed `files`/`cover` pattern.
    pub fn check_missing(&self, repo: &Path) -> Result<Vec<SpecViolation>, SpecError> {
        let sev = self.severity.to_core();
        let mut out = Vec::new();
        for entry in &self.files {
            if is_glob(entry) {
                continue;
            }
            let p = repo.join(entry);
            if !p.exists() {
                out.push(self.violation(
                    ViolationKind::Missing,
                    entry,
                    sev,
                    format!("declared in spec `{}` but absent", self.family),
                ));
            }
        }
        for d in &self.dirs {
            let dir = d.trim_end_matches('/');
            if !dir.is_empty() && !repo.join(dir).is_dir() {
                out.push(self.violation(
                    ViolationKind::Missing,
                    dir,
                    sev,
                    format!("declared dir in spec `{}` but absent", self.family),
                ));
            }
        }
        Ok(out)
    }

    /// File-set diff: declared-but-absent → `missing`; tracked-but-
    /// uncovered → `unlisted`. Only the file-set tier runs —
    /// `[path.*]` predicates and provider namespaces are inert.
    ///
    /// # Errors
    /// [`SpecError::Glob`] on a malformed `files` pattern.
    pub fn check_repo(&self, repo: &Path) -> Result<Vec<SpecViolation>, SpecError> {
        let tracked = repo_files(repo);
        let coverage = self.coverage_set()?;
        let sev = self.severity.to_core();
        let mut out = self.check_missing(repo)?;

        // unlisted — every enumerated file must be covered by a
        // `files`/`cover` glob or live under a declared `dirs` /
        // `cover_dirs` entry. Single-spec callers check against
        // their own coverage; [`check_unlisted`] unions coverage
        // across a matched spec set for the composed standard.
        let covered_dirs: Vec<&str> = self
            .dirs
            .iter()
            .chain(self.cover_dirs.iter())
            .map(|d| d.trim_end_matches('/'))
            .filter(|d| !d.is_empty())
            .collect();
        for f in &tracked {
            let covered =
                coverage.is_match(f) || covered_dirs.iter().any(|d| Path::new(f).starts_with(d));
            if !covered {
                out.push(self.violation(
                    ViolationKind::Unlisted,
                    f,
                    sev,
                    format!(
                        "tracked file not covered by spec `{}` files/dirs",
                        self.family
                    ),
                ));
            }
        }
        Ok(out)
    }

    /// All `files` entries as one matcher — exact paths are valid
    /// globs, so literals and patterns share the set.
    fn coverage_set(&self) -> Result<GlobSet, SpecError> {
        let mut b = GlobSetBuilder::new();
        for pat in self.files.iter().chain(self.cover.iter()) {
            let g = Glob::new(pat).map_err(|err| SpecError::Glob {
                pattern: pat.clone(),
                err,
            })?;
            b.add(g);
        }
        b.build().map_err(|err| SpecError::Glob {
            pattern: "<set>".into(),
            err,
        })
    }

    fn violation(
        &self,
        kind: ViolationKind,
        path: &str,
        severity: mimus_core::Severity,
        detail: String,
    ) -> SpecViolation {
        SpecViolation {
            kind,
            path: path.to_owned(),
            spec: self.family.clone(),
            severity,
            detail,
        }
    }
}

/// `unlisted` check against the **union** of a matched spec set —
/// the composed standard. A tracked file is a violation only when
/// *no* matched spec covers it; per-spec `check_repo` would
/// double-report files a sibling spec legitimately whitelists.
/// Severity is the strictest among `specs`; `spec` names the
/// strictest spec (the violation belongs to the composition, not
/// one pack).
///
/// # Errors
/// [`SpecError::Glob`] on a malformed pattern in any spec.
pub fn check_unlisted(repo: &Path, specs: &[&Spec]) -> Result<Vec<SpecViolation>, SpecError> {
    if specs.is_empty() {
        return Ok(Vec::new());
    }
    let mut b = GlobSetBuilder::new();
    for s in specs {
        for pat in s.files.iter().chain(s.cover.iter()) {
            let g = Glob::new(pat).map_err(|err| SpecError::Glob {
                pattern: pat.clone(),
                err,
            })?;
            b.add(g);
        }
    }
    let coverage = b.build().map_err(|err| SpecError::Glob {
        pattern: "<union>".into(),
        err,
    })?;
    let dirs: Vec<&str> = specs
        .iter()
        .flat_map(|s| s.dirs.iter().chain(s.cover_dirs.iter()))
        .map(|d| d.trim_end_matches('/'))
        .filter(|d| !d.is_empty())
        .collect();
    // strictest severity wins — Error > Warn
    let sev = if specs.iter().any(|s| s.severity == SpecSeverity::Fail) {
        mimus_core::Severity::Error
    } else {
        mimus_core::Severity::Warn
    };
    let anchor = specs
        .iter()
        .find(|s| s.severity == SpecSeverity::Fail)
        .or_else(|| specs.first())
        .map_or_else(|| "<union>".to_owned(), |s| s.family.clone());
    let tracked = repo_files(repo);
    let mut out = Vec::new();
    for f in &tracked {
        let covered = coverage.is_match(f) || dirs.iter().any(|d| Path::new(f).starts_with(d));
        if !covered {
            out.push(SpecViolation {
                kind: ViolationKind::Unlisted,
                path: f.clone(),
                spec: anchor.clone(),
                severity: sev,
                detail: format!(
                    "tracked file not covered by any of {} matched spec(s)",
                    specs.len()
                ),
            });
        }
    }
    Ok(out)
}

/// Glob metacharacters — decides whether a `files` entry is an
/// exact path (must exist) or a pattern (coverage only).
fn is_glob(pat: &str) -> bool {
    pat.contains('*') || pat.contains('?') || pat.contains('[') || pat.contains('{')
}

/// The repo's file list as repo-relative `/`-separated strings:
/// `git ls-files` when possible, filesystem walk otherwise.
fn repo_files(repo: &Path) -> Vec<String> {
    git_ls_files(repo).unwrap_or_else(|| walk_files(repo))
}

/// `git -C <repo> ls-files` — tracked files only. `None` when git
/// is missing, the spawn fails, or the path isn't a repo.
fn git_ls_files(repo: &Path) -> Option<Vec<String>> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["ls-files", "-z"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(
        out.stdout
            .split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect(),
    )
}

/// Fallback enumeration: every file under `repo` except `.git/` —
/// untracked files included (noisier than the git path; documented
/// on the module). Sorted for deterministic output.
fn walk_files(repo: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![repo.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if e.file_name() != ".git" {
                    stack.push(p);
                }
            } else if let Ok(rel) = p.strip_prefix(repo) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    out
}

/// Repo directory name — the `match` globs' subject.
/// Canonicalizes so `..`/`.` segments don't leak into the name.
#[must_use]
pub fn repo_name(repo: &Path) -> String {
    let canon = std::fs::canonicalize(repo).unwrap_or_else(|_| repo.to_path_buf());
    canon
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| repo.to_string_lossy().into_owned())
}

/// Load every spec under `dir` — `*.toml`, `*.yaml`, `*.yml`,
/// `*.json`, sorted by file name for determinism. A single spec
/// file path loads as a one-element vec.
///
/// # Errors
/// Propagates [`Spec::load`] failures.
pub fn load_specs(path: &Path) -> Result<Vec<Spec>, SpecError> {
    if path.is_file() {
        return Ok(vec![Spec::load(path)?]);
    }
    let mut files: Vec<PathBuf> = Vec::new();
    let rd = std::fs::read_dir(path)?;
    for e in rd.flatten() {
        let p = e.path();
        if p.is_file()
            && matches!(
                p.extension().and_then(|x| x.to_str()),
                Some("toml" | "yaml" | "yml" | "json")
            )
        {
            files.push(p);
        }
    }
    files.sort();
    files.iter().map(|p| Spec::load(p)).collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn parse_minimal() {
        let s = Spec::parse(
            r#"
family = "base"
match  = ["engine-*"]
files  = ["Cargo.toml", "src/**/*.rs"]
dirs   = ["src"]
"#,
        )
        .unwrap();
        assert_eq!(s.family, "base");
        assert_eq!(s.repo_match, vec!["engine-*"]);
        assert_eq!(s.severity, SpecSeverity::Fail);
        assert!(s.path.is_empty());
    }

    #[test]
    fn path_tables_and_unknown_keys_deserialize() {
        let s = Spec::parse(
            r#"
family = "base"
providers = { lasius = { require = "clean-clone" } }
git.untracked = ["Cargo.lock"]

[path."Cargo.toml"]
manifest.license = "MIT"
regex.forbid = ['^\[patch\.']
"#,
        )
        .unwrap();
        assert!(s.path.contains_key("Cargo.toml"));
        assert!(s.extra.contains_key("providers"));
        assert!(s.extra.contains_key("git"));
    }

    #[test]
    fn severity_warn_maps() {
        let s = Spec::parse("family='x'\nseverity='warn'\n").unwrap();
        assert_eq!(s.severity.to_core(), mimus_core::Severity::Warn);
    }

    #[test]
    fn cover_whitelists_without_requiring() {
        let s = Spec::parse("family='x'\nfiles=['README.md']\ncover=['optional.toml','*.lock']\n")
            .unwrap();
        assert!(s.cover.contains(&"optional.toml".to_string()));
        // cover entries join the coverage set — a repo carrying only
        // covered files reports no `unlisted`
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        std::fs::write(repo.join("README.md"), "x").unwrap();
        std::fs::write(repo.join("optional.toml"), "x").unwrap();
        std::fs::write(repo.join("Cargo.lock"), "x").unwrap();
        let violations = s.check_repo(repo).unwrap();
        assert!(
            violations.iter().all(|v| v.kind != ViolationKind::Unlisted),
            "{violations:?}"
        );
        // …but cover never produces `missing` — drop README, keep
        // the covered files: only `missing` fires, no unlisted
        std::fs::remove_file(repo.join("README.md")).unwrap();
        let violations = s.check_repo(repo).unwrap();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].kind, ViolationKind::Missing);
    }

    #[test]
    fn match_globs_repo_name() {
        let s = Spec::parse("family='x'\nmatch=['engine-*','*-lib']\n").unwrap();
        assert!(s.matches_repo("engine-foo"));
        assert!(s.matches_repo("my-lib"));
        assert!(!s.matches_repo("docs-site"));
        // empty match applies to all
        let all = Spec::parse("family='x'\n").unwrap();
        assert!(all.matches_repo("anything"));
    }
}
