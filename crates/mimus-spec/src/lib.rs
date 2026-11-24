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
//! `[path."X"]` tables (`exists`, `contains`, `regex`), the
//! `[git]` namespace (`untracked`), and the `[providers.<ns>]`
//! namespace are evaluated by [`Spec::check_repo`]. `tamias:` refs
//! and unknown top-level keys still deserialize (or are ignored)
//! without findings — the model does not `deny_unknown_fields`;
//! they land in [`Spec::extra`] untouched. Unknown keys *inside* a
//! path table or the `git` table are errors (typo ≠ silent pass).
//!
//! Providers (P-002): each org tool owns a predicate namespace
//! (`lasius.*`, `bupha.*`, …) declared as `[providers.<ns>]`
//! tables. At check time the namespace dispatches to the
//! [`StandardProvider`] registered under that `ns()` in a
//! [`ProviderSet`]; an unregistered namespace degrades to a
//! `provider` **warn** violation (`provider-unavailable`) — never
//! a hard fail, since providers are optional by design. The
//! claims table passes through to the provider verbatim: unknown
//! keys inside it are the provider's problem, not the spec
//! model's.
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
/// tables; `providers` collects the `[providers.<ns>]` claim
/// tables (P-002); `extra` catches every other top-level key
/// (`extends` is consumed by the composer before this point, but
/// `tamias`, future namespaces, … land here).
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
    /// `[providers.<ns>]` claim tables — namespace → raw claims
    /// `Value`, passed through to the [`StandardProvider`]
    /// registered under `ns` at check time. Values stay untyped on
    /// purpose: each provider owns its claims schema, and unknown
    /// keys inside a table are the provider's problem. An
    /// unregistered namespace yields a `provider` warn violation,
    /// never a load error.
    #[serde(default)]
    pub providers: BTreeMap<String, serde_json::Value>,
    /// Unknown top-level keys, preserved verbatim — forward-compat
    /// for `tamias` and namespaces the evaluator doesn't know yet.
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
    /// A `[providers.<ns>]` claim — either the namespace has no
    /// registered provider (`provider-unavailable`, always warn)
    /// or a provider reported a finding.
    Provider,
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
    /// `missing`, `unlisted`, `predicate`, or `provider`.
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
    /// A registered provider failed while evaluating its
    /// `[providers.<ns>]` claims table — propagated, never
    /// silently passed (unlike an *unregistered* namespace, which
    /// degrades to a warn violation).
    #[error("provider `{ns}`: {source}")]
    Provider {
        /// The `providers.<ns>` namespace that failed.
        ns: String,
        /// The provider's own error.
        source: ProviderError,
    },
}

/// A [`StandardProvider`] evaluation failure — provider-internal
/// (CLI probe failed, claims unparseable, …). Kept a single
/// message: providers own their diagnostics.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ProviderError(pub String);

/// An org-tool predicate provider (P-002) — owns one
/// `providers.<ns>` namespace and evaluates its claims table
/// against a repo. Implementations may shell out to their tool's
/// CLI (`bupha status --json`-style) or inspect the tree directly;
/// the port stays I/O-light by contract.
///
/// `claims` is the raw `[providers.<ns>]` table from the spec —
/// untyped on purpose: the provider owns its claims schema, and
/// unknown keys inside it are the provider's problem, not the
/// spec model's. Returned violations keep whatever `spec`/
/// `severity` the provider sets; the evaluator passes them
/// through verbatim.
pub trait StandardProvider {
    /// Namespace prefix — `"bupha"`, `"lasius"`, … — matched
    /// against the key under `providers.` in the spec.
    fn ns(&self) -> &str;

    /// Evaluate one claims table against `repo`.
    ///
    /// # Errors
    /// [`ProviderError`] on provider-internal failure — surfaces
    /// as [`SpecError::Provider`] at check time.
    fn check(
        &self,
        repo: &Path,
        claims: &serde_json::Value,
    ) -> Result<Vec<SpecViolation>, ProviderError>;
}

/// The registered [`StandardProvider`]s, keyed by [`ns()`](StandardProvider::ns).
///
/// Empty by default — every `providers.<ns>` claim then degrades
/// to a `provider` warn violation. Real providers (lasius, bupha,
/// …) register here as they land; see `mimus standard`.
#[derive(Default)]
pub struct ProviderSet {
    providers: Vec<Box<dyn StandardProvider>>,
}

impl ProviderSet {
    /// An empty registry — the default everywhere until real
    /// providers ship.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `provider` under its [`ns()`](StandardProvider::ns).
    /// Re-registering the same namespace replaces the earlier
    /// entry (last wins).
    pub fn register<P: StandardProvider + 'static>(&mut self, provider: P) -> &mut Self {
        let ns = provider.ns().to_owned();
        self.providers.retain(|p| p.ns() != ns);
        self.providers.push(Box::new(provider));
        self
    }

    /// The provider registered under `ns`, if any.
    #[must_use]
    pub fn get(&self, ns: &str) -> Option<&dyn StandardProvider> {
        self.providers
            .iter()
            .find(|p| p.ns() == ns)
            .map(|p| p.as_ref())
    }

    /// Number of registered providers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// Whether no providers are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

/// `cargo` provider — Cargo.toml semantics the regex layer can't
/// express. Currently one claim:
///
/// - `patches = "org-only"` — every `[patch.<src>]` table must
///   target an org-local source (`crates-io` or a
///   `github.com/<org>/*` URL), and every patch entry must
///   resolve org-locally: `path = "../…"` sibling checkout or a
///   `git` URL under `github.com/<org>/`. Version-only
///   entries (no `path`/`git`/`registry`) are allowed — they pin,
///   they don't relocate.
/// - `org = "<github-org>"` — optional, default `"handl-lang"`.
///   Names the org that counts as local for `patches`.
/// - `severity = "warn" | "error"` — optional, default `"error"`.
///
/// Unknown claim keys are a [`ProviderError`] — a typo'd claim
/// must not silently pass.
#[derive(Debug, Default, Clone, Copy)]
pub struct CargoProvider;

impl StandardProvider for CargoProvider {
    fn ns(&self) -> &str {
        "cargo"
    }

