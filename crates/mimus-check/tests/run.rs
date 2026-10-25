//! Tests for the checker router: kind dispatch, batching, silence.

use mimus_check::{run, Checker};
use mimus_core::{Claim, ClaimKind, Severity, Violation};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

type Calls = Arc<Mutex<Vec<String>>>;

struct Fake {
    kinds: &'static [ClaimKind],
    calls: Calls,
}

impl Fake {
    fn new(kinds: &'static [ClaimKind]) -> (Self, Calls) {
        let calls = Calls::default();
        (
            Self {
                kinds,
                calls: calls.clone(),
            },
            calls,
        )
    }
}

impl Checker for Fake {
    fn handles(&self) -> &[ClaimKind] {
        self.kinds
    }
    fn check(&self, claim: &Claim) -> Result<Option<Violation>, String> {
        self.calls.lock().unwrap().push(claim.text.clone());
        Ok(claim.text.starts_with("bad").then(|| Violation {
            claim: claim.clone(),
            evidence: "proof".into(),
            fix: Some("fix it".into()),
            severity: Severity::Warn,
        }))
    }
}

fn claim(kind: ClaimKind, text: &str) -> Claim {
    Claim {
        kind,
        text: text.into(),
        source: PathBuf::from("d"),
        line: 1,
    }
}

#[test]
fn routes_to_first_matching_checker() {
    let (a, calls_a) = Fake::new(&[ClaimKind::Path]);
    let (b, calls_b) = Fake::new(&[ClaimKind::Path, ClaimKind::Command]);
    let checkers: Vec<Box<dyn Checker>> = vec![Box::new(a), Box::new(b)];
    let claims = vec![
        claim(ClaimKind::Path, "bad/path"),
        claim(ClaimKind::Command, "bad cmd"),
        claim(ClaimKind::Link, "https://x"), // unhandled → dropped
        claim(ClaimKind::Path, "fine/path"),
    ];
    let out: Vec<_> = run(&checkers, &claims).collect();
    assert_eq!(out.len(), 2);
    // 'a' handled Path claims (and only those); 'b' got the Command.
    assert_eq!(*calls_a.lock().unwrap(), vec!["bad/path", "fine/path"]);
    assert_eq!(*calls_b.lock().unwrap(), vec!["bad cmd"]);
    // Unhandled kind produced nothing.
    assert!(!out
        .iter()
        .any(|r| r.as_ref().unwrap().claim.text.contains("http")));
}

#[test]
fn silence_and_errors_propagate() {
    struct Erring;
    impl Checker for Erring {
        fn handles(&self) -> &[ClaimKind] {
            &[ClaimKind::Symbol]
        }
        fn check(&self, _c: &Claim) -> Result<Option<Violation>, String> {
            Err("backend down".into())
        }
    }
    let checkers: Vec<Box<dyn Checker>> = vec![Box::new(Erring)];
    let claims = vec![claim(ClaimKind::Symbol, "sym")];
    let out: Vec<_> = run(&checkers, &claims).collect();
    assert!(matches!(&out[0], Err(e) if e == "backend down"));
}
