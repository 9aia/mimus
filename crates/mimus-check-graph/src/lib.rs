//! Graph adapter — set-level checking against the kuda index
//! (ADR-0002 phase 1). Consumes the index *file format*, not the
//! kuda crates — the schema is the contract, same as
//! `mimus-check-symbol`.
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

use mimus_check::GraphChecker;
use mimus_core::{Claim, ClaimKind, Edge, EdgeKind, Graph, Severity, Violation};
use rusqlite::{params, Connection, OpenFlags};
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

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
}
