//! Fs adapter — `path` claims resolve against the repo root.
//! The simplest checker and the safest: existence is provable.

use mimus_check::Checker;
use mimus_core::{Claim, ClaimKind, Severity, Violation};
use std::path::PathBuf;

/// Checks `ClaimKind::Path` — `new(repo_root)`. Claims resolve
/// against the repo root, then the claiming doc's own dir, then
/// each entry of `base_dirs` (component roots docs write relative
/// to, e.g. `infrastructure/postgres`). All must miss to refute.
pub struct FsChecker {
    root: PathBuf,
    /// Extra repo-relative bases to try before declaring a miss.
    pub base_dirs: Vec<PathBuf>,
}

impl FsChecker {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            base_dirs: Vec::new(),
        }
    }

    /// Candidate bases in precedence order: repo root, the doc's
    /// dir, then configured base_dirs.
    fn bases(&self, claim: &Claim) -> Vec<PathBuf> {
        let mut v = vec![self.root.clone()];
        if let Some(d) = claim.source.parent() {
            if !d.as_os_str().is_empty() {
                v.push(d.to_path_buf());
            }
        }
        v.extend(self.base_dirs.iter().map(|b| self.root.join(b)));
        v
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
        // `/channel`, `/etc/foo` — a leading slash reads as a URL
        // route or a host-absolute path; neither is provable
        // against a repo root → silence
        if text.starts_with('/') {
            return Ok(None);
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
            let hit = self.bases(claim).iter().any(|b| {
                let pat = b.join(text).to_string_lossy().into_owned();
                glob::glob(&pat)
                    .map(|mut g| g.next().is_some())
                    .unwrap_or(false)
            });
            return Ok(if hit {
                None
            } else {
                Some(Violation {
                    claim: claim.clone(),
                    evidence: format!("no files match `./{text}`"),
                    fix: None,
                    severity: Severity::Error,
                })
            });
        }
        if self.bases(claim).iter().any(|b| b.join(text).exists()) {
            return Ok(None);
        }
        // `a/b`-shaped misses (MIME types, RPC method names like
        // `tools/list`) aren't provably repo paths → silence, per
        // the evidence bar. Hyphenated/dotted segments
        // (`docs/how-to`, `src/lib.rs`) stay real claims.
        if route_shaped(text) {
            return Ok(None);
        }
        Ok(Some(Violation {
            claim: claim.clone(),
            evidence: format!("./{text} does not exist (tried repo root, doc dir, base_dirs)"),
            fix: None,
            severity: Severity::Error,
        }))
    }
}

/// `application/pdf`, `tools/list`, `@scope/pkg` — exactly two
/// lowercase-word segments, no extension. Reads as a
/// route/MIME/method/scoped-package, not a repo path.
fn route_shaped(s: &str) -> bool {
    let s = s.strip_prefix('@').unwrap_or(s); // scoped pkg name
    let mut it = s.split('/');
    matches!(
        (it.next(), it.next(), it.next()),
        (Some(a), Some(b), None)
            if !a.is_empty()
                // `!b.is_empty()` is dead — a trailing `/` is
                // trimmed upstream, so `a/` never arrives here
                && a.bytes().all(|c| c.is_ascii_lowercase())
                && b.bytes().all(|c| c.is_ascii_lowercase())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(text: &str) -> Claim {
        Claim {
            kind: ClaimKind::Path,
            text: text.into(),
            source: "d.md".into(),
            line: 1,
        }
    }

    #[test]
    fn leading_slash_is_silence() {
        let c = FsChecker::new(".");
        for t in ["/channel", "/users", "/consulta-tuss", "/etc/passwd"] {
            assert_eq!(c.check(&claim(t)).unwrap().is_none(), true, "{t}");
        }
    }

    #[test]
    fn route_shaped_miss_is_silence() {
        let c = FsChecker::new(".");
        for t in [
            "application/pdf",
            "tools/list",
            "resources/read",
            "@scope/pkg",
        ] {
            assert_eq!(c.check(&claim(t)).unwrap().is_none(), true, "{t}");
        }
    }

    #[test]
    fn real_dead_path_still_flags() {
        let c = FsChecker::new(".");
        let v = c
            .check(&claim("docs/definitely-not-here-9f3a/x.md"))
            .unwrap();
        assert!(v.is_some());
    }

    #[test]
    fn doc_relative_resolves() {
        // `tests/store.rs` doesn't exist under this crate's dir
        // but does under the claiming doc's dir (mimus-dotenv).
        let c = FsChecker::new(".");
        let mut cl = claim("tests/store.rs");
        cl.source = "../mimus-dotenv/some-doc.md".into();
        assert!(c.check(&cl).unwrap().is_none());
    }

    #[test]
    fn base_dirs_resolve() {
        let mut c = FsChecker::new(".");
        c.base_dirs = vec!["../mimus-dotenv".into()];
        assert!(c.check(&claim("tests/store.rs")).unwrap().is_none());
    }
}
