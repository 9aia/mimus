//! Claim extraction — find factual assertions in prose, comments,
//! and cards. Conservative by design: extracting a claim that
//! isn't one produces noise downstream (see ADR-0001).

use mimus_core::{Claim, ClaimKind};
use std::path::Path;

/// Extract claims from document text. v1 mines backtick-quoted
/// spans — the highest-precision source: writers quote exactly
/// the things they assert (paths, commands, names).
pub fn extract(text: &str, source: &Path) -> Vec<Claim> {
    // <!-- mimus-ignore-file --> — intentional dead refs throughout.
    if text.contains("mimus-ignore-file") {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut in_fence = false;
    let mut para = String::new();
    let mut para_start = 0usize;
    let mut para_end = 0usize;
    let mut suppress_next = false;
    for (line_no, line) in text.lines().enumerate() {
        // <!-- mimus-ignore --> — alone on a line it suppresses
        // the NEXT line's claims; inline it suppresses its own.
        if line.contains("mimus-ignore") {
            suppress_next = !line.contains('`');
            continue;
        }
        if std::mem::take(&mut suppress_next) {
            continue;
        }
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        // fenced blocks are examples/output, not claims about this
        // repo — a tutorial showing `nonexistent/dead.rs` as sample
        // output must not become a dead-path violation
        if in_fence {
            continue;
        }
        // table rows are enumerations of syntax, not assertions —
        // "| `path` | `src/lib.rs` |" is an example, not a claim
        if line.trim_start().starts_with('|') {
            continue;
        }
        for span in backtick_spans(line) {
            // `[text](url)` link syntax backticked is not a path
            if span.contains("](") {
                continue;
            }
            if let Some(kind) = classify(span) {
                out.push(Claim {
                    kind,
                    text: span.to_owned(),
                    source: source.to_path_buf(),
                    line: (line_no + 1) as u32,
                });
            }
        }
        // Paragraph accumulation: hard-wrapped md splits sentences
        // across lines — join, flush on blank/heading/fence.
        let t = line.trim();
        let breaks = t.is_empty() || t.starts_with('#') || t.starts_with('|') || in_fence;
        if breaks {
            flush_para(&mut para, para_start, para_end, source, &mut out);
            continue;
        }
        if t.starts_with('-') || t.starts_with('*') || t.starts_with('>') {
            continue; // list items / quotes — fragments without context
        }
        if para.is_empty() {
            para_start = line_no + 1;
        }
        para_end = line_no + 1;
        if !para.is_empty() {
            para.push(' ');
        }
        para.push_str(t);
    }
    flush_para(&mut para, para_start, para_end, source, &mut out);
    out
}

/// Sentence-split a paragraph into Behavior claims — whole
/// sentences only, capped per file.
fn flush_para(
    para: &mut String,
    start: usize,
    end: usize,
    source: &Path,
    out: &mut Vec<Claim>,
) {
    if para.is_empty() {
        return;
    }
    let text = std::mem::take(para);
    let seen = out
        .iter()
        .filter(|c| c.source == source && c.kind == ClaimKind::Behavior)
        .count();
    let mut budget = 10usize.saturating_sub(seen);
    for sent in sentences(&text) {
        if budget == 0 {
            break;
        }
        if !is_assertion(sent) {
            continue;
        }
        budget -= 1;
        out.push(Claim {
            kind: ClaimKind::Behavior,
            text: sent.to_owned(),
            source: source.to_path_buf(),
            // line granularity: use paragraph start; precise
            // mapping needs offsets, not worth it v1
            line: start.max(end) as u32,
        });
    }
}

/// Naive sentence split on ". " / ".\n" — good enough for claim
/// extraction (false splits cost a claim, not correctness).
fn sentences(text: &str) -> Vec<&str> {
    text.split(". ")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect()
}

fn is_assertion(s: &str) -> bool {
    !s.ends_with(':') // list intro, not a claim
        && s.split_whitespace().count() >= 6
        && s.split_whitespace().any(|w| {
            matches!(
                w.trim_matches(|c: char| !c.is_alphabetic()),
                "is" | "are" | "stores" | "uses" | "reads" | "writes"
                    | "returns" | "contains" | "supports" | "defaults"
            )
        })
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
        // `VAR=value` assignments and template/glob-placeholder
        // spans name a shape, not a concrete file — unverifiable.
        if looks_like_env_assignment(s) || looks_like_template(s) {
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
    // Whole-span `<name>` is a placeholder, not a generic type.
    if s.starts_with('<') && s.ends_with('>') {
        return None;
    }
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '<' | '>'))
        && s.chars().any(|c| c.is_ascii_alphabetic())
        && s.len() >= 3
    {
        return Some(ClaimKind::Symbol);
    }
    None
}

