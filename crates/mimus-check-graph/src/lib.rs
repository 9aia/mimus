//! Graph adapter — set-level checking (ADR-0002): index-edge
//! classes against the kuda index (phase 1) plus index-free prose
//! contradiction buckets (phase 2). Consumes the index *file
//! format*, not the kuda crates — the schema is the contract,
//! same as `mimus-check-symbol`.
//!
//! Classes:
//! - `stale-reference` (error): an `imports`/`package-dep`/
//!   `workspace-member` edge whose target resolves to no indexed
//!   module — or whose *source* module is gone entirely. Internal
//!   vs external is decided conservatively: an unresolved target
//!   flags only when it shares the repo's own naming family (a
//!   '/'-scope or a prefix segment carried by ≥2 indexed modules);
//!   bare external-looking names (`serde`, `react`,
//!   `github.com/ext/x`) are silence.
//! - `unsupported-claim` (warn): prose of the shape
//!   `X imports|depends on|requires|uses Y` where both sides
//!   resolve to indexed modules and the index records no such
//!   edge. Either side unresolvable → the triple is unproveable →
//!   silence (evidence-or-silence, ADR-0001).
//! - `contradiction` / `self-contradiction` (warn, phase 2):
//!   prose claims reduce to `(subject, relation, object)` triples
//!   (mimus-claims::triples) and conflicts fall out of the
//!   `(subject, relation)` bucket — cross-source for doc↔doc,
//!   reflexive when one file asserts two objects for the same
//!   slot. No index needed: this pass runs on any claim set.
//! - phase 3: a per-file triples cache (`cache` module — sqlite
//!   at `.mimus/triples.db`, keyed on content hash + extractor
//!   version) skips re-extracting unchanged files, and the prose
//!   pass's `in_scope` gate lets `--diff` keep the buckets whole
//!   while only reporting conflicts that touch changed files.
//! - `undocumented-fact` (warn, phase 4): reverse coverage — a
//!   *public* module (one the index records as a dependency target)
//!   that no claim names is code the docs never assert. Module
//!   granularity, not edge: every indexed import is a fact nobody
//!   documents, while a module is the smallest unit a doc page can
//!   reasonably cover. `in_scope` gates it the way it gates the
//!   buckets — under `--diff` only modules whose own sources changed
//!   report.

use mimus_check::GraphChecker;
use mimus_claims::triples::{
    canon_subject, contradictions_with_aliases, extract_triples, Contradiction, ContradictionKind,
};
use mimus_core::{Claim, ClaimKind, Edge, EdgeKind, Graph, Severity, Violation};
use rusqlite::{params, Connection, OpenFlags};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub mod cache;
pub use cache::{extract_triples_cached, triples_cache_rows, TriplesCache, TRIPLES_VERSION};

/// Load the kuda index into a `Graph` snapshot — read-only, never
/// creates the file. Missing tables yield empty sets: a
/// symbols-only index still produces a usable graph.
pub fn load_graph(index: &Path) -> Result<Graph, String> {
    let conn = Connection::open_with_flags(index, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let mut g = Graph {
        index: index.to_path_buf(),
        ..Graph::default()
    };
    if table_exists(&conn, "edges")? {
        g.edges = edges(&conn)?;
    }
    if table_exists(&conn, "symbols")? {
        let (modules, symbols) = symbols(&conn)?;
        g.modules = modules;
        g.symbols = symbols;
        g.module_files = module_files(&conn)?;
    }
    Ok(g)
}

/// Age of the index — `meta.generated_at` (unix secs, written by
/// `kuda sync`) when present, else file mtime. `None` when the
/// file can't be read at all; `mimus doctor` reports it.
pub fn index_age(index: &Path) -> Option<Duration> {
    meta_generated_at(index)
        .or_else(|| mtime(index))
        .and_then(|ts| {
            std::time::UNIX_EPOCH
                .checked_add(Duration::from_secs(ts))
                .and_then(|t| t.elapsed().ok())
        })
}

fn meta_generated_at(index: &Path) -> Option<u64> {
    let conn = Connection::open_with_flags(index, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    conn.query_row("SELECT v FROM meta WHERE k = 'generated_at'", [], |r| {
        r.get::<_, String>(0)
    })
    .ok()
    .and_then(|v| v.parse().ok())
}

fn mtime(index: &Path) -> Option<u64> {
    std::fs::metadata(index)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        params![name],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n > 0)
    .map_err(|e| e.to_string())
}

fn ek_from(s: &str) -> EdgeKind {
    match s {
        "imports" => EdgeKind::Imports,
        "package-dep" => EdgeKind::PackageDep,
        "workspace-member" => EdgeKind::WorkspaceMember,
        "calls" => EdgeKind::Calls,
        _ => EdgeKind::Other,
    }
}

fn kind_str(k: EdgeKind) -> &'static str {
    match k {
        EdgeKind::Imports => "imports",
        EdgeKind::PackageDep => "package-dep",
        EdgeKind::WorkspaceMember => "workspace-member",
        EdgeKind::Calls => "calls",
        EdgeKind::Other => "other",
    }
}

