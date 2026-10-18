//! Symbol adapter — `symbol` claims resolve against a `kuda`
//! index. Consumes the index *file format* (`.kuda/index.sqlite`),
//! not the kuda crates — the schema is the contract.
//!
//! Evidence rules: a claim found in the index is verified;
//! a *namespaced* claim (`mod::Sym`) absent from it is refuted;
//! a bare identifier absent from it is silence — it may simply
//! be external (std/dep) and unproveable.

use mimus_check::Checker;
use mimus_core::{Claim, ClaimKind, Severity, Violation};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Checks `ClaimKind::Symbol` — `new(kuda_index_path)`.
/// `conn` is `None` when the index is missing/unreadable: every
/// claim then emits silence, not failures.
pub struct SymbolChecker {
    conn: Option<Mutex<Connection>>,
}

impl SymbolChecker {
    pub fn new(index: impl Into<PathBuf>) -> Self {
        let conn = Connection::open(index.into()).ok().map(Mutex::new);
        Self { conn }
    }

    /// Whether a usable index exists at the configured path.
    pub fn index_present(&self) -> bool {
        self.conn.is_some()
    }
}

/// Every indexed symbol — `(name, kind, file, module)` — for
/// coverage analysis: which symbols no doc ever names.
pub fn all_symbols(index: &Path) -> Result<Vec<(String, String, String, String)>, String> {
    let conn = Connection::open(index).map_err(|e| e.to_string())?;
    let mut st = conn
        .prepare("SELECT name,kind,file,module FROM symbols")
        .map_err(|e| e.to_string())?;
    let rows = st
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

struct Hit {
    file: String,
    line: i64,
    module: String,
}

impl Checker for SymbolChecker {
    fn handles(&self) -> &[ClaimKind] {
        &[ClaimKind::Symbol]
    }

    fn check(&self, claim: &Claim) -> Result<Option<Violation>, String> {
        let Some(conn) = &self.conn else {
            return Ok(None); // no index — nothing to verify against
        };
        let text = claim.text.trim();
        let (leaf, ns) = match text.rsplit_once("::") {
            Some((ns, leaf)) => (leaf, Some(ns)),
            None => (text, None),
        };
        let conn = conn.lock().map_err(|e| e.to_string())?;
        let mut st = conn
            .prepare("SELECT file,line,module FROM symbols WHERE name = ?1")
            .map_err(|e| e.to_string())?;
        let hits: Vec<Hit> = st
            .query_map(params![leaf], |r| {
                Ok(Hit {
                    file: r.get(0)?,
                    line: r.get(1)?,
                    module: r.get(2)?,
                })
            })
            .map_err(|e| e.to_string())?
            .flatten()
            .collect();

        match (ns, hits.is_empty()) {
            // bare ident found — verified
            (None, false) => Ok(None),
            // namespaced and found — the claimed path must agree
            (Some(ns), false) => {
                let ok = hits.iter().any(|h| {
                    h.file.replace('/', "::").contains(ns)
                        || h.module.replace('-', "_").contains(ns)
                });
                if ok {
                    return Ok(None);
                }
                let h = &hits[0];
                Ok(Some(Violation {
                    claim: claim.clone(),
                    evidence: format!(
                        "`{leaf}` lives at {}:{} (module {}), not `{ns}`",
                        h.file, h.line, h.module
                    ),
                    fix: Some(format!("`{}::{leaf}`", h.file.trim_end_matches(".rs").replace('/', "::"))),
                    severity: Severity::Error,
                }))
            }
            // namespaced and absent — the index covers the repo, so
            // the claimed path is refuted
            (Some(_), true) => Ok(Some(Violation {
                claim: claim.clone(),
                evidence: format!("`{text}` not found in .kuda index — renamed, moved, or dead?"),
                fix: Some("update the reference or run `kuda sync`".into()),
                severity: Severity::Error,
            })),
            // bare ident absent — could be std/dep; unproveable
            (None, true) => Ok(None),
        }
    }
}
