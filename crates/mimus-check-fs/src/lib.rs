//! Fs adapter — `path` claims resolve against the repo root.
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
        // unproveable claims — silence, not violations:
        if text.starts_with('~') || text.starts_with('$') {
            return Ok(None); // host/env-relative — can't verify per-repo
        }
        if text.contains('<') || text.contains('>') {
            return Ok(None); // placeholder syntax, not a literal path
        }
        // glob claims verify by expansion (docs legitimately write
        // `crates/*/Cargo.toml`), not literal existence
        if text.contains('*') || text.contains('?') || text.contains('[') {
            let pat = Path::new(&self.root).join(text).to_string_lossy().into_owned();
            return Ok(match glob::glob(&pat).map(|mut g| g.next().is_some()) {
                Ok(true) => None,
                _ => Some(Violation {
                    claim: claim.clone(),
                    evidence: format!("no files match `./{text}`"),
                    fix: None,
                    severity: Severity::Error,
                }),
            });
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
