//! views — the driver's manifest: every ViewSpec plus the query
//! resolvers that feed them.
//!
//! Read path: [`crate::data::Snapshot`] resolves the repo root +
//! `mimus.toml` once per wire op. `status`/`rules`/`audit` read
//! files in-process (config, `rules/*.toml`, `.kuda` card
//! provenance); `violations` shells out to `mimus report --format
//! json` — the check pipeline is the critic's authority and not
//! something a view driver should fork.
//!
//! Verb → view mapping:
//!
//! | `mimus` surface                  | pavo view                |
//! |--------------------------------|---------------------------|
//! | `mimus report` (violations)      | `mimus:violations` table  |
//! | `mimus audit` (card trust)       | `mimus:audit` table+kv    |
//! | `rules/*.toml` convention rules  | `mimus:rules` table       |
//! | config/index/cache digest        | `mimus:status` kv         |
//!
//! mimus persists no verdicts or
//! history — there is no `mimus:verdicts`/`mimus:history` view to
//! build. `mimus:status` says so.

use pavo_protocol::{
    ColumnNode, Contributes, Contribution, DriverManifest, KvNode, MarkdownNode, NodeBase,
    QueryBinding, TableColumn, TableNode, ViewNode, ViewSpec,
};
use serde_json::{json, Map, Value};

use crate::data::Snapshot;

// ---------------------------------------------------------------- helpers

fn titled(title: &str) -> NodeBase {
    NodeBase {
        title: Some(title.to_string()),
        ..NodeBase::default()
    }
}

fn bound(title: &str, query: &str) -> NodeBase {
    NodeBase {
        title: Some(title.to_string()),
        query: Some(query.to_string()),
        ..NodeBase::default()
    }
}

fn col(title: &str, children: Vec<ViewNode>) -> ViewNode {
    ViewNode::Column(ColumnNode {
        base: titled(title),
        children,
        gap: None,
    })
}

fn kv(title: &str, query: &str) -> ViewNode {
    ViewNode::Kv(KvNode {
        base: bound(title, query),
        pairs: Vec::new(),
    })
}

fn table(title: &str, query: &str, columns: &[(&str, &str)]) -> ViewNode {
    ViewNode::Table(TableNode {
        base: bound(title, query),
        columns: columns
            .iter()
            .map(|(key, t)| TableColumn {
                key: (*key).to_string(),
                title: (*t).to_string(),
                align: None,
                width: None,
            })
            .collect(),
        rows: Vec::new(),
        sortable: true,
    })
}

fn md(title: &str, text: &str) -> ViewNode {
    ViewNode::Markdown(MarkdownNode {
        base: titled(title),
        text: text.to_string(),
    })
}

fn binding(name: &str) -> QueryBinding {
    QueryBinding {
        name: name.to_string(),
        op: "query".to_string(),
        params: Map::new(),
    }
}

fn spec(
    id: &str,
    title: &str,
    route: &str,
    description: &str,
    queries: &[&str],
    root: ViewNode,
) -> ViewSpec {
    ViewSpec {
        id: id.to_string(),
        title: title.to_string(),
        route: route.to_string(),
        description: Some(description.to_string()),
        queries: queries.iter().map(|q| binding(q)).collect(),
        root,
    }
}

// ---------------------------------------------------------------- manifest