    fn check(
        &self,
        repo: &Path,
        claims: &serde_json::Value,
    ) -> Result<Vec<SpecViolation>, ProviderError> {
        let obj = claims
            .as_object()
            .ok_or_else(|| ProviderError("`providers.cargo` claims must be a table".into()))?;
        for key in obj.keys() {
            if !matches!(key.as_str(), "patches" | "org" | "severity") {
                return Err(ProviderError(format!(
                    "unknown `providers.cargo` claim `{key}`"
                )));
            }
        }
        let patches = obj
            .get("patches")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if patches != "org-only" {
            return Err(ProviderError(format!(
                "`providers.cargo.patches` must be \"org-only\", got {patches:?}"
            )));
        }
        let org = obj
            .get("org")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("handl-lang");
        let severity = match obj.get("severity").and_then(serde_json::Value::as_str) {
            None | Some("error") => mimus_core::Severity::Error,
            Some("warn") => mimus_core::Severity::Warn,
            Some(other) => {
                return Err(ProviderError(format!(
                    "`providers.cargo.severity` must be \"warn\" or \"error\", got {other:?}"
                )));
            }
        };

        let manifest = repo.join("Cargo.toml");
        let text = match std::fs::read_to_string(&manifest) {
            Ok(t) => t,
            Err(err) => {
                return Err(ProviderError(format!(
                    "cannot read {}: {err}",
                    manifest.display()
                )));
            }
        };
        let doc: toml::Value = text
            .parse()
            .map_err(|err| ProviderError(format!("Cargo.toml parse: {err}")))?;
        let Some(patch) = doc.get("patch").and_then(toml::Value::as_table) else {
            return Ok(Vec::new());
        };

        let mut out = Vec::new();
        let mut violate = |detail: String| {
            out.push(SpecViolation {
                kind: ViolationKind::Provider,
                path: "Cargo.toml".into(),
                spec: "cargo".into(),
                severity,
                detail,
            });
        };
        let org_host = format!("github.com/{org}/");
        let org_ssh = format!("github.com:{org}/");
        let is_org_git = |u: &str| u.contains(&org_host) || u.contains(&org_ssh);
        for (src, entries) in patch {
            let org_src = src == "crates-io" || is_org_git(src);
            if !org_src {
                violate(format!(
                    "`[patch.{src}]` source is not org-local — patches may only target \
                     `crates-io` or `github.com/{org}/*`"
                ));
            }
            let Some(entries) = entries.as_table() else {
                violate(format!("`[patch.{src}]` is not a table"));
                continue;
            };
            for (name, entry) in entries {
                let Some(entry) = entry.as_table() else {
                    violate(format!("`[patch.{src}] {name}` is not a table"));
                    continue;
                };
                if let Some(path) = entry.get("path").and_then(toml::Value::as_str) {
                    if !path.starts_with("../") {
                        violate(format!(
                            "`[patch.{src}] {name}` path {path:?} is not a `../` sibling checkout"
                        ));
                    }
                } else if let Some(git) = entry.get("git").and_then(toml::Value::as_str) {
                    if !is_org_git(git) {
                        violate(format!(
                            "`[patch.{src}] {name}` git {git:?} is not `github.com/{org}/*`"
                        ));
                    }
                } else if entry.contains_key("registry") {
                    violate(format!(
                        "`[patch.{src}] {name}` uses an external `registry` source"
                    ));
                }
            }
        }
        Ok(out)
    }
}

/// `repo` provider — repo-identity checks that need the checkout's
/// directory name, which path predicates can't see. Currently one
/// claim:
///
/// - `bupha-name = true` — `bupha.toml` must contain a `[[repos]]`
///   self-entry named `<org>/<dirname>` whose `dir` also ends in
///   `/<dirname>`. Catches `name =`/`dir =` drift after a rename.
///   Entries naming *other* repos are ignored — shared watcher
///   manifests (docs) legitimately list the fleet. Absent
///   `bupha.toml` is a no-op — the file-set's `missing` finding
///   already covers that.
/// - `severity = "warn" | "error"` — optional, default `"warn"`
///   (identity drift is a floor concern, not a family gate).
///
/// Unknown claim keys are a [`ProviderError`].
#[derive(Debug, Default, Clone, Copy)]
pub struct RepoProvider;

impl StandardProvider for RepoProvider {
    fn ns(&self) -> &str {
        "repo"
    }

    fn check(
        &self,
        repo: &Path,
        claims: &serde_json::Value,
    ) -> Result<Vec<SpecViolation>, ProviderError> {
        let obj = claims
            .as_object()
            .ok_or_else(|| ProviderError("`providers.repo` claims must be a table".into()))?;
        for key in obj.keys() {
            if !matches!(key.as_str(), "bupha-name" | "severity") {
                return Err(ProviderError(format!(
                    "unknown `providers.repo` claim `{key}`"
                )));
            }
        }
        if obj.get("bupha-name").and_then(serde_json::Value::as_bool) != Some(true) {
            return Err(ProviderError(
                "`providers.repo.bupha-name` must be `true`".into(),
            ));
        }
        let severity = match obj.get("severity").and_then(serde_json::Value::as_str) {
            None | Some("warn") => mimus_core::Severity::Warn,
            Some("error") => mimus_core::Severity::Error,
            Some(other) => {
                return Err(ProviderError(format!(
                    "`providers.repo.severity` must be \"warn\" or \"error\", got {other:?}"
                )));
            }
        };

        let manifest = repo.join("bupha.toml");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            return Ok(Vec::new()); // missing file → `missing` finding's job
        };
        let doc: toml::Value = text
            .parse()
            .map_err(|err| ProviderError(format!("bupha.toml parse: {err}")))?;
        let Some(dirname) = repo.file_name().and_then(|n| n.to_str()) else {
            return Err(ProviderError(format!(
                "repo path {} has no usable directory name",
                repo.display()
            )));
        };
        let mut out = Vec::new();
        let mut self_entries = 0usize;
        if let Some(repos) = doc.get("repos").and_then(toml::Value::as_array) {
            for entry in repos {
                let name = entry
                    .get("name")
                    .and_then(toml::Value::as_str)
                    .unwrap_or("");
                if !(name.contains('/') && name.rsplit('/').next() == Some(dirname)) {
                    continue; // another repo's entry — shared manifests list the fleet
                }
                self_entries += 1;
                let dir = entry
                    .get("dir")
                    .and_then(toml::Value::as_str)
                    .unwrap_or("")
                    .trim_end_matches('/');
                if !dir.ends_with(&format!("/{dirname}")) {
                    out.push(SpecViolation {
                        kind: ViolationKind::Provider,
                        path: "bupha.toml".into(),
                        spec: "repo".into(),
                        severity,
                        detail: format!(
                            "`[[repos]] name = {name:?}` has `dir = {dir:?}` not ending \
                             in `/{dirname}` — stale after rename?"
                        ),
                    });
                }
            }
        }
        if self_entries == 0 {
            out.push(SpecViolation {
                kind: ViolationKind::Provider,
                path: "bupha.toml".into(),
                spec: "repo".into(),
                severity,
                detail: format!(
                    "no `[[repos]]` entry named `*/{dirname}` — self-registration missing \
                     or stale after rename?"
                ),
            });
        }
        Ok(out)
    }
}

/// `lasius` provider — lasius-workspace facts the file-set tier
/// can't see. Locates the workspace root by walking ancestors of
/// `repo` for a `lasius.toml` (or an explicit `workspace = "<path>"`
/// claim), then evaluates the enabled claims:
///
/// - `remote_org = true` — `git remote get-url origin` must contain
///   `github.com/<org>/`, where `<org>` comes from the workspace
///   `lasius.toml` `org = "…"` key.
/// - `catalog = true` — `<workspace>/projects.yaml` must carry a
///   `projects:` entry named after the repo directory.
/// - `policy = true` — enforce the workspace `lasius.toml`
///   `[policy]` table: every `require_files` entry must exist in
///   the repo, every `require_worktrees` entry must be a directory.
/// - `severity = "warn" | "error"` — optional, default `"warn"`.
///
/// A workspace that can't be located degrades to one warn
/// violation — standalone clones still get the file-set tier.
/// Unknown claim keys are a [`ProviderError`].
#[derive(Debug, Default, Clone, Copy)]
pub struct LasiusProvider;

