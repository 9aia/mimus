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
//! Spike scope: the file-set tier plus the phase-2 predicate tier —
//! `[path."X"]` tables (`exists`, `contains`, `regex`) and the
//! `[git]` namespace (`untracked`) are evaluated by
//! [`Spec::check_repo`]. `providers`, `tamias:` refs, and unknown
//! top-level keys still deserialize (or are ignored) without
//! findings — the model does not `deny_unknown_fields`; they land
//! in [`Spec::extra`] untouched. Unknown keys *inside* a path
//! table or the `git` table are errors (typo ≠ silent pass).
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
    /// `[path."X"]` per-path predicate tables — evaluated by
    /// [`Spec::check_repo`]. Kept as raw JSON at load time so spec
    /// authors get [`SpecError::Predicate`] (naming the offending
    /// key) at check time rather than an opaque serde error;
    /// `Spec::load`/`parse` stay forward-compatible.
    #[serde(default)]
    pub path: BTreeMap<String, serde_json::Value>,
    /// `[git]` namespace predicates — `untracked` globs flag
    /// untracked-but-not-ignored files. Unknown `git.*` keys fail
    /// deserialization (`deny_unknown_fields`) — typo ≠ silent pass.
    #[serde(default)]
    pub git: Option<GitPredicates>,
    /// Unknown top-level keys, preserved verbatim — forward-compat
    /// for `providers`, `tamias`, and namespaces the evaluator
    /// doesn't know yet.
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
    /// A `[path."X"]` predicate or `[git]` rule failed.
    Predicate,
}

