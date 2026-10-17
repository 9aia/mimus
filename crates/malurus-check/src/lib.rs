//! Check port — verify a claim against reality.
//! Impls: `malurus-check-fs` (paths), `malurus-check-cmd`
//! (commands), `malurus-check-symbol` (kuda index), `check-llm`
//! (behavior prose — semantic tier, advisory). New checker =
//! new impl crate.

use malurus_core::{Claim, ClaimKind, Violation};

/// A verifier for some subset of `ClaimKind`s.
pub trait Checker: Send + Sync {
    /// The claim kinds this checker can prove or refute.
    fn handles(&self) -> &[ClaimKind];
    /// `Ok(Some(v))` = claim refuted with evidence.
    /// `Ok(None)` = verified — OR unverifiable. Evidence-or-silence:
    /// a checker that can't prove the claim false emits nothing.
    fn check(&self, claim: &Claim) -> Result<Option<Violation>, String>;
}

/// Route claims to the checkers that handle their kind.
pub fn run<'a>(
    checkers: &'a [Box<dyn Checker>],
    claims: &'a [Claim],
) -> impl Iterator<Item = Result<Violation, String>> + 'a {
    claims.iter().filter_map(move |c| {
        checkers
            .iter()
            .find(|ch| ch.handles().contains(&c.kind))
            .map(|ch| ch.check(c))
            .and_then(|r| r.transpose())
    })
}
