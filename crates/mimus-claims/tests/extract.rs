//! extract() branches beyond the inline unit tests: fences,
//! tables, links, paragraphs → Behavior claims, budgets, spans.

use mimus_claims::{extract, extract_comments};
use mimus_core::ClaimKind;
use std::path::Path;

fn kinds(text: &str) -> Vec<(ClaimKind, String)> {
    extract(text, Path::new("d.md"))
        .into_iter()
        .map(|c| (c.kind, c.text))
        .collect()
}

#[test]
fn fenced_blocks_and_tables_are_skipped() {
    let c = kinds("pre `README.md`\n```\n`dead/x.rs`\n```\n| `a/b.rs` | col |\npost `Cargo.toml`");
    let texts: Vec<_> = c.iter().map(|(_, t)| t.as_str()).collect();
    assert!(texts.contains(&"README.md"));
    assert!(texts.contains(&"Cargo.toml"));
    assert!(!texts.contains(&"dead/x.rs"), "{texts:?}");
    assert!(!texts.contains(&"a/b.rs"), "{texts:?}");
    // unclosed fence swallows the rest
    let c = kinds("`README.md`\n```\n`dead/y.rs`");
    assert_eq!(c.len(), 1);
}

#[test]
fn link_spans_and_formatting_skipped() {
    // `[text](url)` backticked → not a path claim
    let c = kinds("see `[label](http://x)` done");
    assert!(c.is_empty(), "{c:?}");
    // long span > 200 chars → not a claim
    let long = "a".repeat(210);
    let c = kinds(&format!("`{long}`"));
    assert!(c.is_empty());
    // `-flag`-like spans are not claims; `CAPS/x` IS a real path claim
    let c = kinds("run with `--verbose` or `SOME_CONST/x`");
    assert!(
        c.contains(&(ClaimKind::Path, "SOME_CONST/x".into())),
        "{c:?}"
    );
    assert!(!c.iter().any(|(_, t)| t == "--verbose"), "{c:?}");
}

#[test]
fn urls_and_commands_and_words() {
    let c = kinds("docs at `https://example.com/spec` then `grep -r foo`");
    assert!(c.contains(&(ClaimKind::Link, "https://example.com/spec".into())));
    assert!(c.contains(&(ClaimKind::Command, "grep -r foo".into())));
    // spaced text whose first word is no known binary → not a claim
    let c = kinds("the words `pub struct Foo` appear");
    assert!(c.is_empty(), "{c:?}");
}

#[test]
fn paragraphs_become_behavior_claims() {
    let doc = "The store uses sqlite for persistence and it never caches \
               results across runs.\n\nHeading intro:\n- a list item with \
               `x/y.rs`\n> quoted stuff\n\n# Next\nTail line.";
    let claims = extract(doc, Path::new("d.md"));
    let behaviors: Vec<_> = claims
        .iter()
        .filter(|c| c.kind == ClaimKind::Behavior)
        .collect();
    assert!(
        behaviors.iter().any(|c| c.text.contains("sqlite")),
        "{claims:?}"
    );
    // list intro ending ':' is not an assertion
    assert!(!behaviors.iter().any(|c| c.text.contains("intro:")));
}

#[test]
fn paragraph_wraps_and_caps() {
    // hard-wrapped paragraph joins lines
    let doc = "The engine uses sqlite\nfor all persistence work today.\n";
    let claims = extract(doc, Path::new("d.md"));
    assert!(
        claims.iter().any(|c| c.text.contains("sqlite for all")),
        "{claims:?}"
    );
    // cap: >10 assertive paragraphs → at most 10 Behavior claims
    let many = (0..15)
        .map(|i| format!("Para{i} uses sqlite for persistence and returns values quickly."))
        .collect::<Vec<_>>()
        .join("\n\n");
    let claims = extract(&many, Path::new("d.md"));
    let n = claims
        .iter()
        .filter(|c| c.kind == ClaimKind::Behavior)
        .count();
    assert_eq!(n, 10, "{n}");
}

#[test]
fn backtick_runs_and_templates() {
    // `` `x` `` double-backtick run is skipped, not a span
    let c = kinds("code `` `inner` `` here `README.md`");
    assert!(c.iter().any(|(_, t)| t == "README.md"), "{c:?}");
    // `.md` bare extension → template, not a path
    let c = kinds("files of type `.md` here");
    assert!(c.is_empty(), "{c:?}");
    // env assignment looking span skipped
    let c = kinds("set `FOO_BAR=1/x` first");
    assert!(c.is_empty(), "{c:?}");
}

#[test]
fn comment_extraction_edges() {
    // 10-claim cap
    let src = (0..15)
        .map(|i| format!("// The module{i} uses sqlite for its persistence today"))
        .collect::<Vec<_>>()
        .join("\n");
    let claims = extract_comments(&src, Path::new("s.rs"));
    assert_eq!(claims.len(), 10);
    // short comments, doc attrs, annotations, non-comments skipped
    let src = "let x = 1;\n// too short\n//! [attr]\n// [note] thing uses sqlite for persistence x\n/* block */\n# short\n// a long comment without any trigger verb at all in it\n";
    let claims = extract_comments(src, Path::new("s.rs"));
    assert!(claims.is_empty(), "{claims:?}");
    // /// and #! bodies work
    let src = "/// The parser uses recursion for all nested structures\n#! /bin/sh\n# The script uses bash for all orchestration steps\n";
    let claims = extract_comments(src, Path::new("s.rs"));
    assert_eq!(claims.len(), 2, "{claims:?}");
}

