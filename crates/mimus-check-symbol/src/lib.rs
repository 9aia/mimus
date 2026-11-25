//! Symbol adapter — `symbol` claims resolve against a `kuda`
//! index, read through `kuda-store-sqlite`'s blessed file-format
//! contract ([`SqliteStore::open_readonly`]) — never raw SQL, so
//! index-schema drift can't silently break the checker.
//!
//! Evidence rules: a claim found in the index is verified;
//! a *namespaced* claim (`mod::Sym`) absent from it is refuted;
//! a bare identifier absent from it is silence — it may simply
//! be external (std/dep) and unproveable.

use kuda_core::Symbol;
use kuda_store::IndexStore;
use kuda_store_sqlite::SqliteStore;
use mimus_check::Checker;
use mimus_core::{Claim, ClaimKind, Severity, Violation};
use std::path::{Path, PathBuf};

/// Checks `ClaimKind::Symbol` — `new(kuda_index_path)`.
/// `store` is `None` when the index is missing/unreadable: every
/// claim then emits silence, not failures.
pub struct SymbolChecker {
    store: Option<SqliteStore>,
}

impl SymbolChecker {
    pub fn new(index: impl Into<PathBuf>) -> Self {
        // read-only open: a missing index stays missing — the old
        // `Connection::open` would have *created* an empty file
        let store = SqliteStore::open_readonly(&index.into()).ok();
        Self { store }
    }

    /// Whether a usable index exists at the configured path.
    pub fn index_present(&self) -> bool {
        self.store.is_some()
    }
}

/// Every indexed symbol — for coverage analysis: which symbols no
/// doc ever names.
pub fn all_symbols(index: &Path) -> Result<Vec<Symbol>, String> {
    SqliteStore::open_readonly(index)?.all_symbols()
}

impl Checker for SymbolChecker {
    fn handles(&self) -> &[ClaimKind] {
        &[ClaimKind::Symbol]
    }

    fn check(&self, claim: &Claim) -> Result<Option<Violation>, String> {
        let Some(store) = &self.store else {
            return Ok(None); // no index — nothing to verify against
        };
        let text = claim.text.trim();
        let (leaf, ns) = match text.rsplit_once("::") {
            Some((ns, leaf)) => (leaf, Some(ns)),
            None => (text, None),
        };
        let hits = store.find(leaf)?;

        match (ns, hits.is_empty()) {
            // bare ident found — verified
            (None, false) => Ok(None),
            // namespaced and found — the claimed path must agree
            (Some(ns), false) => {
                let ok = hits.iter().any(|h| {
                    h.file.to_string_lossy().replace('/', "::").contains(ns)
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
                        h.file.display(),
                        h.line,
                        h.module
                    ),
                    fix: Some(format!(
                        "`{}::{leaf}`",
                        h.file
                            .to_string_lossy()
                            .trim_end_matches(".rs")
                            .replace('/', "::")
                    )),
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
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn claim(text: &str) -> Claim {
        Claim {
            kind: ClaimKind::Symbol,
            text: text.into(),
            source: "d.md".into(),
            line: 1,
        }
    }

    /// A missing index is silence, not an error — and the open must
    /// not create the file (auditor-safe read).
    #[test]
    fn missing_index_is_silence_and_creates_nothing() {
        let dir = std::env::temp_dir().join(format!("mimus-sym-miss-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let idx = dir.join("absent.sqlite");
        let c = SymbolChecker::new(&idx);
        assert!(!c.index_present());
        assert!(c.check(&claim("SomeSym")).unwrap().is_none());
        assert!(!idx.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A schema-broken index (no `symbols` table) surfaces as a
    /// checker error, not a panic or a hang (a checker must stay
    /// advisory).
    #[test]
    fn broken_index_errors_per_claim() {
        let dir = std::env::temp_dir().join(format!("mimus-sym-brk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let idx = dir.join("i.sqlite");
        // zero-byte file opens as an empty db — reads hit "no such
        // table", the same failure a schema-broken index produces
        std::fs::write(&idx, b"").unwrap();
        let c = SymbolChecker::new(&idx);
        assert!(c.index_present());
        assert!(c.check(&claim("SomeSym")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