/// Every view the driver declares — static (no repo state needed;
/// `manifest` must answer anywhere).
pub fn specs() -> Vec<ViewSpec> {
    vec![
        spec(
            "status",
            "Status",
            "/mimus/status",
            "Repo digest — resolved mimus.toml, checker config, kuda index age, triples cache, rule/card counts. mimus keeps no state: this is what a run *would* check.",
            &["summary"],
            col(
                "Mimus status",
                vec![
                    kv("Repo", "summary"),
                    md(
                        "Note",
                        "mimus is stateless — `mimus.toml` + repo sources are the state; every verb recomputes. `.mimus/triples.db` is a cache, not data. There are no persisted verdicts or history to view.",
                    ),
                ],
            ),
        ),
        spec(
            "violations",
            "Violations",
            "/mimus/violations",
            "`mimus report` — every confirmed violation (claim + evidence + fix) across docs, comments, cards, rules and the kuda graph. Runs the real check pipeline; may take seconds. Filter with --param severity= / kind= / source= / limit=.",
            &["violations"],
            table(
                "Violations",
                "violations",
                &[
                    ("severity", "Severity"),
                    ("kind", "Kind"),
                    ("source", "Source"),
                    ("line", "Line"),
                    ("claim", "Claim"),
                    ("evidence", "Evidence"),
                    ("fix", "Fix"),
                ],
            ),
        ),
        spec(
            "audit",
            "Audit",
            "/mimus/audit",
            "`mimus audit` — trust-score of .kuda memory cards vs source: fresh / drifted / contradicted from provenance hashes.",
            &["audit_summary", "audit"],
            col(
                "Mimus audit",
                vec![
                    kv("Trust scores", "audit_summary"),
                    table(
                        "Cards",
                        "audit",
                        &[
                            ("card", "Card"),
                            ("name", "Module"),
                            ("score", "Score"),
                            ("files", "Files"),
                        ],
                    ),
                ],
            ),
        ),
        spec(
            "rules",
            "Rules",
            "/mimus/rules",
            "Convention rules as data — `rules/*.toml` (name, severity, file glob, source pattern) plus how many files each currently matches.",
            &["rules_summary", "rules"],
            col(
                "Mimus rules",
                vec![
                    kv("Rules", "rules_summary"),
                    table(
                        "Rule set",
                        "rules",
                        &[
                            ("name", "Name"),
                            ("severity", "Severity"),
                            ("files", "Files glob"),
                            ("matched", "Matched"),
                            ("pattern", "Pattern"),
                            ("message", "Message"),
                        ],
                    ),
                ],
            ),
        ),
    ]
}

/// The `manifest` op result.
pub fn manifest() -> DriverManifest {
    DriverManifest {
        name: "mimus".to_string(),
        title: "Mimus".to_string(),
        icon: Some("🐦".to_string()),
        version: env!("CARGO_PKG_VERSION").to_string(),
        verbs: vec!["manifest".to_string(), "serve".to_string()],
        capabilities: vec!["pavo".to_string()],
        contributes: Contributes {
            navbar: vec![Contribution {
                id: "violations".to_string(),
                title: "Mimus".to_string(),
                route: "/mimus/violations".to_string(),
                priority: 30,
                icon: None,
            }],
            dashboard: vec![Contribution {
                id: "status".to_string(),
                title: "Mimus status".to_string(),
                route: "/mimus/status".to_string(),
                priority: 40,
                icon: None,
            }],
            statusbar: vec![Contribution {
                id: "violations".to_string(),
                title: "Mimus violations".to_string(),
                route: "/mimus/violations".to_string(),
                priority: 50,
                icon: None,
            }],
            routes: Vec::new(),
            extra: Default::default(),
        },
        views: specs(),
        renderers: Vec::new(),
    }
}

// ---------------------------------------------------------------- queries

/// Every query name the driver answers — the `query` op rejects
/// anything outside this set with `ok:false`.
pub fn known_query(name: &str) -> bool {
    matches!(
        name,
        "summary" | "violations" | "audit" | "audit_summary" | "rules" | "rules_summary"
    )
}

/// `{"op":"query","query":name,"params":{…}}` → the data payload.
/// Failures degrade to `{"error": …}` objects — the wire stays
/// `ok:true` so hosts render the failure inline.
pub fn query_data(name: &str, params: &Map<String, Value>, snap: &Snapshot) -> Value {
    match name {
        "summary" => summary(snap),
        "violations" => violations(snap, params),
        "audit" => audit_rows(snap),
        "audit_summary" => audit_summary(snap),
        "rules" => rules_rows(snap),
        "rules_summary" => rules_summary(snap),
        other => json!({"error": format!("unknown query '{other}'")}),
    }
}

// ---------------------------------------------------------------- shared

/// Optional string param.
fn param(params: &Map<String, Value>, key: &str) -> Option<String> {
    params.get(key).and_then(Value::as_str).map(str::to_string)
}

