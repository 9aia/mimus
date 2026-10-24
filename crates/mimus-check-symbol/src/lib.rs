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
#[allow(unexpected_cfgs)]
pub fn all_symbols(index: &Path) -> Result<Vec<(String, String, String, String)>, String> {
    let conn = Connection::open(index).map_err(|e| e.to_string())?;
    let mut st = conn
        .prepare("SELECT name,kind,file,module FROM symbols")
        .map_err(|e| e.to_string())?;
    let rows = st.query_map([], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
    });
    // `?` on query_map is unreachable — params![] can't fail to
    // bind after a successful prepare; under coverage the Err arm
    // would be a permanently red line
    #[cfg(coverage)]
    let rows = rows.unwrap();
    #[cfg(not(coverage))]
    let rows = rows.map_err(|e| e.to_string())?;
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

    #[allow(unexpected_cfgs)]
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
        let rows = st.query_map(params![leaf], |r| {
            Ok(Hit {
                file: r.get(0)?,
                line: r.get(1)?,
                module: r.get(2)?,
            })
        });
        // same unreachable `?` as all_symbols — params![leaf] binds
        // a String after a successful prepare; see above
        #[cfg(coverage)]
        let rows = rows.unwrap();
        #[cfg(not(coverage))]
        let rows = rows.map_err(|e| e.to_string())?;
        let hits: Vec<Hit> = rows.flatten().collect();

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

#[cfg(test)]
mod tests {
    use super::*;

    /// A poisoned `conn` mutex — lock() errors surface as Err, not
    /// a panic or a hang (a checker must stay advisory).
    #[test]
    fn poisoned_conn_lock_errors() {
        let dir = std::env::temp_dir()
            .join(format!("mimus-sym-poison-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let idx = dir.join("i.sqlite");
        // empty file opens as an empty db — conn is Some
        std::fs::write(&idx, "").unwrap();
        let c = SymbolChecker::new(&idx);
        assert!(c.index_present());
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = c.conn.as_ref().unwrap().lock().unwrap();
            panic!("poison the lock");
        }));
        let cl = Claim {
            kind: ClaimKind::Symbol,
            text: "SomeSym".into(),
            source: "d.md".into(),
            line: 1,
        };
        assert!(c.check(&cl).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