/// What a located lasius workspace tells the provider.
struct LasiusWorkspace {
    /// Workspace root (the dir containing `lasius.toml`).
    root: PathBuf,
    /// `org = "…"` from lasius.toml, when declared.
    org: Option<String>,
    /// `[policy] require_files` entries.
    require_files: Vec<String>,
    /// `[policy] require_worktrees` entries.
    require_worktrees: Vec<String>,
}

impl LasiusProvider {
    fn locate_workspace(repo: &Path, explicit: Option<&str>) -> Option<LasiusWorkspace> {
        let root = match explicit {
            Some(p) => PathBuf::from(p),
            None => repo
                .ancestors()
                .take(8)
                .find(|d| d.join("lasius.toml").is_file())?
                .to_path_buf(),
        };
        let text = std::fs::read_to_string(root.join("lasius.toml")).ok()?;
        let doc: toml::Value = text.parse().ok()?;
        let org = doc
            .get("org")
            .and_then(toml::Value::as_str)
            .map(str::to_owned);
        let policy = doc.get("policy").and_then(toml::Value::as_table);
        let strs = |key: &str| -> Vec<String> {
            policy
                .and_then(|p| p.get(key))
                .and_then(toml::Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        Some(LasiusWorkspace {
            root,
            org,
            require_files: strs("require_files"),
            require_worktrees: strs("require_worktrees"),
        })
    }
}

impl StandardProvider for LasiusProvider {
    fn ns(&self) -> &str {
        "lasius"
    }

    fn check(
        &self,
        repo: &Path,
        claims: &serde_json::Value,
    ) -> Result<Vec<SpecViolation>, ProviderError> {
        let obj = claims
            .as_object()
            .ok_or_else(|| ProviderError("`providers.lasius` claims must be a table".into()))?;
        for key in obj.keys() {
            if !matches!(
                key.as_str(),
                "workspace" | "remote_org" | "catalog" | "policy" | "severity"
            ) {
                return Err(ProviderError(format!(
                    "unknown `providers.lasius` claim `{key}`"
                )));
            }
        }
        let flag = |key: &str| obj.get(key).and_then(serde_json::Value::as_bool) == Some(true);
        let severity = match obj.get("severity").and_then(serde_json::Value::as_str) {
            None | Some("warn") => mimus_core::Severity::Warn,
            Some("error") => mimus_core::Severity::Error,
            Some(other) => {
                return Err(ProviderError(format!(
                    "`providers.lasius.severity` must be \"warn\" or \"error\", got {other:?}"
                )));
            }
        };
        let explicit = match obj.get("workspace") {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(w)) if w == "auto" => None,
            Some(serde_json::Value::String(w)) => Some(w.as_str()),
            Some(_) => {
                return Err(ProviderError(
                    "`providers.lasius.workspace` must be a path string or \"auto\"".into(),
                ))
            }
        };

        let Some(ws) = Self::locate_workspace(repo, explicit) else {
            return Ok(vec![SpecViolation {
                kind: ViolationKind::Provider,
                path: "providers.lasius".into(),
                spec: "lasius".into(),
                severity,
                detail: "no lasius.toml found — repo is outside a lasius workspace".into(),
            }]);
        };

        let dirname = repo
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut out = Vec::new();
        let mut violate = |detail: String| {
            out.push(SpecViolation {
                kind: ViolationKind::Provider,
                path: "lasius.toml".into(),
                spec: "lasius".into(),
                severity,
                detail,
            });
        };

        if flag("remote_org") {
            match ws.org.as_deref() {
                Some(org) => {
                    let url = std::process::Command::new("git")
                        .arg("-C")
                        .arg(repo)
                        .args(["remote", "get-url", "origin"])
                        .output();
                    match url {
                        Ok(o) if o.status.success() => {
                            let remote = String::from_utf8_lossy(&o.stdout);
                            // https://github.com/<org>/… and
                            // git@github.com:<org>/… (scp-like ssh)
                            // are both org-local.
                            let https = format!("github.com/{org}/");
                            let ssh = format!("github.com:{org}/");
                            if !remote.contains(&https) && !remote.contains(&ssh) {
                                violate(format!(
                                    "origin remote `{}` is not under github.com/{org}/",
                                    remote.trim()
                                ));
                            }
                        }
                        _ => violate("no `origin` remote — cannot verify org".into()),
                    }
                }
                None => violate("workspace lasius.toml has no `org` key".into()),
            }
        }

        if flag("catalog") {
            let catalog = ws.root.join("projects.yaml");
            match std::fs::read_to_string(&catalog) {
                Ok(text) => match serde_yaml::from_str::<serde_yaml::Value>(&text) {
                    Ok(doc) => {
                        let listed = doc
                            .get("projects")
                            .and_then(serde_yaml::Value::as_sequence)
                            .is_some_and(|projects| {
                                projects.iter().any(|p| {
                                    p.get("name").and_then(serde_yaml::Value::as_str)
                                        == Some(dirname.as_str())
                                })
                            });
                        if !listed {
                            violate(format!(
                                "`{}` has no `projects:` entry named `{dirname}`",
                                catalog.display()
                            ));
                        }
                    }
                    Err(err) => {
                        return Err(ProviderError(format!("{} parse: {err}", catalog.display())))
                    }
                },
                Err(_) => violate(format!("{} absent", catalog.display())),
            }
        }

        if flag("policy") {
            for f in &ws.require_files {
                if !repo.join(f).exists() {
                    violate(format!(
                        "lasius.toml `[policy] require_files` entry `{f}` absent"
                    ));
                }
            }
            for w in &ws.require_worktrees {
                if !repo.join(w).is_dir() {
                    violate(format!(
                        "lasius.toml `[policy] require_worktrees` entry `{w}` absent"
                    ));
                }
            }
        }

        Ok(out)
    }
}

/// Shared `severity = "warn" | "error"` claim parsing for the
/// org-marker providers — `default` applies when the claim is
/// absent. Unknown values are a [`ProviderError`].
fn severity_claim(
    obj: &serde_json::Map<String, serde_json::Value>,
    ns: &str,
    default: mimus_core::Severity,
) -> Result<mimus_core::Severity, ProviderError> {
    match obj.get("severity").and_then(serde_json::Value::as_str) {
        None => Ok(default),
        Some("warn") => Ok(mimus_core::Severity::Warn),
        Some("error") => Ok(mimus_core::Severity::Error),
        Some(other) => Err(ProviderError(format!(
            "`providers.{ns}.severity` must be \"warn\" or \"error\", got {other:?}"
        ))),
    }
}

/// Reject claim keys outside `allowed` — a typo'd claim must not
/// silently pass.
fn check_claim_keys(
    obj: &serde_json::Map<String, serde_json::Value>,
    ns: &str,
    allowed: &[&str],
) -> Result<(), ProviderError> {
    for key in obj.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(ProviderError(format!(
                "unknown `providers.{ns}` claim `{key}`"
            )));
        }
    }
    Ok(())
}