#[allow(unexpected_cfgs)] // `cfg(coverage)` like mimus-check-symbol
fn edges(conn: &Connection) -> Result<Vec<Edge>, String> {
    let mut st = conn
        .prepare("SELECT from_mod,to_mod,kind FROM edges")
        .map_err(|e| e.to_string())?;
    let rows = st.query_map([], |r| {
        Ok(Edge {
            from: r.get(0)?,
            to: r.get(1)?,
            kind: ek_from(&r.get::<_, String>(2)?),
        })
    });
    // `?` on query_map is unreachable — [] can't fail to bind
    // after a successful prepare; under coverage the Err arm
    // would be a permanently red line
    #[cfg(coverage)]
    let rows = rows.unwrap();
    #[cfg(not(coverage))]
    let rows = rows.map_err(|e| e.to_string())?;
    // row-level decode errors (NULL cols on hand-built indexes)
    // are skipped, not fatal — same as SymbolChecker's flatten()
    Ok(rows.flatten().collect())
}

/// `(modules, symbols)` — one pass over the symbols table; the
/// module set matches kuda's `DISTINCT module FROM symbols`.
#[allow(unexpected_cfgs)] // `cfg(coverage)` like mimus-check-symbol
fn symbols(conn: &Connection) -> Result<(HashSet<String>, HashSet<String>), String> {
    let mut st = conn
        .prepare("SELECT name,module FROM symbols")
        .map_err(|e| e.to_string())?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)));
    // same unreachable `?` as edges() — see above
    #[cfg(coverage)]
    let rows = rows.unwrap();
    #[cfg(not(coverage))]
    let rows = rows.map_err(|e| e.to_string())?;
    let (mut modules, mut syms) = (HashSet::new(), HashSet::new());
    for (name, module) in rows.flatten() {
        syms.insert(name);
        modules.insert(module);
    }
    Ok((modules, syms))
}

/// `module → files` — the phase-4 provenance. Optional by design:
/// a minimal symbols table (`name`,`module` only) leaves it empty,
/// and the graph pass then treats every module as in scope.
#[allow(unexpected_cfgs)] // `cfg(coverage)` like mimus-check-symbol
fn module_files(conn: &Connection) -> Result<HashMap<String, HashSet<PathBuf>>, String> {
    if !column_exists(conn, "symbols", "file")? {
        return Ok(HashMap::new());
    }
    let mut st = conn
        .prepare("SELECT module,file FROM symbols")
        .map_err(|e| e.to_string())?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)));
    // same unreachable `?` as edges() — see above
    #[cfg(coverage)]
    let rows = rows.unwrap();
    #[cfg(not(coverage))]
    let rows = rows.map_err(|e| e.to_string())?;
    let mut files: HashMap<String, HashSet<PathBuf>> = HashMap::new();
    for (module, file) in rows.flatten() {
        files.entry(module).or_default().insert(PathBuf::from(file));
    }
    Ok(files)
}

/// Does `table` carry `col`? PRAGMA-driven so a minimal index
/// schema degrades instead of erroring.
fn column_exists(conn: &Connection, table: &str, col: &str) -> Result<bool, String> {
    let mut st = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|e| e.to_string())?;
    let rows = st
        .query_map([], |r| r.get::<_, String>(1))
        .map_err(|e| e.to_string())?;
    for name in rows.flatten() {
        if name == col {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Set-level graph pass — phase-1 classes only. Stateless; all
/// evidence comes from the `Graph` snapshot.
pub struct IndexEdgeChecker;

impl GraphChecker for IndexEdgeChecker {
    fn run_set(&self, claims: &[Claim], graph: &Graph) -> Vec<Violation> {
        let mut out = Vec::new();
        stale_references(graph, &mut out);
        unsupported_claims(claims, graph, &mut out);
        out
    }
}

/// Prose contradiction pass — ADR-0002 phase 2. The bucket
/// classes `contradiction` (cross-source, doc↔doc) and
/// `self-contradiction` (reflexive: one file asserting ≥2 objects
/// for the same slot). Index-free — prose triples don't need the
/// kuda index, so this runs on every claim set, not just when
/// `[check].kuda_index` is configured. Warn tier: extraction is
/// heuristic, so these report but never gate.
pub struct ProseContradictionChecker {
    /// Subject aliases from `[check].aliases` — `(phrase,
    /// canonical)` pairs resolved at bucket-key time.
    pub aliases: Vec<(String, String)>,
    /// Per-file triples cache (phase 3) — `Some` serves unchanged
    /// files' triples from `.mimus/triples.db`; `None` extracts
    /// every run.
    pub cache: Option<TriplesCache>,
}

impl ProseContradictionChecker {
    /// claims → triples → buckets → warn-tier violations.
    /// `in_scope` gates *findings*, not extraction: the buckets
    /// see every file's triples (a contradiction needs both
    /// sides) but a conflict emits only when ≥1 object site is
    /// in scope — under `--diff` that's the changed-file side,
    /// so unchanged↔unchanged drift stays silent.
    pub fn run_prose(
        &mut self,
        claims: &[Claim],
        in_scope: &dyn Fn(&Path) -> bool,
    ) -> Vec<Violation> {
        let triples = match self.cache.as_mut() {
            Some(c) => extract_triples_cached(claims, c),
            None => extract_triples(claims),
        };
        contradictions_with_aliases(&triples, &self.aliases)
            .iter()
            .filter(|c| c.objects.iter().any(|(_, src, _)| in_scope(src)))
            .map(|c| contradiction_violation(c, claims, in_scope))
            .collect()
    }
}

/// `undocumented-fact` — ADR-0002 phase 4: reverse coverage
/// (code→doc), warn tier. The *public surface* is the set of
/// modules the index records as a dependency target — internal
/// details nobody imports aren't contract. A public module emits
/// when no document in the mined set names it, matched the way the
/// buckets match subjects (`canon_subject`: case, `-`/`_`, plurals
/// and `[check].aliases` all fold), so `mimus-core`, `mimus_core`
/// and an aliased spelling each count as documenting it. The
/// haystack is document text, not extracted claims: a doc that
/// merely lists a crate (`[mimus-check-acp](…)`) documents it
/// without asserting a checkable claim — claim-level matching would
/// flag every crate index page.
pub struct UndocumentedFactChecker<'a> {
    /// Subject aliases from `[check].aliases` — `(phrase,
    /// canonical)` pairs, the same map the contradiction buckets
    /// resolve.
    pub aliases: &'a [(String, String)],
}

impl UndocumentedFactChecker<'_> {
    /// `in_scope` gates *findings*, not the scan: under `--diff`
    /// only a module whose own indexed sources changed reports;
    /// unscoped runs pass `|_| true`. The module set itself always
    /// comes from the whole index — a snapshot has no per-run
    /// scope to narrow. `docs` is the concatenated text of every
    /// document the claim set was mined from (cards included).
    pub fn run_facts(
        &self,
        docs: &str,
        graph: &Graph,
        in_scope: &dyn Fn(&Path) -> bool,
    ) -> Vec<Violation> {
        // padded canonical doc text — the haystack, so `mimus-core`
        // can't match inside `mimus-core-extra`
        let hay = format!(" {} ", canon_subject(docs, self.aliases));
        public_modules(graph)
            .into_iter()
            .filter(|m| !documented(m, &hay, self.aliases))
            .filter(|m| module_in_scope(graph, m, in_scope))
            .map(|m| fact_violation(graph, m))
            .collect()
    }
}