/// `FOO=bar`, `NEURALCHAT_BFF_PUBLIC_URL=http://x` — an env
/// assignment, not a path the repo can satisfy.
fn looks_like_env_assignment(s: &str) -> bool {
    match s.split_once('=') {
        Some((k, _)) => {
            !k.is_empty()
                && !k.chars().next().unwrap().is_ascii_digit()
                && k.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        None => false,
    }
}

/// Spans naming a *pattern* — `..._x.md`, `{a,b}`, `<name>.rs`,
/// `adr-mkt-0N.md`, `.md` — not a concrete path claim.
fn looks_like_template(s: &str) -> bool {
    if s.contains("...")
        || s.contains('{')
        || s.contains('}')
        || s.contains('<')
        || s.contains('>')
    {
        return true;
    }
    // bare extension (`.md`) — a suffix, not a file
    if let Some(rest) = s.strip_prefix('.') {
        if !rest.is_empty() && !rest.contains(['.', '/', '\\']) {
            return true;
        }
    }
    // digit+uppercase fragment inside a segment (`0N`, `0NN`) —
    // serial placeholders, not real basenames. `v18`/`rfc1234`
    // (letter-then-digit or lowercase) stay checkable.
    s.split('/').any(|seg| {
        seg.as_bytes()
            .windows(2)
            .any(|w| w[0].is_ascii_digit() && w[1].is_ascii_uppercase())
    })
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

    #[test]
    fn skips_env_assignments() {
        let claims = extract(
            "set `NEURALCHAT_BFF_PUBLIC_URL=http://localhost:3000` then `FOO=bar`",
            "d".as_ref(),
        );
        assert!(claims.is_empty(), "{:?}", claims);
    }

    #[test]
    fn skips_template_spans() {
        let claims = extract(
            "name it `adr-mkt-0N.md` or `..._planejamento.md`, glob `{a,b}.toml`, type `.md`, param `<path>`",
            "d".as_ref(),
        );
        assert!(claims.is_empty(), "{:?}", claims);
    }

    #[test]
    fn mimus_ignore_suppresses() {
        // marker alone on a line -> next line's claims
        let c = extract(
            "<!-- mimus-ignore -->\nsee `dead/x.md` and `real/README.md`",
            "d".as_ref(),
        );
        assert!(c.is_empty(), "{:?}", c);
        // inline marker -> same line only
        let c = extract(
            "see `dead/x.md` <!-- mimus-ignore -->\nbut `README.md` lives",
            "d".as_ref(),
        );
        assert_eq!(c.len(), 1, "{:?}", c);
        assert_eq!(c[0].text, "README.md");
        // file-level marker
        let c = extract("<!-- mimus-ignore-file -->\n`README.md`", "d".as_ref());
        assert!(c.is_empty(), "{:?}", c);
    }
}

/// Extract assertive comment lines as Behavior claims — the
/// code↔code consistency surface: comments that assert things the
/// code may or may not do. `//`, `///`, `//!`, `#` lines; capped.
pub fn extract_comments(text: &str, source: &Path) -> Vec<Claim> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let t = line.trim();
        let body = t
            .strip_prefix("///")
            .or_else(|| t.strip_prefix("//!"))
            .or_else(|| t.strip_prefix("//"))
            .or_else(|| t.strip_prefix('#'))
            .map(str::trim);
        let Some(b) = body else { continue };
        if b.len() < 20 || b.starts_with('!') || b.starts_with('[') {
            continue; // doc attrs / annotations
        }
        if !is_assertion(b) {
            continue;
        }
        out.push(Claim {
            kind: ClaimKind::Behavior,
            text: b.to_owned(),
            source: source.to_path_buf(),
            line: (i + 1) as u32,
        });
        if out.len() >= 10 {
            break;
        }
    }
    out
}