/// `kuda` provider — kuda-wired repo facts. Declaring
/// `[providers.kuda]` asserts the repo carries the kuda markers:
/// a `.kuda/` directory plus `kuda.toml` or `kuda.yaml`. On top of
/// that presence check, optional claims:
///
/// - `cards = true` — `.kuda/` must contain at least one `*.yaml`
///   card (recursive; `index.yaml` excluded — it's the generated
///   card index, not a card).
/// - `index = true` — `.kuda/index.sqlite` must exist. The index
///   is gitignored + rebuildable (`kuda sync` regenerates it), so
///   this finding is **always warn** regardless of `severity` —
///   presence drift must never gate.
/// - `severity = "warn" | "error"` — optional, default `"warn"`.
///
/// Card *content* drift is `kuda status`'s job — this provider is
/// presence-level only. Unknown claim keys are a [`ProviderError`].
#[derive(Debug, Default, Clone, Copy)]
pub struct KudaProvider;

impl StandardProvider for KudaProvider {
    fn ns(&self) -> &str {
        "kuda"
    }

    fn check(
        &self,
        repo: &Path,
        claims: &serde_json::Value,
    ) -> Result<Vec<SpecViolation>, ProviderError> {
        let obj = claims
            .as_object()
            .ok_or_else(|| ProviderError("`providers.kuda` claims must be a table".into()))?;
        check_claim_keys(obj, "kuda", &["cards", "index", "severity"])?;
        let flag = |key: &str| obj.get(key).and_then(serde_json::Value::as_bool) == Some(true);
        let severity = severity_claim(obj, "kuda", mimus_core::Severity::Warn)?;

        let kuda_dir = repo.join(".kuda");
        let mut out = Vec::new();
        let mut violate = |path: &str, sev: mimus_core::Severity, detail: String| {
            out.push(SpecViolation {
                kind: ViolationKind::Provider,
                path: path.into(),
                spec: "kuda".into(),
                severity: sev,
                detail,
            });
        };

        if !kuda_dir.is_dir() {
            violate(
                ".kuda",
                severity,
                "`.kuda/` directory absent — repo is not kuda-wired".into(),
            );
        }
        let has_config = repo.join("kuda.toml").is_file() || repo.join("kuda.yaml").is_file();
        if !has_config {
            violate(
                "kuda.toml",
                severity,
                "neither `kuda.toml` nor `kuda.yaml` present — kuda config missing".into(),
            );
        }

        if flag("cards") {
            // `.kuda/` cards are the checked-in `*.yaml` artifacts —
            // `index.yaml` is the generated card index, not a card.
            let mut stack = vec![kuda_dir.clone()];
            let mut cards = 0usize;
            while let Some(dir) = stack.pop() {
                let Ok(rd) = std::fs::read_dir(&dir) else {
                    continue;
                };
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else if p.extension().and_then(|x| x.to_str()) == Some("yaml")
                        && p.file_name().and_then(|n| n.to_str()) != Some("index.yaml")
                    {
                        cards += 1;
                    }
                }
            }
            if cards == 0 {
                violate(
                    ".kuda",
                    severity,
                    "`cards = true` but no `.kuda/**/*.yaml` cards found".into(),
                );
            }
        }

        if flag("index") && !kuda_dir.join("index.sqlite").is_file() {
            // gitignored + rebuildable — advisory only, never gates.
            violate(
                ".kuda/index.sqlite",
                mimus_core::Severity::Warn,
                "`index = true` but `.kuda/index.sqlite` absent — run `kuda sync`".into(),
            );
        }

        Ok(out)
    }
}

/// `sturn` provider — sturn-wired repo marker. Declaring
/// `[providers.sturn]` asserts `sturn.toml` is present; there are
/// no further claims (the WAL/refs state is sturn's own domain —
/// this is presence-level only).
///
/// - `severity = "warn" | "error"` — optional, default `"warn"`.
///
/// Unknown claim keys are a [`ProviderError`].
#[derive(Debug, Default, Clone, Copy)]
pub struct SturnProvider;

impl StandardProvider for SturnProvider {
    fn ns(&self) -> &str {
        "sturn"
    }

    fn check(
        &self,
        repo: &Path,
        claims: &serde_json::Value,
    ) -> Result<Vec<SpecViolation>, ProviderError> {
        let obj = claims
            .as_object()
            .ok_or_else(|| ProviderError("`providers.sturn` claims must be a table".into()))?;
        check_claim_keys(obj, "sturn", &["severity"])?;
        let severity = severity_claim(obj, "sturn", mimus_core::Severity::Warn)?;
        if !repo.join("sturn.toml").is_file() {
            return Ok(vec![SpecViolation {
                kind: ViolationKind::Provider,
                path: "sturn.toml".into(),
                spec: "sturn".into(),
                severity,
                detail: "`sturn.toml` absent — repo is not sturn-wired".into(),
            }]);
        }
        Ok(Vec::new())
    }
}

/// `myco` provider — myco ledger mount. Declaring
/// `[providers.myco]` asserts `myco.toml` is present (the myco
/// config marker); optional claims:
///
/// - `worktree = true` — `project/` must exist as a directory —
///   the myco ledger worktree mount. Dir-level only: whether it's
///   a healthy worktree is `myco status`'s job.
/// - `severity = "warn" | "error"` — optional, default `"warn"`.
///
/// Unknown claim keys are a [`ProviderError`].
#[derive(Debug, Default, Clone, Copy)]
pub struct MycoProvider;

impl StandardProvider for MycoProvider {
    fn ns(&self) -> &str {
        "myco"
    }

    fn check(
        &self,
        repo: &Path,
        claims: &serde_json::Value,
    ) -> Result<Vec<SpecViolation>, ProviderError> {
        let obj = claims
            .as_object()
            .ok_or_else(|| ProviderError("`providers.myco` claims must be a table".into()))?;
        check_claim_keys(obj, "myco", &["worktree", "severity"])?;
        let flag = |key: &str| obj.get(key).and_then(serde_json::Value::as_bool) == Some(true);
        let severity = severity_claim(obj, "myco", mimus_core::Severity::Warn)?;

        let mut out = Vec::new();
        let mut violate = |path: &str, detail: String| {
            out.push(SpecViolation {
                kind: ViolationKind::Provider,
                path: path.into(),
                spec: "myco".into(),
                severity,
                detail,
            });
        };

        if !repo.join("myco.toml").is_file() {
            violate(
                "myco.toml",
                "`myco.toml` absent — repo is not myco-wired".into(),
            );
        }
        if flag("worktree") && !repo.join("project").is_dir() {
            violate(
                "project",
                "`worktree = true` but `project/` ledger worktree absent".into(),
            );
        }
        Ok(out)
    }
}

