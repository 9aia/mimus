//! Cmd adapter — `command` claims resolve their binary on PATH.
//! Default verifies *existence* of the invoked binary only; the
//! opt-in `allow_exec` tier additionally runs the claim as argv
//! (no shell, bounded by `exec_timeout`) — a non-zero exit or
//! timeout refutes the claim.

use mimus_check::Checker;
use mimus_core::{Claim, ClaimKind, Severity, Violation};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// Checks `ClaimKind::Command` — binary resolves in PATH.
pub struct CmdChecker {
    /// Opt-in execution tier — run PATH-resolved command claims
    /// and refute on non-zero exit. Disable on untrusted repos:
    /// claim text becomes argv (no shell, but still arbitrary
    /// programs with the checker's privileges).
    pub allow_exec: bool,
    /// Bound on a single executed claim — the process is killed
    /// past this. Only consulted when `allow_exec` is set.
    pub exec_timeout: Duration,
    /// Tool prerequisites the repo declares ([check].binaries) —
    /// commands invoking these are assumed correct and skip the
    /// PATH probe: docs may legitimately require tools the
    /// checker's machine doesn't have.
    pub declared: Vec<String>,
}

impl CmdChecker {
    pub fn new() -> Self {
        Self {
            allow_exec: false,
            exec_timeout: Duration::from_secs(5),
            declared: Vec::new(),
        }
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

/// Claim text → argv: whitespace split, backticks trimmed, no
/// shell — metacharacters become literal args, never interpreted.
fn argv(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|t| t.trim_matches('`').to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// `None` = still running at the deadline (killed). `Err` = the
/// process never spawned.
fn run_bounded(argv: &[String], timeout: Duration) -> Result<Option<ExitStatus>, String> {
    let mut child = Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn failed: {e}"))?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status)),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(None);
            }
            Err(e) => return Err(format!("wait failed: {e}")),
        }
    }
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
        if self.declared.iter().any(|d| d == first) {
            return Ok(None); // declared prerequisite — don't probe PATH
        }
        if !binary_on_path(first) {
            return Ok(Some(Violation {
                claim: claim.clone(),
                evidence: format!("binary `{first}` not found in PATH"),
                fix: None,
                severity: Severity::Warn, // env-dependent — warn, not error
            }));
        }
        if !self.allow_exec {
            return Ok(None);
        }
        let args = argv(&claim.text);
        if args.is_empty() {
            return Ok(None);
        }
        let evidence = match run_bounded(&args, self.exec_timeout) {
            Ok(Some(status)) if status.success() => return Ok(None),
            Ok(Some(status)) => format!("command exited with {status}"),
            Ok(None) => format!("command timed out after {:?}", self.exec_timeout),
            Err(e) => format!("command could not be executed: {e}"),
        };
        Ok(Some(Violation {
            claim: claim.clone(),
            evidence,
            fix: None,
            severity: Severity::Warn, // env-dependent — warn, not error
        }))
    }
}
