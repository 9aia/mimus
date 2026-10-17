//! Fs impl — `path` claims resolve against the repo root.
//! The simplest checker and the safest: existence is provable.

use mimus_check::Checker;
use mimus_core::{Claim, ClaimKind, Severity, Violation};
use std::path::{Path, PathBuf};

/// Checks `ClaimKind::Path` — `new(repo_root)`.
pub struct FsChecker {
    root: PathBuf,
}

impl FsChecker {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl Checker for FsChecker {
    fn handles(&self) -> &[ClaimKind] {
        &[ClaimKind::Path]
    }

    fn check(&self, claim: &Claim) -> Result<Option<Violation>, String> {
        let text = claim.text.trim_matches('`').trim_end_matches('/');
        if text.is_empty() || text.starts_with("http") {
            return Ok(None); // not a path claim in practice — silence
        }
        let p = Path::new(&self.root).join(text);
        if p.exists() {
            return Ok(None);
        }
        Ok(Some(Violation {
            claim: claim.clone(),
            evidence: format!("{} does not exist", p.display()),
            fix: None,
            severity: Severity::Error,
        }))
    }
}