/// `cypera` provider — cypera docs-generation contract marker.
/// Declaring `[providers.cypera]` asserts `cypera.toml` exists and
/// parses as TOML. Optional claims:
///
/// - `sections = ["crate_map", "cli", …]` — each named top-level
///   key must be present in `cypera.toml` (cheap structural check;
///   contents are cypera's own schema).
/// - `severity = "warn" | "error"` — optional, default `"warn"`.
///
/// **Boundary:** this provider is file/parse-level only — it never
/// shells out to `cypera check`/`cypera gen`. Providers must stay
/// fast and mechanical; semantic validation of the cypera document
/// is `cypera check`'s job. Unknown claim keys are a
/// [`ProviderError`].
#[derive(Debug, Default, Clone, Copy)]
pub struct CyperaProvider;

impl StandardProvider for CyperaProvider {
    fn ns(&self) -> &str {
        "cypera"
    }

    fn check(
        &self,
        repo: &Path,
        claims: &serde_json::Value,
    ) -> Result<Vec<SpecViolation>, ProviderError> {
        let obj = claims
            .as_object()
            .ok_or_else(|| ProviderError("`providers.cypera` claims must be a table".into()))?;
        check_claim_keys(obj, "cypera", &["sections", "severity"])?;
        let severity = severity_claim(obj, "cypera", mimus_core::Severity::Warn)?;

        let manifest = repo.join("cypera.toml");
        if !manifest.is_file() {
            return Ok(vec![SpecViolation {
                kind: ViolationKind::Provider,
                path: "cypera.toml".into(),
                spec: "cypera".into(),
                severity,
                detail: "`cypera.toml` absent — repo has no cypera contract".into(),
            }]);
        }
        let text = std::fs::read_to_string(&manifest)
            .map_err(|err| ProviderError(format!("cannot read {}: {err}", manifest.display())))?;
        let doc: toml::Value = text
            .parse()
            .map_err(|err| ProviderError(format!("cypera.toml parse: {err}")))?;

        let mut out = Vec::new();
        if let Some(sections) = obj.get("sections") {
            let arr = sections.as_array().ok_or_else(|| {
                ProviderError("`providers.cypera.sections` must be an array of strings".into())
            })?;
            for s in arr {
                let name = s.as_str().ok_or_else(|| {
                    ProviderError("`providers.cypera.sections` entries must be strings".into())
                })?;
                if doc.get(name).is_none() {
                    out.push(SpecViolation {
                        kind: ViolationKind::Provider,
                        path: "cypera.toml".into(),
                        spec: "cypera".into(),
                        severity,
                        detail: format!("`sections` claim: top-level `[{name}]` absent"),
                    });
                }
            }
        }
        Ok(out)
    }
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
    /// tables → `predicate`; `[git] untracked` globs → `predicate`;
    /// `[providers.<ns>]` tables → `provider`. `check_missing`/
    /// `check_unlisted` keep file-set semantics — predicate and
    /// provider evaluation live only here.
    ///
    /// Providers run against an **empty** [`ProviderSet`] — every
    /// declared namespace degrades to a `provider` warn violation.
    /// Callers holding real providers use
    /// [`Spec::check_repo_with`].
    ///
    /// # Errors
    /// [`SpecError::Glob`] on a malformed `files`/`cover`/`git`
    /// pattern, [`SpecError::Predicate`] on an unknown path-table
    /// key, [`SpecError::PredicateValue`] on a mistyped predicate
    /// value or uncompilable regex, [`SpecError::Provider`] when a
    /// registered provider fails.
    pub fn check_repo(&self, repo: &Path) -> Result<Vec<SpecViolation>, SpecError> {
        self.check_repo_with(repo, &ProviderSet::new())
    }

    /// [`Spec::check_repo`] with a provider registry — the
    /// `[providers.<ns>]` tier dispatches each declared namespace
    /// to the [`StandardProvider`] registered under it. Runs last,
    /// after file-set + path predicates. An unregistered namespace
    /// yields a `provider` **warn** violation regardless of the
    /// spec's own severity (providers are optional by design —
    /// degrade, never fail).
    ///
    /// # Errors
    /// Same as [`Spec::check_repo`], plus [`SpecError::Provider`]
    /// when a registered provider's `check` fails.
    pub fn check_repo_with(
        &self,
        repo: &Path,
        providers: &ProviderSet,
    ) -> Result<Vec<SpecViolation>, SpecError> {
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
        out.extend(self.check_providers(repo, providers)?);
        Ok(out)
    }

