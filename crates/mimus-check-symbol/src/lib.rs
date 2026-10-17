//! Symbol impl — `symbol` claims resolve against a `kuda` index.
//! Consumes the index *file format* (`.kuda/index.sqlite`), not the
//! kuda crates — the schema is the contract. v1 is a stub: it
//! verifies index presence and defers lookups until the kuda
//! schema stabilizes (see project/TODO.md).

use mimus_check::Checker;
use mimus_core::{Claim, ClaimKind, Violation};
use std::path::PathBuf;

/// Checks `ClaimKind::Symbol` — `new(kuda_index_path)`.
pub struct SymbolChecker {
    index: PathBuf,
}

impl SymbolChecker {
    pub fn new(index: impl Into<PathBuf>) -> Self {
        Self { index: index.into() }
    }

    /// Whether a usable index exists at the configured path.
    pub fn index_present(&self) -> bool {
        self.index.is_file()
    }
}

impl Checker for SymbolChecker {
    fn handles(&self) -> &[ClaimKind] {
        &[ClaimKind::Symbol]
    }

    fn check(&self, _claim: &Claim) -> Result<Option<Violation>, String> {
        // Evidence-or-silence: without a stable kuda schema we
        // cannot prove a symbol missing — emit nothing yet.
        Ok(None)
    }
}
