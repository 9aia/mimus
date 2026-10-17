//! Cmd impl — `command` claims resolve their binary on PATH.
//! v1 verifies *existence* of the invoked binary, not exit status —
//! executing documented commands belongs behind an explicit flag.

use malurus_check::Checker;
use malurus_core::{Claim, ClaimKind, Severity, Violation};
use std::path::Path;

/// Checks `ClaimKind::Command` — binary resolves in PATH.
pub struct CmdChecker {
    /// Disable execution-tier checks entirely on untrusted repos.
    pub allow_exec: bool,
}

impl CmdChecker {
    pub fn new() -> Self {
        Self { allow_exec: false }
    }
}

impl Default for CmdChecker {
    fn default() -> Self {
        Self::new()
    }
}

fn binary_on_path(bin: &str) -> bool {
    if bin.contains('/') {
        return Path::new(bin).exists();
    }
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|d| {
                let p = d.join(bin);
                p.is_file()
            })
        })
        .unwrap_or(false)
}

impl Checker for CmdChecker {
    fn handles(&self) -> &[ClaimKind] {
        &[ClaimKind::Command]
    }

    fn check(&self, claim: &Claim) -> Result<Option<Violation>, String> {
        let first = match claim.text.split_whitespace().next() {
            Some(b) => b.trim_matches('`'),
            None => return Ok(None),
        };
        // Skip shell builtins and obvious non-binaries.
        if matches!(
            first,
            "cd" | "echo" | "export" | "set" | "if" | "for" | "while" | "test" | "true" | "false"
        ) {
            return Ok(None);
        }
        if binary_on_path(first) {
            return Ok(None);
        }
        Ok(Some(Violation {
            claim: claim.clone(),
            evidence: format!("binary `{first}` not found in PATH"),
            fix: None,
            severity: Severity::Warn, // env-dependent — warn, not error
        }))
    }
}