/// `age` as "Nh" / "NNd" / "Nm" — the same shape `mimus doctor`
/// prints for index freshness.
fn fmt_age(d: std::time::Duration) -> String {
    let h = d.as_secs() / 3600;
    if h >= 24 {
        format!("{}d", h / 24)
    } else if h > 0 {
        format!("{h}h")
    } else {
        format!("{}m", d.as_secs() / 60)
    }
}

fn err(e: impl std::fmt::Display) -> Value {
    json!({"error": e.to_string()})
}

// ---------------------------------------------------------------- queries

/// `summary` query — the kv object: resolution + health digest.
fn summary(snap: &Snapshot) -> Value {
    let cfg = snap.config.as_ref();
    let idx = cfg.and_then(|c| c.check.kuda_index.as_ref());
    let idx_path = idx.map(|p| snap.root.join(p));
    let idx_age = idx_path
        .as_ref()
        .and_then(|p| mimus_check_graph::index_age(p));
    let triples = mimus_check_graph::triples_cache_rows(&snap.root.join(".mimus/triples.db"));
    let rules_dir = cfg
        .map(|c| c.rules.dir.clone())
        .unwrap_or_else(|| "rules".into());
    let rules_count = mimus_rules::load_rules(&snap.root.join(&rules_dir))
        .map(|r| r.len())
        .unwrap_or(0);
    let cards = mimus_rules::matching_files(&snap.root, ".kuda/modules/*.yaml").len();
    let mut out = Map::new();
    out.insert("root".into(), json!(snap.root.display().to_string()));
    out.insert("config".into(), json!(snap.config_path));
    out.insert(
        "mimus_cli".into(),
        json!(snap.mimus_bin.as_ref().map(|p| p.display().to_string())),
    );
    out.insert(
        "mechanical".into(),
        json!(cfg.map(|c| c.check.mechanical).unwrap_or(true)),
    );
    out.insert(
        "semantic".into(),
        json!(cfg.map(|c| c.check.semantic).unwrap_or(false)),
    );
    out.insert(
        "kuda_index".into(),
        json!(idx_path.as_ref().map(|p| p.display().to_string())),
    );
    out.insert("kuda_index_age".into(), json!(idx_age.map(fmt_age)));
    out.insert("triples_cache_rows".into(), json!(triples));
    out.insert("rules_dir".into(), json!(rules_dir.display().to_string()));
    out.insert("rules".into(), json!(rules_count));
    out.insert("cards".into(), json!(cards));
    if snap.config_path.is_none() {
        out.insert(
            "note".into(),
            json!("no mimus.toml — mimus:violations needs one; audit/rules still read <root>"),
        );
    }
    if !snap.warnings.is_empty() {
        out.insert("warnings".into(), json!(snap.warnings));
    }
    Value::Object(out)
}

/// `violations` query — `mimus report --format json` rows.
/// Filters: `severity` (error|warn), `kind` (path|command|symbol|
/// count|link|behavior), `source` (substring), `limit` (default
/// 200, 0 = all).
fn violations(snap: &Snapshot, params: &Map<String, Value>) -> Value {
    let severity = param(params, "severity").map(|s| s.to_lowercase());
    let kind = param(params, "kind").map(|s| s.to_lowercase());
    let source = param(params, "source").map(|s| s.to_lowercase());
    let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(200) as usize;

    let all = match crate::data::violations_json(snap) {
        Ok(v) => v,
        Err(e) => return err(e),
    };
    let mut rows: Vec<Value> = all
        .iter()
        .filter(|v| {
            severity
                .as_deref()
                .is_none_or(|s| v["severity"].as_str() == Some(s))
        })
        .filter(|v| {
            kind.as_deref()
                .is_none_or(|k| v["claim"]["kind"].as_str() == Some(k))
        })
        .filter(|v| {
            source.as_deref().is_none_or(|s| {
                v["claim"]["source"]
                    .as_str()
                    .map(|src| src.to_lowercase().contains(s))
                    .unwrap_or(false)
            })
        })
        .map(|v| {
            json!({
                "severity": v["severity"],
                "kind": v["claim"]["kind"],
                "source": v["claim"]["source"],
                "line": v["claim"]["line"],
                "claim": v["claim"]["text"],
                "evidence": v["evidence"],
                "fix": v["fix"],
            })
        })
        .collect();
    if limit > 0 {
        rows.truncate(limit);
    }
    Value::Array(rows)
}