/// Modules the index records as a dependency *target* — the public
/// surface other code actually consumes. `calls` edges prove nothing
/// about the API, an unresolved target is external, and a self-edge
/// (`a → a`) marks nothing.
fn public_modules(graph: &Graph) -> BTreeSet<&str> {
    graph
        .edges
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                EdgeKind::Imports | EdgeKind::PackageDep | EdgeKind::WorkspaceMember
            )
        })
        .filter_map(|e| {
            let target = resolve(&e.to, &graph.modules)?;
            (module_key(&e.from) != module_key(target)).then_some(target)
        })
        .collect()
}

/// Does any document name `module`? Canon-matched against the padded
/// haystack, plus every `[check].aliases` spelling whose canonical
/// target folds onto the module.
fn documented(module: &str, hay: &str, aliases: &[(String, String)]) -> bool {
    let canon = canon_subject(module, aliases);
    if canon.is_empty() {
        // a name made only of stopwords has no needle to look for —
        // silence beats a substring match on padding
        return true;
    }
    let spellings = std::iter::once(canon.clone()).chain(
        aliases
            .iter()
            .filter(|(_, target)| canon_subject(target, aliases) == canon)
            .map(|(alias, _)| canon_subject(alias, &[])),
    );
    spellings
        .filter(|n| !n.is_empty())
        .any(|n| hay.contains(&format!(" {n} ")))
}

/// `--diff` gate: does any of the module's own indexed sources sit
/// in scope? A snapshot without file provenance (a hand-built
/// `Graph`) can't answer — it reports rather than silently drop.
fn module_in_scope(graph: &Graph, module: &str, in_scope: &dyn Fn(&Path) -> bool) -> bool {
    match graph.module_files.get(module) {
        Some(files) => files.iter().any(|f| in_scope(f)),
        None => true,
    }
}

/// One undocumented public module → one warn. `line: 0` — the
/// finding is module-level, anchored on a real source file so the
/// report points at code rather than the index.
fn fact_violation(graph: &Graph, module: &str) -> Violation {
    let files = graph.module_files.get(module);
    let source = files
        .and_then(|f| f.iter().min())
        .cloned()
        .unwrap_or_else(|| graph.index.clone());
    let n = files.map_or(0, HashSet::len);
    Violation {
        claim: Claim {
            kind: ClaimKind::Symbol,
            text: module.to_owned(),
            source,
            line: 0,
        },
        evidence: format!(
            "public module `{module}` ({n} indexed file(s)) is named by no claim — code asserts it, the docs never do"
        ),
        fix: Some("document the module's role — or drop it from the public surface".into()),
        severity: Severity::Warn,
    }
}

/// One contradiction → one warn violation: the class name, the
/// bucket key it matched on, and every object with its
/// `file:line` site is the evidence. The anchor claim prefers an
/// in-scope site — under `--diff` that's the *new* side of the
/// disagreement, so output points at the drifted file.
fn contradiction_violation(
    c: &Contradiction,
    claims: &[Claim],
    in_scope: &dyn Fn(&Path) -> bool,
) -> Violation {
    let class = match c.kind {
        ContradictionKind::CrossSource => "contradiction",
        ContradictionKind::Reflexive => "self-contradiction",
    };
    let sites = c
        .objects
        .iter()
        .map(|(o, src, line)| format!("`{o}` ({}:{line})", src.display()))
        .collect::<Vec<_>>()
        .join(" vs ");
    let anchor = c
        .objects
        .iter()
        .position(|(_, src, _)| in_scope(src))
        .unwrap_or(0);
    let (obj, src, line) = &c.objects[anchor];
    // a triple's (source, line) always came from a claim in this
    // set — the position() fallback is unreachable, just total
    let i = claims
        .iter()
        .position(|k| k.source == *src && k.line as usize == *line && k.text.contains(obj.as_str()))
        .unwrap_or_default();
    Violation {
        claim: claims[i].clone(),
        evidence: format!(
            "{class} — bucket `{}` asserts conflicting objects: {sites}",
            c.bucket
        ),
        fix: Some("reconcile the claims — pick the true object for this slot".into()),
        severity: Severity::Warn,
    }
}

