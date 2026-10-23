//! extract_comments: assertive code-comment lines become Behavior
//! claims; the marker/caps are exercised in the lib's own tests.

use mimus_claims::extract_comments;
use std::path::Path;

#[test]
fn comment_assertions_become_behavior_claims() {
    let src = "// The store uses sqlite for persistence\nlet x = 1;\n";
    let claims = extract_comments(src, Path::new("src/s.rs"));
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].kind, mimus_core::ClaimKind::Behavior);
    assert!(claims[0].text.contains("sqlite"));
}

#[test]
fn non_assertive_comments_skip() {
    let src = "// hi\n// fixme:\n";
    assert!(extract_comments(src, Path::new("s.rs")).is_empty());
}
