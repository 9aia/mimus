//! Claim extraction — find factual assertions in prose, comments,
//! and cards. Conservative by design: extracting a claim that
//! isn't one produces noise downstream (see ADR-0001).

use mimus_core::{Claim, ClaimKind};
use std::path::Path;

/// Extract claims from document text. v1 mines backtick-quoted
/// spans — the highest-precision source: writers quote exactly
/// the things they assert (paths, commands, names).
pub fn extract(text: &str, source: &Path) -> Vec<Claim> {
    let mut out = Vec::new();
    let mut in_fence = false;
    let mut behavior_seen = 0usize;
    for (line_no, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        for span in backtick_spans(line) {
            if let Some(kind) = classify(span) {
                out.push(Claim {
                    kind,
                    text: span.to_owned(),
                    source: source.to_path_buf(),
                    line: (line_no + 1) as u32,
                });
            }
        }
        // prose assertions -> Behavior claims for the semantic
        // checker; unhandled kinds are skipped by checkers that
        // don't claim them, so mechanical runs are unaffected.
        // Cap per file: docs full of assertions shouldn't flood
        // the (rate-limited) agent.
        if !in_fence && behavior_seen < 10 {
            let t = line.trim();
            let assertive = t.ends_with('.')
                && !t.starts_with('#')
                && !t.starts_with('|')
                && !t.starts_with('-')
                && !t.starts_with('*')
                && t.split_whitespace().count() >= 6
                && t.split_whitespace().any(|w| {
                    matches!(
                        w.trim_matches(|c: char| !c.is_alphabetic()),
                        "is" | "are" | "stores" | "uses" | "reads" | "writes"
                            | "returns" | "contains" | "supports" | "defaults"
                    )
                });
            if assertive {
                behavior_seen += 1;
                out.push(Claim {
                    kind: ClaimKind::Behavior,
                    text: t.to_owned(),
                    source: source.to_path_buf(),
                    line: (line_no + 1) as u32,
                });
            }
        }
    }
    out
}

/// Text inside `…` spans on one line.
fn backtick_spans(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            // skip run of 2+ backticks (code fences)
            if bytes.get(i + 1) == Some(&b'`') {
                i += 1;
                while i < bytes.len() && bytes[i] == b'`' {
                    i += 1;
                }
                continue;
            }
            if let Some(end) = line[i + 1..].find('`') {
                let span = &line[i + 1..i + 1 + end];
                if !span.is_empty() {
                    out.push(span);
                }
                i += end + 2;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Classify a backticked span — None means "not a checkable
/// claim" (formatting, code refs we can't verify, etc.).
fn classify(span: &str) -> Option<ClaimKind> {
    let s = span.trim();
    if s.is_empty() || s.len() > 200 {
        return None;
    }
    if s.starts_with("http://") || s.starts_with("https://") {
        return Some(ClaimKind::Link);
    }
    // Path-ish: contains / and no spaces, or ends in a known ext.
    if !s.contains(char::is_whitespace)
        && (s.contains('/')
            || s.ends_with(".rs")
            || s.ends_with(".md")
            || s.ends_with(".toml")
            || s.ends_with(".yaml")
            || s.ends_with(".yml")
            || s.ends_with(".sh"))
    {
        // But not env vars, flags, or dotted identifiers.
        if s.starts_with('-') || s.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
            return None;
        }
        return Some(ClaimKind::Path);
    }
    // Command-ish: only when the first token is a known binary —
    // "starts with a lowercase word" eats code keywords (pub, data).
    if s.contains(' ') {
        let first = s.split_whitespace().next()?;
        const BINARIES: &[&str] = &[
            "cargo", "rustc", "rustup", "git", "gh", "npm", "npx", "yarn", "pnpm", "bun",
            "deno", "node", "pip", "pip3", "uv", "python", "python3", "pytest", "poetry",
            "go", "make", "cmake", "ninja", "meson", "bazel", "mvn", "gradle", "docker",
            "kubectl", "helm", "terraform", "ansible", "systemctl", "journalctl", "dagger",
            "curl", "wget", "ssh", "scp", "rsync", "jq", "tar", "gzip", "zip", "unzip",
            "rg", "grep", "find", "sed", "awk", "sort", "diff", "patch", "xargs", "tee",
            "sh", "bash", "zsh", "sqlite3", "psql", "mysql", "openssl", "ffmpeg",
            "nextest", "cargo-nextest", "kuda", "mimus", "bupha", "hnl", "clippy",
            "rustfmt", "rustdoc", "wasm-pack", "trunk", "vite", "tsc", "eslint",
        ];
        if BINARIES.contains(&first) {
            return Some(ClaimKind::Command);
        }
        return None;
    }
    // Bare identifier → symbol candidate (snake/camel/Upper idents).
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '<' | '>'))
        && s.chars().any(|c| c.is_ascii_alphabetic())
        && s.len() >= 3
    {
        return Some(ClaimKind::Symbol);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_paths_and_commands() {
        let claims = extract(
            "run `cargo test -p foo` then see `crates/x/src/lib.rs` and `README.md`",
            "doc.md".as_ref(),
        );
        let kinds: Vec<_> = claims.iter().map(|c| (c.kind, c.text.as_str())).collect();
        assert!(kinds.contains(&(ClaimKind::Command, "cargo test -p foo")));
        assert!(kinds.contains(&(ClaimKind::Path, "crates/x/src/lib.rs")));
        assert!(kinds.contains(&(ClaimKind::Path, "README.md")));
    }

    #[test]
    fn skips_formatting_and_env() {
        let claims = extract("`KUDA_TOKEN` and `--flag` and `someVar_123`", "d".as_ref());
        assert!(claims.iter().all(|c| c.kind == ClaimKind::Symbol));
    }
}