    /// `[providers.<ns>]` dispatch — each declared namespace routes
    /// to the registered [`StandardProvider`]; unregistered
    /// namespaces degrade to a `provider` warn violation
    /// (`provider-unavailable`), never a hard fail.
    fn check_providers(
        &self,
        repo: &Path,
        providers: &ProviderSet,
    ) -> Result<Vec<SpecViolation>, SpecError> {
        let mut out = Vec::new();
        for (ns, claims) in &self.providers {
            match providers.get(ns) {
                Some(p) => {
                    let vs = p.check(repo, claims).map_err(|err| SpecError::Provider {
                        ns: ns.clone(),
                        source: err,
                    })?;
                    out.extend(vs);
                }
                None => out.push(self.violation(
                    ViolationKind::Provider,
                    &format!("providers.{ns}"),
                    mimus_core::Severity::Warn,
                    format!(
                        "provider namespace `{ns}` has no registered provider — claims not evaluated"
                    ),
                )),
            }
        }
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
        // `providers` is a typed field now — the `<ns>` claims
        // tables land in `Spec::providers`, not `extra`
        let lasius = s.providers.get("lasius").unwrap();
        assert_eq!(lasius["require"], "clean-clone");
        assert!(!s.extra.contains_key("providers"));
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

    // --- P-002 provider tier ------------------------------------

    /// Records the claims `Value` it was called with; returns one
    /// fixed provider finding so tests can assert pass-through.
    struct FakeProvider {
        ns: &'static str,
        seen: std::sync::Arc<std::sync::Mutex<Option<serde_json::Value>>>,
    }

    impl StandardProvider for FakeProvider {
        fn ns(&self) -> &str {
            self.ns
        }

        fn check(
            &self,
            _repo: &Path,
            claims: &serde_json::Value,
        ) -> Result<Vec<SpecViolation>, ProviderError> {
            *self.seen.lock().unwrap() = Some(claims.clone());
            Ok(vec![SpecViolation {
                kind: ViolationKind::Provider,
                path: format!("providers.{}", self.ns),
                spec: "fake-provider".into(),
                severity: mimus_core::Severity::Error,
                detail: "fake finding".into(),
            }])
        }
    }

    #[test]
    fn providers_table_parses() {
        let s = Spec::parse(
            r#"
family = "t"

[providers.bupha]
context = "ci/bupha"

[providers.lasius]
clone = "present"
"#,
        )
        .unwrap();
        assert_eq!(s.providers.len(), 2);
        assert_eq!(s.providers["bupha"]["context"], "ci/bupha");
        assert_eq!(s.providers["lasius"]["clone"], "present");
        assert!(s.extra.is_empty());
    }

    #[test]
    fn unregistered_provider_ns_warns_never_fails() {
        // severity=fail spec, but provider-unavailable is always
        // warn — providers are optional by design
        let s = Spec::parse(
            r#"
family = "t"
severity = "fail"
files = ["a.txt"]

[providers.bupha]
context = "ci/bupha"
"#,
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "x").unwrap();
        let vs = s.check_repo(dir.path()).unwrap();
        assert_eq!(vs.len(), 1, "{vs:?}");
        assert_eq!(vs[0].kind, ViolationKind::Provider);
        assert_eq!(vs[0].severity, mimus_core::Severity::Warn);
        assert_eq!(vs[0].path, "providers.bupha");
        assert!(vs[0].detail.contains("bupha"));
    }

    #[test]
    fn registered_provider_gets_claims_verbatim() {
        let s = Spec::parse(
            r#"
family = "t"
files = ["a.txt"]

[providers.fake]
known = "yes"
unknown.typo = "provider's problem, not ours"
"#,
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "x").unwrap();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
        let mut set = ProviderSet::new();
        set.register(FakeProvider {
            ns: "fake",
            seen: seen.clone(),
        });
        let vs = s.check_repo_with(dir.path(), &set).unwrap();
        // the provider's own violation passes through untouched —
        // its spec/severity are the provider's to set
        assert_eq!(vs.len(), 1, "{vs:?}");
        assert_eq!(vs[0].kind, ViolationKind::Provider);
        assert_eq!(vs[0].spec, "fake-provider");
        assert_eq!(vs[0].severity, mimus_core::Severity::Error);
        // claims Value arrived verbatim, unknown keys included
        let got = seen.lock().unwrap().clone().unwrap();
        assert_eq!(got["known"], "yes");
        assert_eq!(got["unknown"]["typo"], "provider's problem, not ours");
    }

    #[test]
    fn provider_error_surfaces_as_spec_error() {
        struct Boom;
        impl StandardProvider for Boom {
            fn ns(&self) -> &str {
                "boom"
            }
            fn check(
                &self,
                _repo: &Path,
                _claims: &serde_json::Value,
            ) -> Result<Vec<SpecViolation>, ProviderError> {
                Err(ProviderError("probe failed".into()))
            }
        }
        let s = Spec::parse("family='t'\nfiles=['a.txt']\n\n[providers.boom]\nx = 1\n").unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "x").unwrap();
        let mut set = ProviderSet::new();
        set.register(Boom);
        let err = s.check_repo_with(dir.path(), &set).unwrap_err();
        match err {
            SpecError::Provider { ns, source } => {
                assert_eq!(ns, "boom");
                assert_eq!(source.to_string(), "probe failed");
            }
            other => panic!("expected Provider error, got {other}"),
        }
    }

    #[test]
    fn provider_set_register_replaces_same_ns() {
        let mut set = ProviderSet::new();
        assert!(set.is_empty());
        set.register(FakeProvider {
            ns: "fake",
            seen: std::sync::Arc::new(std::sync::Mutex::new(None)),
        });
        set.register(FakeProvider {
            ns: "fake",
            seen: std::sync::Arc::new(std::sync::Mutex::new(None)),
        });
        assert_eq!(set.len(), 1);
        assert!(set.get("fake").is_some());
        assert!(set.get("other").is_none());
    }

    fn cargo_claims() -> serde_json::Value {
        serde_json::json!({ "patches": "org-only" })
    }

    #[test]
    fn cargo_provider_accepts_org_local_patches() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            r#"
[package]
name = "x"

[patch."https://github.com/handl-lang/kernel"]
handl-language = { path = "../kernel/crates/language" }
other = { git = "ssh://git@github.com/handl-lang/engine.git" }
pinned = { version = "1.2.3" }

[patch.crates-io]
serde = { git = "https://github.com/handl-lang/serde-shim.git" }
"#,
        )
        .unwrap();
        let vs = CargoProvider.check(dir.path(), &cargo_claims()).unwrap();
        assert!(vs.is_empty(), "{vs:?}");
    }

    #[test]
    fn cargo_provider_flags_external_patch_source() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[patch.\"https://github.com/rust-lang/crates.io-index\"]\nfoo = { git = \"https://github.com/evil/x.git\" }\n",
        )
        .unwrap();
        let vs = CargoProvider.check(dir.path(), &cargo_claims()).unwrap();
        assert_eq!(vs.len(), 2, "{vs:?}"); // bad src + bad git entry
        assert!(vs.iter().all(|v| v.kind == ViolationKind::Provider));
        assert!(vs.iter().all(|v| v.severity == mimus_core::Severity::Error));
    }

    #[test]
    fn cargo_provider_flags_non_sibling_path_and_registry() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            r#"