// ---------------------------------------------------------------- audit

/// One `.kuda` card's trust verdict — the same computation
/// `mimus audit` performs: provenance files must exist and their
/// `git hash-object` must match the recorded hash.
struct CardRow {
    card: String,
    name: String,
    score: &'static str,
    files: usize,
}

/// Batch `git hash-object` for the provenance files that exist —
/// one spawn for the whole card set. Returns file → hash; a file
/// missing from the map counts as "" (drifted), matching the CLI's
/// `.ok().filter(success)` fallback.
fn hash_objects(root: &std::path::Path, files: &[String]) -> Map<String, Value> {
    let mut out = Map::new();
    let existing: Vec<&String> = files
        .iter()
        .filter(|f| root.join(f.as_str()).exists())
        .collect();
    if existing.is_empty() {
        return out;
    }
    let mut cmd = std::process::Command::new("git");
    cmd.arg("-C")
        .arg(root)
        .arg("hash-object")
        .arg("--")
        .args(existing.iter().map(|s| s.as_str()));
    let hashes = cmd
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for (f, h) in existing.iter().zip(hashes) {
        out.insert((*f).clone(), json!(h));
    }
    out
}

/// Scan `.kuda/modules/*.yaml` under root — `mimus audit`'s corpus.
fn audit_cards(snap: &Snapshot) -> Vec<CardRow> {
    let mut cards = Vec::new();
    // Collect (card path, name, files, hashes) first so hash-object
    // can batch across every card.
    struct Pending {
        card: String,
        name: String,
        files: Vec<String>,
        hashes: Vec<String>,
    }
    let mut pending = Vec::new();
    for p in mimus_rules::matching_files(&snap.root, ".kuda/modules/*.yaml") {
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        let Ok(v) = serde_yaml::from_str::<serde_yaml::Value>(&text) else {
            continue;
        };
        let name = v["name"].as_str().unwrap_or("?").to_owned();
        let seq = |key: &str| -> Vec<String> {
            v["provenance"][key]
                .as_sequence()
                .map(|s| {
                    s.iter()
                        .filter_map(|f| f.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        let card = p
            .strip_prefix(&snap.root)
            .unwrap_or(&p)
            .display()
            .to_string();
        pending.push(Pending {
            card,
            name,
            files: seq("files"),
            hashes: seq("hashes"),
        });
    }
    let all_files: Vec<String> = pending.iter().flat_map(|p| p.files.clone()).collect();
    let hashes = hash_objects(&snap.root, &all_files);
    for p in pending {
        let mut score = "fresh";
        for (f, h) in p.files.iter().zip(&p.hashes) {
            if !snap.root.join(f).exists() {
                score = "contradicted";
                break;
            }
            let cur = hashes.get(f).and_then(Value::as_str).unwrap_or("");
            if cur != h {
                score = "drifted";
            }
        }
        cards.push(CardRow {
            card: p.card,
            name: p.name,
            score,
            files: p.files.len(),
        });
    }
    cards
}

/// `audit` query — card rows for the table binding.
fn audit_rows(snap: &Snapshot) -> Value {
    Value::Array(
        audit_cards(snap)
            .into_iter()
            .map(|c| {
                json!({
                    "card": c.card,
                    "name": c.name,
                    "score": c.score,
                    "files": c.files,
                })
            })
            .collect(),
    )
}

/// `audit_summary` query — the kv object: score counts.
fn audit_summary(snap: &Snapshot) -> Value {
    let cards = audit_cards(snap);
    let count = |s: &str| cards.iter().filter(|c| c.score == s).count();
    json!({
        "cards": cards.len(),
        "fresh": count("fresh"),
        "drifted": count("drifted"),
        "contradicted": count("contradicted"),
    })
}

// ---------------------------------------------------------------- rules

/// `rules` query — the `rules/*.toml` rule set as rows. `matched`
/// counts files the rule's glob currently selects under root.
fn rules_rows(snap: &Snapshot) -> Value {
    let dir = snap
        .config
        .as_ref()
        .map(|c| c.rules.dir.clone())
        .unwrap_or_else(|| "rules".into());
    let rules = match mimus_rules::load_rules(&snap.root.join(&dir)) {
        Ok(r) => r,
        Err(e) => return err(format!("rules {}: {e}", dir.display())),
    };
    Value::Array(
        rules
            .iter()
            .map(|r| {
                let pattern = match &r.match_ {
                    mimus_rules::RuleMatch::SourcePattern { pattern } => pattern.clone(),
                };
                let matched = mimus_rules::matching_files(&snap.root, &r.files).len();
                json!({
                    "name": r.name,
                    "severity": r.severity,
                    "files": r.files,
                    "matched": matched,
                    "pattern": pattern,
                    "message": r.message,
                })
            })
            .collect(),
    )
}

/// `rules_summary` query — the kv object: dir + count.
fn rules_summary(snap: &Snapshot) -> Value {
    let dir = snap
        .config
        .as_ref()
        .map(|c| c.rules.dir.clone())
        .unwrap_or_else(|| "rules".into());
    let count = mimus_rules::load_rules(&snap.root.join(&dir))
        .map(|r| r.len())
        .unwrap_or(0);
    json!({
        "dir": dir.display().to_string(),
        "rules": count,
        "strict": snap.config.as_ref().map(|c| c.rules.strict).unwrap_or(false),
    })
}

// ---------------------------------------------------------------- view op

/// `{"op":"view","view":id,"params":{…}}` → [`pavo_protocol::wire::ViewResult`].
pub fn view_result(
    view_id: &str,
    params: &Map<String, Value>,
) -> Result<pavo_protocol::wire::ViewResult, String> {
    let spec = specs()
        .into_iter()
        .find(|v| v.id == view_id)
        .ok_or_else(|| format!("unknown view: {view_id}"))?;

    let snap = crate::data::load(params);
    let mut data = Map::new();
    for b in &spec.queries {
        data.insert(b.name.clone(), query_data(&b.name, params, &snap));
    }

    Ok(pavo_protocol::wire::ViewResult { view: spec, data })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn manifest_advertises_pavo_and_lists_every_view() {
        let m = manifest();
        assert!(m.verbs.iter().any(|v| v == "serve"));
        assert!(m.capabilities.iter().any(|c| c == "pavo"));
        let ids: std::collections::BTreeSet<&str> = m.views.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids.len(), m.views.len(), "duplicate view ids");
        for want in ["status", "violations", "audit", "rules"] {
            assert!(ids.contains(want), "missing view {want}");
        }
        assert!(m
            .contributes
            .navbar
            .iter()
            .any(|c| c.route == "/mimus/violations"));
        assert!(m
            .contributes
            .statusbar
            .iter()
            .any(|c| c.route == "/mimus/violations"));
        assert!(m
            .contributes
            .dashboard
            .iter()
            .any(|c| c.route == "/mimus/status"));
    }

    #[test]
    fn every_binding_is_a_known_query() {
        for v in specs() {
            for b in &v.queries {
                assert!(known_query(&b.name), "{}: unknown query {}", v.id, b.name);
            }
        }
    }

    #[test]
    fn fmt_age_scales() {
        assert_eq!(fmt_age(std::time::Duration::from_secs(30)), "0m");
        assert_eq!(fmt_age(std::time::Duration::from_secs(3600)), "1h");
        assert_eq!(fmt_age(std::time::Duration::from_secs(3 * 86400)), "3d");
    }

    #[test]
    fn degraded_snapshot_still_answers_in_process_queries() {
        // No mimus.toml, no .kuda cards, no rules dir — every query
        // must return a value (possibly an error object), never
        // panic or fail the op.
        let dir = std::env::temp_dir().join(format!("mimus-drv-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let snap = Snapshot {
            root: dir.clone(),
            mimus_bin: None,
            ..Snapshot::default()
        };
        let params = Map::new();
        for name in [
            "summary",
            "audit",
            "audit_summary",
            "rules",
            "rules_summary",
        ] {
            let v = query_data(name, &params, &snap);
            assert!(v.is_object() || v.is_array(), "{name}: {v}");
        }
        // violations without a mimus binary → honest error payload
        let v = query_data("violations", &params, &snap);
        assert!(
            v["error"].as_str().unwrap_or("").contains("mimus CLI"),
            "{v}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