/// `stale-reference` — error tier: the edge exists but its target
/// (or its source module) doesn't. Mechanical-grade evidence.
fn stale_references(g: &Graph, out: &mut Vec<Violation>) {
    let keys: HashSet<String> = g.modules.iter().map(|m| module_key(m)).collect();
    let mut seen = HashSet::new();
    for e in &g.edges {
        // an edge recorded *from* a module the index no longer
        // holds — orphaned rows a partial sync left behind
        if !keys.contains(&module_key(&e.from)) {
            emit_stale(
                g,
                &mut seen,
                out,
                e,
                format!(
                    "edge recorded from `{from}`, which is absent from the index — stale rows",
                    from = e.from
                ),
            );
            continue;
        }
        if !matches!(
            e.kind,
            EdgeKind::Imports | EdgeKind::PackageDep | EdgeKind::WorkspaceMember
        ) {
            continue; // calls/unknown kinds assert no module dep
        }
        if keys.contains(&module_key(&e.to)) {
            continue; // resolved target — healthy edge
        }
        // `workspace-member` targets are internal by definition;
        // the rest flag only when internal-shaped — else the
        // missing target is just an external dep
        if e.kind == EdgeKind::WorkspaceMember || internal_shaped(&e.to, &g.modules) {
            emit_stale(
                g,
                &mut seen,
                out,
                e,
                format!(
                    "`{from}` {kind} `{to}`, but `{to}` is absent from the index — removed, renamed, or a stale index",
                    from = e.from,
                    kind = kind_str(e.kind),
                    to = e.to
                ),
            );
        }
    }
}

fn emit_stale(
    g: &Graph,
    seen: &mut HashSet<(String, String)>,
    out: &mut Vec<Violation>,
    e: &Edge,
    evidence: String,
) {
    if !seen.insert((e.from.clone(), e.to.clone())) {
        return; // one finding per from→to pair
    }
    out.push(Violation {
        claim: Claim {
            kind: ClaimKind::Symbol,
            text: format!(
                "{from} -[{kind}]-> {to}",
                from = e.from,
                kind = kind_str(e.kind),
                to = e.to
            ),
            source: g.index.clone(),
            line: 0, // index-backed, not file-backed
        },
        evidence,
        fix: Some("run `kuda sync` — or remove the dead reference".into()),
        severity: Severity::Error,
    });
}

/// `unsupported-claim` — warn tier: prose asserts an import/dep
/// edge the index lacks. Both sides must resolve to indexed
/// modules — anything fuzzier is unproveable, silence.
fn unsupported_claims(claims: &[Claim], g: &Graph, out: &mut Vec<Violation>) {
    for c in claims {
        let mut seen = HashSet::new();
        for (x, y) in edge_assertions(&c.text) {
            if !seen.insert((x.to_lowercase(), y.to_lowercase())) {
                continue;
            }
            if module_key(x) == module_key(y) {
                continue; // self-edge claims answer nothing
            }
            let Some((a, b)) = resolve(x, &g.modules).zip(resolve(y, &g.modules)) else {
                continue; // can't prove both sides internal — silence
            };
            if edge_exists(g, a, b) {
                continue;
            }
            out.push(Violation {
                claim: c.clone(),
                evidence: format!(
                    "asserts `{a}` depends on `{b}`, but the index records no imports/package-dep edge — doc drifted or the edge was removed"
                ),
                fix: Some("update the doc — or run `kuda sync` if the edge should exist".into()),
                severity: Severity::Warn,
            });
        }
    }
}

/// `X <verb> Y` pairs in prose — `imports`, `depends on`,
/// `requires`, `uses`. Token-level, no regex: module names are
/// words of ident/import-specifier characters.
fn edge_assertions(text: &str) -> Vec<(&str, &str)> {
    let words: Vec<&str> = text
        .split(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '@' | '/')))
        .filter(|w| !w.is_empty())
        .collect();
    let mut out = Vec::new();
    for (i, w) in words.iter().enumerate() {
        // the verb's object is one token ahead — two for the
        // phrasal `depends on`
        let gap = match w.to_lowercase().as_str() {
            "imports" | "requires" | "uses" => 1,
            "depends"
                if words
                    .get(i + 1)
                    .is_some_and(|n| n.eq_ignore_ascii_case("on")) =>
            {
                2
            }
            _ => continue,
        };
        let (Some(x), Some(y)) = (i.checked_sub(1).map(|j| words[j]), words.get(i + gap)) else {
            continue;
        };
        // sentence-ending periods ride along — '.' is a legit
        // interior char (go paths) but never a boundary one
        out.push((x.trim_matches('.'), y.trim_matches('.')));
    }
    out
}

/// Does the graph record a dep-ish edge `a → b`? Compared on
/// kuda's folded `module_key`, not raw spelling.
fn edge_exists(g: &Graph, a: &str, b: &str) -> bool {
    let (ka, kb) = (module_key(a), module_key(b));
    g.edges.iter().any(|e| {
        matches!(
            e.kind,
            EdgeKind::Imports | EdgeKind::PackageDep | EdgeKind::WorkspaceMember
        ) && module_key(&e.from) == ka
            && module_key(&e.to) == kb
    })
}