[patch."https://github.com/handl-lang/kernel"]
a = { path = "vendor/kernel" }
b = { registry = "https://example.com" }
"#,
        )
        .unwrap();
        let vs = CargoProvider.check(dir.path(), &cargo_claims()).unwrap();
        assert_eq!(vs.len(), 2, "{vs:?}");
    }

    #[test]
    fn cargo_provider_no_patch_table_is_clean() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        let vs = CargoProvider.check(dir.path(), &cargo_claims()).unwrap();
        assert!(vs.is_empty(), "{vs:?}");
    }

    #[test]
    fn cargo_provider_rejects_bad_claims() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
        for claims in [
            serde_json::json!({ "patches": "anything" }),
            serde_json::json!({ "patches": "org-only", "bogus": 1 }),
            serde_json::json!({ "severity": "warn" }),
            serde_json::json!("nope"),
        ] {
            assert!(
                CargoProvider.check(dir.path(), &claims).is_err(),
                "{claims}"
            );
        }
        // severity warn downgrades
        let vs = CargoProvider
            .check(
                dir.path(),
                &serde_json::json!({ "patches": "org-only", "severity": "warn" }),
            )
            .unwrap();
        assert!(vs.is_empty());
    }

    #[test]
    fn repo_provider_bupha_name_matches_dirname() {
        let dir = tempfile::tempdir().unwrap();
        // tempdir name is random — create a named child dir instead
        let repo = dir.path().join("my-repo");
        std::fs::create_dir(&repo).unwrap();
        std::fs::write(
            repo.join("bupha.toml"),
            "[[repos]]\nname = \"handl-lang/my-repo\"\ndir = \"~/GitHub/handl-lang/my-repo\"\n",
        )
        .unwrap();
        let claims = serde_json::json!({ "bupha-name": true });
        assert!(RepoProvider.check(&repo, &claims).unwrap().is_empty());
    }

    #[test]
    fn repo_provider_bupha_name_flags_stale() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("new-name");
        std::fs::create_dir(&repo).unwrap();
        // self-entry under the OLD name — the rename-drift case
        std::fs::write(
            repo.join("bupha.toml"),
            "[[repos]]\nname = \"handl-lang/old-name\"\ndir = \"~/GitHub/handl-lang/old-name\"\n",
        )
        .unwrap();
        let vs = RepoProvider
            .check(&repo, &serde_json::json!({ "bupha-name": true }))
            .unwrap();
        assert_eq!(vs.len(), 1, "{vs:?}");
        assert_eq!(vs[0].severity, mimus_core::Severity::Warn);
        assert!(vs[0].detail.contains("new-name"));
    }

    #[test]
    fn repo_provider_bupha_dir_drift_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("my-repo");
        std::fs::create_dir(&repo).unwrap();
        std::fs::write(
            repo.join("bupha.toml"),
            "[[repos]]\nname = \"handl-lang/my-repo\"\ndir = \"~/GitHub/handl-lang/other-dir\"\n",
        )
        .unwrap();
        let vs = RepoProvider
            .check(&repo, &serde_json::json!({ "bupha-name": true }))
            .unwrap();
        assert_eq!(vs.len(), 1, "{vs:?}");
        assert!(vs[0].detail.contains("other-dir"));
    }

    #[test]
    fn repo_provider_shared_manifest_ignores_other_repos() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("docs");
        std::fs::create_dir(&repo).unwrap();
        std::fs::write(
            repo.join("bupha.toml"),
            "[[repos]]\nname = \"handl-lang/host\"\ndir = \"~/GitHub/handl-lang/host\"\n\n\
             [[repos]]\nname = \"handl-lang/docs\"\ndir = \"~/GitHub/handl-lang/docs\"\n",
        )
        .unwrap();
        let vs = RepoProvider
            .check(&repo, &serde_json::json!({ "bupha-name": true }))
            .unwrap();
        assert!(vs.is_empty(), "{vs:?}");
    }

    #[test]
    fn repo_provider_absent_bupha_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let vs = RepoProvider
            .check(dir.path(), &serde_json::json!({ "bupha-name": true }))
            .unwrap();
        assert!(vs.is_empty(), "{vs:?}");
    }

    // --- org-parameterized cargo + lasius provider ----------------

    #[test]
    fn cargo_provider_honors_org_claim() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            r#"