#[test]
fn stray_backtick_flag_spans_and_dotfiles() {
    // trailing unclosed backtick → find('`') misses → i += 1
    let c = kinds("see `README.md` and a stray ` here");
    assert!(c.iter().any(|(_, t)| t == "README.md"), "{c:?}");
    // flag-shaped path-ish span → not a claim (starts with '-')
    let c = kinds("pass `--flag.toml` to the tool");
    assert!(c.is_empty(), "{c:?}");
    // `.config.toml` — leading dot with an inner dot → real path
    let c = kinds("see `.config.toml` here");
    assert!(
        c.contains(&(ClaimKind::Path, ".config.toml".into())),
        "{c:?}"
    );
}

#[test]
fn import_specifiers_are_not_paths() {
    // scoped packages, ~ aliases, bare package subpaths — JS import
    // specifiers, not repo-relative path claims
    let c = kinds(
        "deps: `@tanstack/react-router` and `nuqs/adapters/tanstack-router` \
         and `~/lib/utils` and `~components/button`",
    );
    assert!(c.is_empty(), "{c:?}");
    // snake_case + digit segments exercise every `||` arm of the
    // specifier filter — still specifiers, still silence
    let c = kinds("imports `foo_bar/baz9` and `a9b/c_d` here");
    assert!(c.is_empty(), "{c:?}");
    // empty segment (`a//b`) fails the per-segment check → Path
    let c = kinds("see `a//b` here");
    assert!(c.contains(&(ClaimKind::Path, "a//b".into())), "{c:?}");
}

#[test]
fn classifier_rejects_and_env_digit_edges() {
    // 1-2 char alnum span → below the symbol floor → not a claim
    assert!(kinds("the `ab` rev").is_empty());
    // disallowed chars (no space, not path-ish) → falls out: not a claim
    assert!(kinds("the `x+y+z` op").is_empty());
    // env-assignment with a leading-digit key → not env → Path claim
    // (the `=` survives classification; fs triages it downstream)
    let c = kinds("see `9foo=x.md` here");
    assert!(c.contains(&(ClaimKind::Path, "9foo=x.md".into())), "{c:?}");
    // dotted-leading specifier-ish segment → not a template → path
    let c = kinds("see `.a/b` here");
    assert!(c.contains(&(ClaimKind::Path, ".a/b".into())), "{c:?}");
    // env-assignment with an empty key (`=x`) or punctuation in the
    // key (`FO-O`) → not env → survives as a path claim
    let c = kinds("see `=x.md` and `FO-O=x.md` here");
    assert!(c.contains(&(ClaimKind::Path, "=x.md".into())), "{c:?}");
    assert!(c.contains(&(ClaimKind::Path, "FO-O=x.md".into())), "{c:?}");
}

#[test]
fn genuine_paths_survive_specifier_filter() {
    let c = kinds("see `src/foo.rs` and `crates/x/tests/y.rs` now");
    let texts: Vec<_> = c.iter().map(|(_, t)| t.as_str()).collect();
    assert!(texts.contains(&"src/foo.rs"), "{texts:?}");
    assert!(texts.contains(&"crates/x/tests/y.rs"), "{texts:?}");
    // dotted segments or an extension anywhere → still a path claim
    let c = kinds("see `docs/v1.2/notes` here");
    assert!(
        c.contains(&(ClaimKind::Path, "docs/v1.2/notes".into())),
        "{c:?}"
    );
}

#[test]
fn mimus_ignore_directives_suppress() {
    // whole-file opt-out → early return, no claims at all
    let c = kinds("<!-- mimus-ignore-file -->\nsee `dead/x.md` here");
    assert!(c.is_empty(), "{c:?}");
    // `mimus-ignore` alone on a line suppresses the NEXT line only
    let c = kinds(
        "<!-- mimus-ignore -->\nsee `dead/x.md` here\nbut `README.md` lives",
    );
    assert_eq!(c.len(), 1, "{c:?}");
    assert_eq!(c[0].1, "README.md", "{c:?}");
    // inline `mimus-ignore` with a backtick suppresses its own line,
    // leaves suppress_next unset
    let c = kinds("see `dead/x.md` <!-- mimus-ignore -->\nbut `README.md` lives");
    assert_eq!(c.len(), 1, "{c:?}");
    assert_eq!(c[0].1, "README.md", "{c:?}");
}

#[test]
fn placeholder_and_bare_extension_spans() {
    // whole-span `<name>` — a placeholder, not a generic type
    assert!(kinds("fill in `<name>` here").is_empty());
    // `.rs` alone — a bare extension/suffix, not a file
    assert!(kinds("every `.rs` file").is_empty());
    // `{name}` inside a path-ish span — a template placeholder
    assert!(kinds("see `docs/{name}.md` for it").is_empty());
}