/// Claim ident → indexed module name: folded match first, then
/// subpath (`pkg/sub` claims resolve to `pkg`) — mirrors kuda's
/// `resolve_module`. Longest match wins, ties break
/// lexicographically — deterministic regardless of set order.
fn resolve<'a>(name: &str, modules: &'a HashSet<String>) -> Option<&'a str> {
    let want = module_key(name);
    modules
        .iter()
        .filter(|m| {
            let k = module_key(m);
            k == want || want.starts_with(&format!("{k}/"))
        })
        .max_by_key(|m| (m.len(), m.as_str()))
        .map(String::as_str)
}

/// kuda's `module_key` — dash/underscore/case fold, the same
/// normalization the indexer applies to edge targets.
fn module_key(name: &str) -> String {
    name.replace('-', "_").to_lowercase()
}

/// First '-'/'_'/'.'-separated segment — the naming-family prefix.
fn seg0(name: &str) -> &str {
    // split yields ≥1 item on any input — unwrap_or avoids unwrap
    name.split(['-', '_', '.']).next().unwrap_or(name)
}

/// Is `to` shaped like a workspace member of this repo? A missing
/// target flags only when it shares the monorepo's own naming
/// family — a '/'-scope or a ≥3-char flat prefix carried by ≥2
/// indexed modules (a convention, not a coincidence). Anything
/// else is an external dep, not a stale reference.
fn internal_shaped(to: &str, modules: &HashSet<String>) -> bool {
    if to.contains('/') {
        let Some(sc) = scope(to) else {
            return false; // two-segment names carry no reliable scope
        };
        return modules
            .iter()
            .filter(|m| scope(m).is_some_and(|s| s.eq_ignore_ascii_case(sc)))
            .count()
            >= 2;
    }
    let lower = to.to_lowercase();
    let first = seg0(&lower);
    first.len() >= 3
        && modules
            .iter()
            .filter(|m| seg0(&m.to_lowercase()) == first)
            .count()
            >= 2
}