[patch."https://github.com/9aia/kuda"]
kuda-core = { path = "../kuda/crates/kuda-core" }
"#,
        )
        .unwrap();
        // 9aia org → clean
        let vs = CargoProvider
            .check(
                dir.path(),
                &serde_json::json!({ "patches": "org-only", "org": "9aia" }),
            )
            .unwrap();
        assert!(vs.is_empty(), "{vs:?}");
        // default org (handl-lang) → the same patch is external
        let vs = CargoProvider.check(dir.path(), &cargo_claims()).unwrap();
        assert_eq!(vs.len(), 1, "{vs:?}");
        assert!(vs[0].detail.contains("9aia"));
    }

    /// Build `<tmp>/ws/{lasius.toml,projects.yaml,repos/<name>}`.
    fn lasius_fixture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("ws");
        let repo = ws.join("repos").join("my-repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(
            ws.join("lasius.toml"),
            "org = \"9aia\"\n\n[policy]\nrequire_files = [\"AGENTS.md\"]\nrequire_worktrees = [\"project\"]\n",
        )
        .unwrap();
        std::fs::write(
            ws.join("projects.yaml"),
            "projects:\n  - name: my-repo\n    tagline: x\n",
        )
        .unwrap();
        (dir, repo)
    }

    #[test]
    fn lasius_provider_policy_and_catalog_pass() {
        let (_dir, repo) = lasius_fixture();
        std::fs::write(repo.join("AGENTS.md"), "x").unwrap();
        std::fs::create_dir(repo.join("project")).unwrap();
        let claims = serde_json::json!({
            "policy": true,
            "catalog": true,
            "remote_org": true,
        });
        let vs = LasiusProvider.check(&repo, &claims).unwrap();
        // remote_org fails — fixture repo has no git remote; the
        // other two claims are clean
        assert_eq!(vs.len(), 1, "{vs:?}");
        assert!(vs[0].detail.contains("origin"));
        assert_eq!(vs[0].severity, mimus_core::Severity::Warn);
    }

    #[test]
    fn lasius_provider_flags_policy_and_catalog_drift() {
        let (_dir, repo) = lasius_fixture();
        // AGENTS.md + project/ missing → policy violations; repo not
        // in projects.yaml? it is — drop catalog check here
        let claims = serde_json::json!({ "policy": true });
        let vs = LasiusProvider.check(&repo, &claims).unwrap();
        assert_eq!(vs.len(), 2, "{vs:?}");
        assert!(vs.iter().all(|v| v.kind == ViolationKind::Provider));
    }

    #[test]
    fn lasius_provider_catalog_flags_unlisted_repo() {
        let (dir, repo) = lasius_fixture();
        // rename the repo dir — catalog entry no longer matches
        let renamed = repo.with_file_name("other-repo");
        std::fs::rename(&repo, &renamed).unwrap();
        let claims = serde_json::json!({ "catalog": true });
        let vs = LasiusProvider.check(&renamed, &claims).unwrap();
        assert_eq!(vs.len(), 1, "{vs:?}");
        assert!(vs[0].detail.contains("other-repo"));
        drop(dir);
    }

    #[test]
    fn lasius_provider_outside_workspace_warns() {
        let dir = tempfile::tempdir().unwrap();
        let claims = serde_json::json!({ "policy": true });
        let vs = LasiusProvider.check(dir.path(), &claims).unwrap();
        assert_eq!(vs.len(), 1, "{vs:?}");
        assert!(vs[0].detail.contains("lasius.toml"));
    }

    #[test]
    fn lasius_provider_rejects_bad_claims() {
        let dir = tempfile::tempdir().unwrap();
        for claims in [
            serde_json::json!({ "bogus": true }),
            serde_json::json!({ "severity": "fatal" }),
            serde_json::json!("nope"),
        ] {
            assert!(
                LasiusProvider.check(dir.path(), &claims).is_err(),
                "{claims}"
            );
        }
    }

    // --- org-marker providers (kuda/sturn/myco/cypera) ------------

    /// `<tmp>/repo` with `.kuda/{modules/x.yaml,index.yaml,index.sqlite}`
    /// + `kuda.toml` — a fully wired kuda fixture.
    fn kuda_fixture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        let kuda = repo.join(".kuda");
        std::fs::create_dir_all(kuda.join("modules")).unwrap();
        std::fs::write(repo.join("kuda.toml"), "[repo]\n").unwrap();
        std::fs::write(kuda.join("modules").join("x.yaml"), "name: x\n").unwrap();
        std::fs::write(kuda.join("index.yaml"), "cards: []\n").unwrap();
        std::fs::write(kuda.join("index.sqlite"), "db").unwrap();
        (dir, repo)
    }

    #[test]
    fn kuda_provider_wired_repo_is_clean() {
        let (_dir, repo) = kuda_fixture();
        let claims = serde_json::json!({ "cards": true, "index": true });
        let vs = KudaProvider.check(&repo, &claims).unwrap();
        assert!(vs.is_empty(), "{vs:?}");
    }

    #[test]
    fn kuda_provider_flags_missing_markers() {
        let dir = tempfile::tempdir().unwrap();
        let vs = KudaProvider
            .check(dir.path(), &serde_json::json!({}))
            .unwrap();
        assert_eq!(vs.len(), 2, "{vs:?}"); // .kuda/ + config
        assert!(vs.iter().all(|v| v.severity == mimus_core::Severity::Warn));
        // kuda.yaml alone satisfies the config marker
        std::fs::create_dir(dir.path().join(".kuda")).unwrap();
        std::fs::write(dir.path().join("kuda.yaml"), "repo: x\n").unwrap();
        let vs = KudaProvider
            .check(dir.path(), &serde_json::json!({}))
            .unwrap();
        assert!(vs.is_empty(), "{vs:?}");
    }

    #[test]
    fn kuda_provider_cards_requires_yaml_cards() {
        let (_dir, repo) = kuda_fixture();
        // remove the real card, keep only index.yaml → cards fails
        std::fs::remove_file(repo.join(".kuda/modules/x.yaml")).unwrap();
        let vs = KudaProvider
            .check(&repo, &serde_json::json!({ "cards": true }))
            .unwrap();
        assert_eq!(vs.len(), 1, "{vs:?}");
        assert!(vs[0].detail.contains("cards"));
    }

    #[test]
    fn kuda_provider_index_is_always_warn() {
        let (_dir, repo) = kuda_fixture();
        std::fs::remove_file(repo.join(".kuda/index.sqlite")).unwrap();
        // severity=error must NOT escalate the gitignored index
        let vs = KudaProvider
            .check(
                &repo,
                &serde_json::json!({ "index": true, "severity": "error" }),
            )
            .unwrap();
        assert_eq!(vs.len(), 1, "{vs:?}");
        assert_eq!(vs[0].severity, mimus_core::Severity::Warn);
        assert!(vs[0].detail.contains("kuda sync"));
    }

    #[test]
    fn kuda_provider_rejects_bad_claims() {
        let dir = tempfile::tempdir().unwrap();
        for claims in [
            serde_json::json!({ "bogus": true }),
            serde_json::json!({ "severity": "fatal" }),
            serde_json::json!("nope"),
        ] {
            assert!(KudaProvider.check(dir.path(), &claims).is_err(), "{claims}");
        }
    }

    #[test]
    fn sturn_provider_presence_and_severity() {
        let dir = tempfile::tempdir().unwrap();
        // absent → warn by default
        let vs = SturnProvider
            .check(dir.path(), &serde_json::json!({}))
            .unwrap();
        assert_eq!(vs.len(), 1, "{vs:?}");
        assert_eq!(vs[0].severity, mimus_core::Severity::Warn);
        assert_eq!(vs[0].path, "sturn.toml");
        // present → clean; severity=error honored on a fresh repo
        std::fs::write(dir.path().join("sturn.toml"), "[repo]\n").unwrap();
        let vs = SturnProvider
            .check(dir.path(), &serde_json::json!({ "severity": "error" }))
            .unwrap();
        assert!(vs.is_empty(), "{vs:?}");
        for claims in [
            serde_json::json!({ "bogus": true }),
            serde_json::json!({ "severity": "fatal" }),
            serde_json::json!("nope"),
        ] {
            assert!(
                SturnProvider.check(dir.path(), &claims).is_err(),
                "{claims}"
            );
        }
    }

    #[test]
    fn myco_provider_toml_and_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        // no myco.toml → one violation even with worktree claim
        let vs = MycoProvider
            .check(&repo, &serde_json::json!({ "worktree": true }))
            .unwrap();
        assert_eq!(vs.len(), 2, "{vs:?}"); // myco.toml + project/
                                           // myco.toml only → worktree claim still fires
        std::fs::write(repo.join("myco.toml"), "[repo]\n").unwrap();
        let vs = MycoProvider
            .check(&repo, &serde_json::json!({ "worktree": true }))
            .unwrap();
        assert_eq!(vs.len(), 1, "{vs:?}");
        assert_eq!(vs[0].path, "project");
        // project/ mounted → clean
        std::fs::create_dir(repo.join("project")).unwrap();
        let vs = MycoProvider
            .check(&repo, &serde_json::json!({ "worktree": true }))
            .unwrap();
        assert!(vs.is_empty(), "{vs:?}");
        // without the claim, project/ absence is not this provider's job
        std::fs::remove_dir(repo.join("project")).unwrap();
        let vs = MycoProvider.check(&repo, &serde_json::json!({})).unwrap();
        assert!(vs.is_empty(), "{vs:?}");
        for claims in [
            serde_json::json!({ "bogus": true }),
            serde_json::json!({ "severity": "fatal" }),
            serde_json::json!("nope"),
        ] {
            assert!(MycoProvider.check(&repo, &claims).is_err(), "{claims}");
        }
    }

    #[test]
    fn cypera_provider_parse_and_sections() {
        let dir = tempfile::tempdir().unwrap();
        // absent → violation
        let vs = CyperaProvider
            .check(dir.path(), &serde_json::json!({}))
            .unwrap();
        assert_eq!(vs.len(), 1, "{vs:?}");
        assert_eq!(vs[0].path, "cypera.toml");
        // unparsable → ProviderError, not a silent pass
        std::fs::write(dir.path().join("cypera.toml"), "not = [toml\n").unwrap();
        assert!(CyperaProvider
            .check(dir.path(), &serde_json::json!({}))
            .is_err());
        // parses + declared sections present → clean
        std::fs::write(
            dir.path().join("cypera.toml"),
            "[crate_map]\norder = [\"domain\"]\n\n[cli]\npackage = \"x\"\n",
        )
        .unwrap();
        let vs = CyperaProvider
            .check(
                dir.path(),
                &serde_json::json!({ "sections": ["crate_map", "cli"] }),
            )
            .unwrap();
        assert!(vs.is_empty(), "{vs:?}");
        // missing section → violation
        let vs = CyperaProvider
            .check(
                dir.path(),
                &serde_json::json!({ "sections": ["crate_map", "config"] }),
            )
            .unwrap();
        assert_eq!(vs.len(), 1, "{vs:?}");
        assert!(vs[0].detail.contains("config"));
        // malformed sections claim → error
        for claims in [
            serde_json::json!({ "sections": "crate_map" }),
            serde_json::json!({ "sections": [1] }),
            serde_json::json!({ "bogus": true }),
            serde_json::json!({ "severity": "fatal" }),
            serde_json::json!("nope"),
        ] {
            assert!(
                CyperaProvider.check(dir.path(), &claims).is_err(),
                "{claims}"
            );
        }
    }
}
