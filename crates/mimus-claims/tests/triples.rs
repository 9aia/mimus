//! triples: claims → (subject, relation, object) with provenance,
//! bucketed contradiction detection, duplicate-fingerprint pass —
//! the public API from outside the crate (ADR-0002 phase 2).

use mimus_claims::triples::{
    contradictions, contradictions_with_aliases, extract_triples, same_fingerprint,
    ContradictionKind,
};
use mimus_core::{Claim, ClaimKind};
use std::path::{Path, PathBuf};

fn claim(text: &str, src: &str, line: u32) -> Claim {
    Claim {
        kind: ClaimKind::Behavior,
        text: text.to_owned(),
        source: Path::new(src).to_path_buf(),
        line,
    }
}

#[test]
fn claims_to_triples_carry_provenance() {
    let claims = vec![
        claim("the auth service uses jwt for sessions", "docs/a.md", 3),
        claim("auth uses `tokens` for sessions", "docs/b.md", 7),
    ];
    let ts = extract_triples(&claims);
    assert_eq!(ts.len(), 2, "{ts:?}");
    assert_eq!(ts[0].relation, "uses");
    assert_eq!(ts[0].object, "jwt");
    assert_eq!(ts[0].source, PathBuf::from("docs/a.md"));
    assert_eq!(ts[0].line, 3);
    // non-prose claim kinds simply yield nothing
    let claims = vec![claim("cargo test -p foo", "d.md", 1)];
    assert!(extract_triples(&claims).is_empty());
    // empty input
    assert!(extract_triples(&[]).is_empty());
}

#[test]
fn cross_doc_and_reflexive_contradictions() {
    // doc↔doc: two files, same slot, different objects
    let claims = vec![
        claim("auth service uses jwt", "a.md", 1),
        claim("the auth module uses tokens", "b.md", 2),
        claim("worker pool runs 4 workers", "a.md", 5),
        claim("worker pool runs 8 workers", "c.md", 5),
    ];
    let cs = contradictions(&extract_triples(&claims));
    assert_eq!(cs.len(), 2, "{cs:?}");
    // sorted bucket keys — deterministic output
    assert_eq!(cs[0].bucket, "auth|uses");
    assert_eq!(cs[1].bucket, "worker|count");
    assert!(cs.iter().all(|c| c.kind == ContradictionKind::CrossSource));
    // reflexive: one card asserts two locations for the same thing
    let claims = vec![
        claim("auth lives in `src/auth.rs`", "card.yaml", 1),
        claim("`other/auth.rs` owns auth", "card.yaml", 2),
    ];
    let cs = contradictions(&extract_triples(&claims));
    assert_eq!(cs.len(), 1, "{cs:?}");
    assert_eq!(cs[0].kind, ContradictionKind::Reflexive);
    assert_eq!(cs[0].bucket, "auth|located_in");
    let objs: Vec<&str> = cs[0].objects.iter().map(|(o, _, _)| o.as_str()).collect();
    assert_eq!(objs, vec!["src/auth.rs", "other/auth.rs"]);
}

#[test]
fn aliases_unify_subject_buckets() {
    let claims = vec![
        claim("billing svc requires postgres", "a.md", 1),
        claim("the billing service requires sqlite", "b.md", 2),
    ];
    let ts = extract_triples(&claims);
    assert!(contradictions(&ts).is_empty(), "{ts:?}");
    let aliases = vec![("billing svc".to_owned(), "billing".to_owned())];
    let cs = contradictions_with_aliases(&ts, &aliases);
    assert_eq!(cs.len(), 1, "{cs:?}");
    assert_eq!(cs[0].bucket, "billing|requires");
}

#[test]
fn fingerprint_candidates() {
    // synthetic ids standing in for phase-1 shape fingerprints
    let groups = vec![
        ("shape-aaa", "crates/x/src/a.rs::parse"),
        ("shape-aaa", "crates/y/src/b.rs::parse"),
        ("shape-bbb", "crates/x/src/c.rs::render"),
        ("shape-bbb", "crates/x/src/c.rs::render"), // dup location
    ];
    let dups = same_fingerprint(&groups);
    assert_eq!(dups.len(), 1, "{dups:?}");
    assert_eq!(dups[0].fingerprint, "shape-aaa");
    assert_eq!(
        dups[0].locations,
        vec!["crates/x/src/a.rs::parse", "crates/y/src/b.rs::parse"]
    );
    // works over owned types too — PathBuf/u32 locations
    let groups = vec![
        (1u64, (PathBuf::from("a.rs"), 10usize)),
        (1u64, (PathBuf::from("b.rs"), 20usize)),
    ];
    let dups = same_fingerprint(&groups);
    assert_eq!(dups[0].locations.len(), 2);
}
