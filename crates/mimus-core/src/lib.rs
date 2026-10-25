//! mimus-core — domain: claims, violations, evidence, trust
//! scores, and the `mimus.toml` config model (ADR-0001).
//!
//! The evidence bar: a [`Violation`] is claim + evidence + fix.
//! No evidence, no finding — unsure checkers return "no finding",
//! never a guess.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// A factual assertion extracted from prose, comments, or cards.
#[derive(Debug, Clone, Serialize)]
pub struct Claim {
    pub kind: ClaimKind,
    /// The asserted text — "cargo test -p foo", "crates/x/", "357".
    pub text: String,
    /// File the claim was found in.
    pub source: PathBuf,
    /// 1-based line in `source`.
    pub line: u32,
}

/// What kind of assertion — decides which [`Checker`] can prove
/// or refute it (ADR-0001 taxonomy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClaimKind {
    /// A file/dir path that should exist.
    Path,
    /// A shell command that should parse and run clean.
    Command,
    /// A symbol name that should resolve in the index.
    Symbol,
    /// An asserted number that should recompute.
    Count,
    /// A URL that should resolve.
    Link,
    /// A prose claim about behavior — semantic tier only.
    Behavior,
}

/// A claim proven false — with the proof attached.
#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    pub claim: Claim,
    /// What reality says instead — the proof.
    pub evidence: String,
    /// Suggested correction when mechanical.
    pub fix: Option<String>,
    pub severity: Severity,
}

/// `Error` fails `mimus check`; `Warn` only appears in reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warn,
}

/// Trust verdict for a memory artifact (`mimus audit`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustScore {
    /// Provenance hashes match current content.
    Fresh,
    /// Files changed since the card was built — regenerate.
    Drifted,
    /// Card asserts something provably false — attention needed.
    Contradicted,
}

// ---- graph tier (ADR-0002) ----

/// A module→module edge the code index recorded — `(importer,
/// relation, target)` is already a triple. Pure data; the
/// index-read lives in adapter crates (`mimus-check-graph`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Edge {
    /// Importing/declaring module (always an indexed module).
    pub from: String,
    /// Target — an indexed module name, or an external dep name
    /// that resolved to nothing at index time.
    pub to: String,
    pub kind: EdgeKind,
}

/// Edge relation — the kuda index's `edges.kind` strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    /// `import x` / `use x` inside a module's sources.
    Imports,
    /// A manifest-declared dependency (Cargo.toml, package.json).
    PackageDep,
    /// Workspace membership — unambiguously internal.
    WorkspaceMember,
    /// Call-graph edge — a different relation entirely; satisfies
    /// no import/dep claim and isn't staleness-checked.
    Calls,
    /// A kind this mimus doesn't know — never flagged, still
    /// checked for a missing source module.
    Other,
}

/// A structural snapshot of the indexed code: edges plus the
/// module/symbol sets they resolve against. Set-level classes
/// (stale-reference, unsupported-claim) compare claims to this —
/// see `mimus_check::GraphChecker`.
#[derive(Debug, Default)]
pub struct Graph {
    pub edges: Vec<Edge>,
    /// Indexed module names (kuda: `DISTINCT module FROM symbols`).
    pub modules: std::collections::HashSet<String>,
    /// Indexed symbol names — the phase-2+ surface (per-file
    /// triples, undocumented fact).
    pub symbols: std::collections::HashSet<String>,
    /// Files each module was indexed from — the phase-4 provenance:
    /// `undocumented-fact` honors `--diff` by asking whether a
    /// module's own sources changed.
    pub module_files: std::collections::HashMap<String, std::collections::HashSet<PathBuf>>,
    /// Provenance — the index file this snapshot was read from.
    /// Violations sourced from the graph point here.
    pub index: PathBuf,
}

// ---- mimus.toml config model ----

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub sources: SourcesCfg,
    #[serde(default)]
    pub check: CheckCfg,
    #[serde(default)]
    pub rules: RulesCfg,
    #[serde(default)]
    pub report: ReportCfg,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct SourcesCfg {
    /// Doc glob patterns mined for claims.
    #[serde(default = "default_docs")]
    pub docs: Vec<String>,
    /// Extract claims from code comments too.
    #[serde(default)]
    pub comments: bool,
    /// Glob for memory cards to audit (empty = skip).
    #[serde(default = "default_cards_glob")]
    pub cards: String,
}

impl Default for SourcesCfg {
    fn default() -> Self {
        Self {
            docs: default_docs(),
            comments: true,
            cards: default_cards_glob(),
        }
    }
}

fn default_docs() -> Vec<String> {
    vec![
        "README.md".into(),
        "docs/**/*.md".into(),
        "AGENTS.md".into(),
        "project/**/*.md".into(),
    ]
}
fn default_cards_glob() -> String {
    ".kuda/modules/*.yaml".into()
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct CheckCfg {
    /// Mechanical tier — deterministic, CI-gateable.
    #[serde(default = "default_true")]
    pub mechanical: bool,
    /// Semantic tier — advisory only, can never gate.
    #[serde(default)]
    pub semantic: bool,
    /// kuda index path for the symbol checker.
    #[serde(default)]
    pub kuda_index: Option<PathBuf>,
    /// ACP agent command for the semantic checker
    /// (e.g. "cline --acp"). Semantic tier is advisory only.
    #[serde(default)]
    pub acp_cmd: Option<String>,
    /// Extra repo-relative roots path claims resolve against —
    /// for docs that reference paths relative to a component
    /// root (e.g. "infrastructure/postgres").
    #[serde(default)]
    pub base_dirs: Vec<PathBuf>,
    /// Only check claims of these kinds (empty = all).
    #[serde(default)]
    pub kinds: Vec<ClaimKind>,
    /// Skip claims whose text starts with any of these prefixes.
    #[serde(default)]
    pub ignore_prefixes: Vec<String>,
    /// Tool prerequisites the repo declares — commands invoking
    /// these skip the PATH probe (docs may legitimately require
    /// tools the checker's machine lacks).
    #[serde(default)]
    pub binaries: Vec<String>,
    /// Subject aliases for the contradiction buckets — a
    /// `[check.aliases]` table mapping `alias = "canonical"`;
    /// emitted bucket keys name the canonical form (ADR-0002).
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
}

impl Default for CheckCfg {
    fn default() -> Self {
        Self {
            mechanical: true,
            semantic: false,
            kuda_index: None,
            acp_cmd: None,
            base_dirs: Vec::new(),
            kinds: Vec::new(),
            ignore_prefixes: Vec::new(),
            binaries: Vec::new(),
            aliases: BTreeMap::new(),
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct RulesCfg {
    /// Dir of *.toml rule files.
    #[serde(default = "default_rules_dir")]
    pub dir: PathBuf,
    /// Rule violations also fail `check` (not just report).
    #[serde(default)]
    pub strict: bool,
}

impl Default for RulesCfg {
    fn default() -> Self {
        Self {
            dir: default_rules_dir(),
            strict: false,
        }
    }
}

fn default_rules_dir() -> PathBuf {
    "rules".into()
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ReportCfg {
    /// "human" | "json".
    #[serde(default = "default_format")]
    pub format: String,
    /// Suppress low-evidence findings ("high" | "all").
    #[serde(default = "default_confidence")]
    pub min_confidence: String,
}

impl Default for ReportCfg {
    fn default() -> Self {
        Self {
            format: default_format(),
            min_confidence: default_confidence(),
        }
    }
}

fn default_format() -> String {
    "human".into()
}
fn default_confidence() -> String {
    "high".into()
}