/// The '/'-scope a name belongs to: `@org` for `@org/pkg`;
/// `host/org` for go-style `github.com/org/repo` — so an external
/// `github.com/ext/x` can't collide with the monorepo's own org.
/// `None` for flat names and two-segment paths.
fn scope(name: &str) -> Option<&str> {
    let (s1, rest) = name.split_once('/')?;
    if s1.starts_with('@') {
        Some(s1)
    } else {
        rest.split_once('/')
            .map(|(s2, _)| &name[..s1.len() + 1 + s2.len()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The naming-family rules that keep external deps silent —
    /// the precision core of `stale-reference`.
    #[test]
    fn internal_shaped_rules() {
        let mods: HashSet<String> = ["@org/a", "@org/b", "mimus-core", "mimus-cli", "solo-mod"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        // shared '@org' scope (≥2 modules) → internal
        assert!(internal_shaped("@org/gone", &mods));
        // shared 'mimus-' flat prefix (≥2, ≥3 chars) → internal
        assert!(internal_shaped("mimus-gone", &mods));
        assert!(internal_shaped("mimus_gone", &mods));
        // 'solo' prefix carried by one module only → coincidence,
        // not a convention → silence
        assert!(!internal_shaped("solo-thing", &mods));
        // bare external names → silence
        assert!(!internal_shaped("serde", &mods));
        assert!(!internal_shaped("react", &mods));
        // short prefixes don't count as a family
        assert!(!internal_shaped("mi-x", &mods));
        // a different scope → silence
        assert!(!internal_shaped("@types/node", &mods));
        // two-segment non-scoped paths → silence
        assert!(!internal_shaped("github.com/foo", &mods));
        let go: HashSet<String> = ["github.com/org/a", "github.com/org/b"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        // same host+org as ≥2 modules → internal; another org → not
        assert!(internal_shaped("github.com/org/gone", &go));
        assert!(!internal_shaped("github.com/ext/x", &go));
        // module names themselves exercise scope()/seg0() arms
        assert!(internal_shaped("@ORG/Case", &mods)); // case-insensitive scope
    }

    #[test]
    fn edge_assertion_phrasings() {
        let v = edge_assertions(
            "mimus-cli imports mimus-core. The web depends on api-v2 and app requires db. x uses y.",
        );
        assert_eq!(
            v,
            [
                ("mimus-cli", "mimus-core"),
                ("web", "api-v2"),
                ("app", "db"),
                ("x", "y")
            ]
        );
        // verb-first sentence — no subject token before the verb
        assert!(edge_assertions("imports nothing-here").is_empty());
        // 'depends' without 'on' is not an edge claim
        assert!(edge_assertions("it depends largely").is_empty());
        assert!(edge_assertions("plain prose, no verbs").is_empty());
    }

    #[test]
    fn resolve_folded_and_subpath() {
        let mods: HashSet<String> = ["mimus-core", "pkg", "a-b"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        // dash/underscore/case fold
        assert_eq!(resolve("mimus_core", &mods), Some("mimus-core"));
        assert_eq!(resolve("MIMUS-CORE", &mods), Some("mimus-core"));
        // subpath resolves to the module root
        assert_eq!(resolve("pkg/sub", &mods), Some("pkg"));
        // unknown → None
        assert_eq!(resolve("nowhere", &mods), None);
        // longest match wins over a shorter prefix
        let both: HashSet<String> = ["x", "x/y"].iter().map(|s| s.to_string()).collect();
        assert_eq!(resolve("x/y/z", &both), Some("x/y"));
        // kind_str / ek_from cover every variant
        assert_eq!(kind_str(ek_from("imports")), "imports");
        assert_eq!(kind_str(ek_from("package-dep")), "package-dep");
        assert_eq!(kind_str(ek_from("workspace-member")), "workspace-member");
        assert_eq!(kind_str(ek_from("calls")), "calls");
        assert_eq!(kind_str(ek_from("bogus")), "other");
    }

    #[test]
    fn stale_reference_classes() {
        let mk = |edges: Vec<Edge>| Graph {
            modules: ["a", "b", "mim-x", "mim-y", "@s/one", "@s/two"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            edges,
            index: PathBuf::from("idx.sqlite"),
            ..Graph::default()
        };
        let edge = |f: &str, t: &str, k: EdgeKind| Edge {
            from: f.into(),
            to: t.into(),
            kind: k,
        };
        // healthy: resolved target — no finding
        let mut out = Vec::new();
        stale_references(&mk(vec![edge("a", "b", EdgeKind::Imports)]), &mut out);
        assert!(out.is_empty());
        // internal-shaped missing targets — flat prefix + scope
        let mut out = Vec::new();
        stale_references(
            &mk(vec![
                edge("a", "mim-gone", EdgeKind::Imports),
                edge("a", "@s/gone", EdgeKind::PackageDep),
                edge("a", "@s/gone", EdgeKind::PackageDep), // dup → deduped
            ]),
            &mut out,
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].severity, Severity::Error);
        assert_eq!(out[0].claim.source, PathBuf::from("idx.sqlite"));
        assert!(out[0].evidence.contains("mim-gone"), "{}", out[0].evidence);
        assert!(out[0].claim.text.contains("a -[imports]-> mim-gone"));
        // external-shaped missing target → silence
        let mut out = Vec::new();
        stale_references(
            &mk(vec![
                edge("a", "serde", EdgeKind::PackageDep),
                edge("a", "react", EdgeKind::Imports),
            ]),
            &mut out,
        );
        assert!(out.is_empty());
        // calls edges assert no module dep — skipped even when the
        // target looks internal
        let mut out = Vec::new();
        stale_references(&mk(vec![edge("a", "mim-gone", EdgeKind::Calls)]), &mut out);
        assert!(out.is_empty());
        // workspace-member target is internal by definition — a
        // bare name flags with no naming-family requirement
        let mut out = Vec::new();
        stale_references(
            &mk(vec![edge("a", "anything", EdgeKind::WorkspaceMember)]),
            &mut out,
        );
        assert_eq!(out.len(), 1);
        // orphaned source module — flagged regardless of kind
        let mut out = Vec::new();
        stale_references(
            &mk(vec![
                edge("ghost", "b", EdgeKind::Imports),
                edge("ghost2", "b", EdgeKind::Other),
            ]),
            &mut out,
        );
        assert_eq!(out.len(), 2);
        assert!(out[0].evidence.contains("ghost"));
        assert!(out[1].claim.text.contains("-[other]->"));
    }

    #[test]
    fn unsupported_claim_class() {
        let g = Graph {
            modules: ["a", "b", "c"].iter().map(|s| s.to_string()).collect(),
            edges: vec![Edge {
                from: "a".into(),
                to: "b".into(),
                kind: EdgeKind::Imports,
            }],
            ..Graph::default()
        };
        let claim = |text: &str| Claim {
            kind: ClaimKind::Behavior,
            text: text.into(),
            source: PathBuf::from("d.md"),
            line: 3,
        };
        // asserted edge exists → silence
        let mut out = Vec::new();
        unsupported_claims(&[claim("doc says a imports b plainly")], &g, &mut out);
        assert!(out.is_empty());
        // asserted edge missing → warn, claim preserved
        let mut out = Vec::new();
        unsupported_claims(
            &[
                claim("doc says b imports a and b requires c"),
                // same pair twice in one claim → deduped per-claim
                claim("doc says b depends on a and b depends on a"),
            ],
            &g,
            &mut out,
        );
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|v| v.severity == Severity::Warn));
        assert!(out[0].evidence.contains("b"), "{}", out[0].evidence);
        // a side that doesn't resolve → unproveable → silence
        let mut out = Vec::new();
        unsupported_claims(&[claim("docs say zzz imports a")], &g, &mut out);
        assert!(out.is_empty());
        // self-edge claim → silence
        let mut out = Vec::new();
        unsupported_claims(&[claim("a-a imports a_a")], &g, &mut out);
        assert!(out.is_empty());
        // calls edges don't satisfy an import claim
        let gc = Graph {
            modules: g.modules.clone(),
            edges: vec![Edge {
                from: "a".into(),
                to: "b".into(),
                kind: EdgeKind::Calls,
            }],
            ..Graph::default()
        };
        let mut out = Vec::new();
        unsupported_claims(&[claim("a uses b")], &gc, &mut out);
        assert_eq!(out.len(), 1);
    }

    /// run_set composes both classes — one stale error + one
    /// unsupported warn from the same snapshot.
    #[test]
    fn run_set_composes() {
        let g = Graph {
            modules: ["app-x", "app-y"].iter().map(|s| s.to_string()).collect(),
            edges: vec![Edge {
                from: "app-x".into(),
                to: "app-gone".into(),
                kind: EdgeKind::Imports,
            }],
            ..Graph::default()
        };
        let claims = [Claim {
            kind: ClaimKind::Behavior,
            text: "app-y uses app-x".into(),
            source: PathBuf::from("d.md"),
            line: 1,
        }];
        let v = IndexEdgeChecker.run_set(&claims, &g);
        assert_eq!(v.len(), 2);
        assert!(v.iter().any(|v| v.severity == Severity::Error));
        assert!(v.iter().any(|v| v.severity == Severity::Warn));
    }

    /// run_prose composes extraction → bucketing → violation: the
    /// reflexive (same-doc) and cross-source classes, warn tier,
    /// evidence naming the bucket + both sites.
    #[test]
    fn prose_contradiction_classes() {
        let bclaim = |text: &str, src: &str, line: u32| Claim {
            kind: ClaimKind::Behavior,
            text: text.into(),
            source: PathBuf::from(src),
            line,
        };
        let mut chk = ProseContradictionChecker {
            aliases: Vec::new(),
            cache: None,
        };
        // no scope gate — everything emits, the pre-ph3 behavior
        let all: &dyn Fn(&Path) -> bool = &|_| true;
        // empty in → silence
        assert!(chk.run_prose(&[], all).is_empty());
        // agreeing claims → silence
        let claims = [
            bclaim("auth uses jwt", "a.md", 1),
            bclaim("the auth service uses jwt", "b.md", 2),
        ];
        assert!(chk.run_prose(&claims, all).is_empty());
        // same doc, two objects → reflexive warn with both sites
        let claims = [
            bclaim("auth uses jwt", "a.md", 1),
            bclaim("auth uses sessions", "a.md", 5),
        ];
        let v = chk.run_prose(&claims, all);
        assert_eq!(v.len(), 1, "{v:?}");
        assert_eq!(v[0].severity, Severity::Warn);
        // anchored on the first-seen site — the real claim
        assert_eq!(v[0].claim.text, "auth uses jwt");
        assert_eq!(v[0].claim.line, 1);
        let e = &v[0].evidence;
        assert!(e.contains("self-contradiction"), "{e}");
        assert!(v[0].evidence.contains("auth|uses"), "{}", v[0].evidence);
        assert!(v[0].evidence.contains("a.md:1"), "{}", v[0].evidence);
        assert!(v[0].evidence.contains("a.md:5"), "{}", v[0].evidence);
        assert!(v[0].fix.is_some());
        // different files → cross-source
        let claims = [
            bclaim("auth uses jwt", "a.md", 1),
            bclaim("auth uses sessions", "b.md", 2),
        ];
        let v = chk.run_prose(&claims, all);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].evidence.contains("contradiction —"));
        assert!(!v[0].evidence.contains("self-contradiction"));
        // three-way disagreement → one violation, all three sites
        let claims = [
            bclaim("auth uses jwt", "a.md", 1),
            bclaim("auth uses sessions", "a.md", 2),
            bclaim("auth uses tokens", "b.md", 3),
        ];
        let v = chk.run_prose(&claims, all);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].evidence.contains("tokens"), "{}", v[0].evidence);
        // aliases fold differently-spelled subjects into one bucket
        let claims = [
            bclaim("login flow uses jwt", "a.md", 1),
            bclaim("the auth service uses sessions", "b.md", 2),
        ];
        assert!(
            chk.run_prose(&claims, all).is_empty(),
            "no aliases → silence"
        );
        let mut chk = ProseContradictionChecker {
            aliases: vec![("login flow".to_owned(), "auth".to_owned())],
            cache: None,
        };
        let v = chk.run_prose(&claims, all);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].evidence.contains("auth|uses"), "{}", v[0].evidence);
    }

    /// ADR-0002 ph3 — `in_scope` gates *findings*, not the
    /// buckets: a conflict touching a scoped file emits and
    /// anchors on the in-scope side; one entirely outside stays
    /// silent. The cache path returns identical findings — once
    /// extracting, once reading stored rows.
    #[test]
    fn prose_scope_gate_and_cache() {
        let dir = std::env::temp_dir().join(format!("mimus-prose-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b, c, d) = (
            dir.join("a.md"),
            dir.join("b.md"),
            dir.join("c.md"),
            dir.join("d.md"),
        );
        std::fs::write(&a, "auth uses sessions\n").unwrap();
        std::fs::write(&b, "auth uses jwt\n").unwrap();
        std::fs::write(&c, "pool uses postgres\n").unwrap();
        std::fs::write(&d, "pool uses redis\n").unwrap();
        let mk = |text: &str, src: &Path, line: u32| Claim {
            kind: ClaimKind::Behavior,
            text: text.into(),
            source: src.to_path_buf(),
            line,
        };
        // b.md first-seen so the scope-preferred anchor is
        // observable: in-scope a.md is the *second* object site
        let claims = [
            mk("auth uses jwt", &b, 1),
            mk("auth uses sessions", &a, 1),
            mk("pool uses postgres", &c, 1),
            mk("pool uses redis", &d, 1),
        ];
        let only_a: &dyn Fn(&Path) -> bool = &|p| p == a;
        // uncached path: a↔b emits (anchored on in-scope a.md,
        // though b.md's site is first-seen), c↔d stays silent
        let mut chk = ProseContradictionChecker {
            aliases: Vec::new(),
            cache: None,
        };
        let v = chk.run_prose(&claims, only_a);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].evidence.contains("auth|uses"), "{}", v[0].evidence);
        assert!(v[0].evidence.contains("b.md:1"), "{}", v[0].evidence);
        assert_eq!(v[0].claim.source, a, "anchor prefers the in-scope site");
        assert_eq!(v[0].claim.text, "auth uses sessions");
        // cached path — same findings across an extract run and
        // a stored-rows run
        let mut chk = ProseContradictionChecker {
            aliases: Vec::new(),
            cache: Some(TriplesCache::open(&dir.join("triples.db")).unwrap()),
        };
        for _ in 0..2 {
            let v = chk.run_prose(&claims, only_a);
            assert_eq!(v.len(), 1, "{v:?}");
            assert_eq!(v[0].claim.source, a);
        }
        // nothing in scope → silence; everything → both conflicts
        assert!(chk.run_prose(&claims, &|_| false).is_empty());
        let v = chk.run_prose(&claims, &|_| true);
        assert_eq!(v.len(), 2, "{v:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ADR-0002 ph4 — the public surface is what the index records
    /// as a dependency target: `imports`/`package-dep`/
    /// `workspace-member` from a *different* module. `calls`,
    /// unresolved externals and self-edges mark nothing.
    #[test]
    fn public_module_surface() {
        let mk = |from: &str, to: &str, kind: EdgeKind| Edge {
            from: from.into(),
            to: to.into(),
            kind,
        };
        let g = Graph {
            modules: ["app-cli", "app-core", "app-internal"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            edges: vec![
                mk("app-cli", "APP_CORE", EdgeKind::Imports), // folded target
                mk("app-cli", "serde", EdgeKind::PackageDep), // external
                mk("app-core", "app-core", EdgeKind::Imports), // self-edge
                mk("app-core", "app-internal", EdgeKind::Calls), // not a dep
                mk("app-core", "mim-ghost", EdgeKind::Imports), // unresolvable
            ],
            ..Graph::default()
        };
        assert_eq!(
            public_modules(&g).into_iter().collect::<Vec<_>>(),
            ["app-core"]
        );
    }

    /// ADR-0002 ph4 — a public module no document names warns; any
    /// canon or alias spelling documents it. Provenance gates the
    /// finding under `--diff`; a snapshot without it reports rather
    /// than drops.
    #[test]
    fn undocumented_fact_class() {
        let g = Graph {
            modules: ["app-cli", "app-core"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            edges: vec![Edge {
                from: "app-cli".into(),
                to: "app-core".into(),
                kind: EdgeKind::Imports,
            }],
            index: PathBuf::from("idx.sqlite"),
            ..Graph::default()
        };
        let facts = UndocumentedFactChecker { aliases: &[] };
        let all: &dyn Fn(&Path) -> bool = &|_| true;
        // any canon spelling (dash, underscore, case) documents it
        assert!(facts
            .run_facts("app-core owns the domain", &g, all)
            .is_empty());
        assert!(facts
            .run_facts("the APP_CORE crate owns it", &g, all)
            .is_empty());
        // a name of stopwords only has no needle — silence
        assert!(documented("the", " app core ", &[]));
        // no document names it → one warn, anchored on the index
        let v = facts.run_facts("app-cli parses flags", &g, all);
        assert_eq!(v.len(), 1, "{v:?}");
        let e = &v[0].evidence;
        assert!(e.contains("app-core"), "{e}");
        assert!(e.contains("0 indexed file(s)"), "{e}");
        assert_eq!(v[0].severity, Severity::Warn);
        assert_eq!(v[0].claim.text, "app-core");
        assert_eq!(v[0].claim.line, 0);
        assert_eq!(v[0].claim.source, PathBuf::from("idx.sqlite"));
        assert!(v[0].fix.is_some());
        // an alias spelling documents the module it canonicalizes to
        let aliased = UndocumentedFactChecker {
            aliases: &[("the core crate".to_owned(), "app-core".to_owned())],
        };
        assert!(aliased
            .run_facts("the core crate owns the domain", &g, all)
            .is_empty());
        // a stopword alias has no needle, and unrelated aliases
        // don't widen the match
        let odd = UndocumentedFactChecker {
            aliases: &[
                ("the".to_owned(), "app-core".to_owned()),
                ("login flow".to_owned(), "auth".to_owned()),
            ],
        };
        assert_eq!(odd.run_facts("unrelated docs", &g, all).len(), 1);
        // provenance: `--diff` scope asks the module's own files
        let g = Graph {
            module_files: [(
                "app-core".to_owned(),
                [PathBuf::from("crates/core/src/lib.rs")]
                    .into_iter()
                    .collect(),
            )]
            .into_iter()
            .collect(),
            ..g
        };
        let in_scope: &dyn Fn(&Path) -> bool = &|p| p.ends_with("lib.rs");
        let v = facts.run_facts("app-cli parses flags", &g, in_scope);
        assert_eq!(v.len(), 1, "{v:?}");
        assert_eq!(v[0].claim.source, PathBuf::from("crates/core/src/lib.rs"));
        assert!(v[0].evidence.contains("1 indexed file(s)"), "{e}");
        // module untouched by the diff → silence
        assert!(facts
            .run_facts("app-cli parses flags", &g, &|_| false)
            .is_empty());
    }
}