/// `[git]` namespace predicates — evaluated by
/// [`Spec::check_repo`]. `deny_unknown_fields`: a mistyped `git.*`
/// key fails spec loading instead of silently passing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitPredicates {
    /// Repo-relative globs that must not appear in
    /// `git ls-files --others --exclude-standard` — i.e. files that
    /// are present but neither tracked nor ignored. Non-git repos
    /// skip this check silently.
    #[serde(default)]
    pub untracked: Vec<String>,
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
    /// A `[path."X"]` table carries a key the evaluator doesn't
    /// know — typo ≠ silent pass.
    #[error("unknown predicate key `{key}` in path table `{path}`")]
    Predicate {
        /// The `[path."X"]` key the table is attached to.
        path: String,
        /// The unrecognized predicate key.
        key: String,
    },
    /// A path-table predicate has the wrong shape or a bad regex —
    /// the table *is* known, its value isn't usable.
    #[error("bad predicate `{key}` in path table `{path}`: {detail}")]
    PredicateValue {
        /// The `[path."X"]` key the table is attached to.
        path: String,
        /// The predicate key (`exists`, `contains`, `regex`, …).
        key: String,
        /// What was wrong with the value.
        detail: String,
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

    /// File-set diff plus the predicate tier: declared-but-absent →
    /// `missing`; tracked-but-uncovered → `unlisted`; `[path."X"]`
    /// tables → `predicate`; `[git] untracked` globs → `predicate`.
    /// `check_missing`/`check_unlisted` keep file-set semantics —
    /// predicate evaluation lives only here.
    ///
    /// # Errors
    /// [`SpecError::Glob`] on a malformed `files`/`cover`/`git`
    /// pattern, [`SpecError::Predicate`] on an unknown path-table
    /// key, [`SpecError::PredicateValue`] on a mistyped predicate
    /// value or uncompilable regex.
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
        out.extend(self.check_path_predicates(repo)?);
        out.extend(self.check_git_untracked(repo)?);
        Ok(out)
    }

    /// Evaluate every `[path."X"]` predicate table against `repo`.
    ///
    /// Semantics:
    /// - Tables are only evaluated for paths **present on disk** —
    ///   an absent path is already a `missing` finding when declared
    ///   in `files`, and a path-table-only absence produces nothing
    ///   (path tables never emit `missing`; `exists = true` on an
    ///   absent file is a no-op, `exists = false` is satisfied).
    /// - `exists = false` fires when the path *is* present — disk
    ///   presence counts whether the file is tracked or not.
    /// - `contains`/`regex` read the file as UTF-8-lossy; a path
    ///   that exists but isn't a readable regular file (e.g. a
    ///   directory) fails content predicates rather than passing
    ///   silently.
    /// - Unknown keys inside a table → [`SpecError::Predicate`];
    ///   mistyped values / bad regexes → [`SpecError::PredicateValue`].
    fn check_path_predicates(&self, repo: &Path) -> Result<Vec<SpecViolation>, SpecError> {
        let sev = self.severity.to_core();
        let mut out = Vec::new();
        for (rel, table) in &self.path {
            let obj = table.as_object().ok_or_else(|| SpecError::PredicateValue {
                path: rel.clone(),
                key: "<table>".into(),
                detail: "path table must be a TOML/YAML table".into(),
            })?;
            for key in obj.keys() {
                if !matches!(key.as_str(), "exists" | "contains" | "regex" | "glob") {
                    return Err(SpecError::Predicate {
                        path: rel.clone(),
                        key: key.clone(),
                    });
                }
            }
            // `glob` is a recognized-but-ignored key: `files`/`cover`
            // already own glob semantics — accepting it keeps specs
            // portable without double-evaluating.
            let abs = repo.join(rel);
            if !abs.exists() {
                continue;
            }
            if let Some(v) = obj.get("exists") {
                let want = v.as_bool().ok_or_else(|| SpecError::PredicateValue {
                    path: rel.clone(),
                    key: "exists".into(),
                    detail: "expected a boolean".into(),
                })?;
                if !want {
                    out.push(self.violation(
                        ViolationKind::Predicate,
                        rel,
                        sev,
                        format!(
                            "`exists = false` in spec `{}` but path is present",
                            self.family
                        ),
                    ));
                    continue; // content predicates are moot on a forbidden path
                }
            }
            let needs_content = obj.contains_key("contains") || obj.contains_key("regex");
            if !needs_content {
                continue;
            }
            let bytes = std::fs::read(&abs).map_err(|err| SpecError::PredicateValue {
                path: rel.clone(),
                key: "<read>".into(),
                detail: format!("cannot read file for content predicates: {err}"),
            })?;
            let text = String::from_utf8_lossy(&bytes);
            if let Some(v) = obj.get("contains") {
                let needle = v.as_str().ok_or_else(|| SpecError::PredicateValue {
                    path: rel.clone(),
                    key: "contains".into(),
                    detail: "expected a string".into(),
                })?;
                if !text.contains(needle) {
                    out.push(self.violation(
                        ViolationKind::Predicate,
                        rel,
                        sev,
                        format!(
                            "`contains` predicate in spec `{}`: expected substring `{needle:?}`",
                            self.family
                        ),
                    ));
                }
            }
            if let Some(v) = obj.get("regex") {
                out.extend(self.check_regex(rel, v, &text, sev)?);
            }
        }
        Ok(out)
    }

    /// `regex` accepts `"pattern"` (must match — require-one) or a
    /// table `{ forbid = [...], require = [...] }`.
    fn check_regex(
        &self,
        rel: &str,
        value: &serde_json::Value,
        text: &str,
        sev: mimus_core::Severity,
    ) -> Result<Vec<SpecViolation>, SpecError> {
        let bad = |detail: String| SpecError::PredicateValue {
            path: rel.to_owned(),
            key: "regex".into(),
            detail,
        };
        let (forbid, require): (Vec<String>, Vec<String>) =
            match value {
                serde_json::Value::String(pat) => (Vec::new(), vec![pat.clone()]),
                serde_json::Value::Object(map) => {
                    let mut forbid = Vec::new();
                    let mut require = Vec::new();
                    for (k, v) in map {
                        let slot = match k.as_str() {
                            "forbid" => &mut forbid,
                            "require" => &mut require,
                            other => {
                                return Err(SpecError::Predicate {
                                    path: rel.to_owned(),
                                    key: format!("regex.{other}"),
                                });
                            }
                        };
                        let arr = v.as_array().ok_or_else(|| {
                            bad(format!("`regex.{k}` must be an array of patterns"))
                        })?;
                        for p in arr {
                            slot.push(p.as_str().map(str::to_owned).ok_or_else(|| {
                                bad(format!("`regex.{k}` entries must be strings"))
                            })?);
                        }
                    }
                    (forbid, require)
                }
                _ => {
                    return Err(bad(
                        "expected a string or {{ forbid, require }} table".into()
                    ))
                }
            };
        let mut out = Vec::new();
        for pat in forbid {
            let re = regex::Regex::new(&pat)
                .map_err(|err| bad(format!("bad pattern {pat:?}: {err}")))?;
            if re.is_match(text) {
                out.push(self.violation(
                    ViolationKind::Predicate,
                    rel,
                    sev,
                    format!(
                        "`regex.forbid` in spec `{}`: pattern {pat:?} must not match",
                        self.family
                    ),
                ));
            }
        }
        for pat in require {
            let re = regex::Regex::new(&pat)
                .map_err(|err| bad(format!("bad pattern {pat:?}: {err}")))?;
            if !re.is_match(text) {
                out.push(self.violation(
                    ViolationKind::Predicate,
                    rel,
                    sev,
                    format!(
                        "`regex`/`regex.require` in spec `{}`: pattern {pat:?} must match",
                        self.family
                    ),
                ));
            }
        }
        Ok(out)
    }

    /// `[git] untracked` — flag untracked (not ignored) files
    /// matching the declared globs. Non-git repos return empty:
    /// there's no index to diff against, and the filesystem-walk
    /// fallback already counts every file for `unlisted`.
    fn check_git_untracked(&self, repo: &Path) -> Result<Vec<SpecViolation>, SpecError> {
        let Some(git) = &self.git else {
            return Ok(Vec::new());
        };
        if git.untracked.is_empty() {
            return Ok(Vec::new());
        }
        let mut b = GlobSetBuilder::new();
        for pat in &git.untracked {
            let g = Glob::new(pat).map_err(|err| SpecError::Glob {
                pattern: pat.clone(),
                err,
            })?;
            b.add(g);
        }
        let set = b.build().map_err(|err| SpecError::Glob {
            pattern: "<git.untracked>".into(),
            err,
        })?;
        let Some(untracked) = git_ls_files_others(repo) else {
            return Ok(Vec::new()); // not a git repo / git missing — skip
        };
        let sev = self.severity.to_core();
        Ok(untracked
            .iter()
            .filter(|f| set.is_match(f))
            .map(|f| {
                self.violation(
                    ViolationKind::Predicate,
                    f,
                    sev,
                    format!(
                        "untracked file matches `git.untracked` in spec `{}`",
                        self.family
                    ),
                )
            })
            .collect())
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

/// `git -C <repo> ls-files --others --exclude-standard` — untracked,
/// non-ignored files. `None` when git is missing, the spawn fails,
/// or the path isn't a repo.
fn git_ls_files_others(repo: &Path) -> Option<Vec<String>> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["ls-files", "--others", "--exclude-standard", "-z"])
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
        // `git` is a typed field now — untracked globs deserialize
        let git = s.git.as_ref().unwrap();
        assert_eq!(git.untracked, vec!["Cargo.lock".to_string()]);
        assert!(!s.extra.contains_key("git"));
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
    fn unknown_git_key_fails_load() {
        // typo safety — `git.*` is deny_unknown_fields, so a bad key
        // errors at parse, not silently at check time
        assert!(Spec::parse("family='x'\ngit.untrackd=['x']\n").is_err());
        assert!(Spec::parse("family='x'\ngit.untracked=['x']\n").is_ok());
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
