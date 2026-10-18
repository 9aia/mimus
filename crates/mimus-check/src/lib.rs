//! Check port — verify a claim against reality.
//! Impls: `mimus-check-fs` (paths), `mimus-check-cmd`
//! (commands), `mimus-check-symbol` (kuda index), `check-llm`
//! (behavior prose — semantic tier, advisory). New checker =
//! new adapter crate.

use mimus_core::{Claim, ClaimKind, Violation};

/// A verifier for some subset of `ClaimKind`s.
pub trait Checker: Send + Sync {
    /// The claim kinds this checker can prove or refute.
    fn handles(&self) -> &[ClaimKind];
    /// `Ok(Some(v))` = claim refuted with evidence.
    /// `Ok(None)` = verified — OR unverifiable. Evidence-or-silence:
    /// a checker that can't prove the claim false emits nothing.
    fn check(&self, claim: &Claim) -> Result<Option<Violation>, String>;

    /// Batch path — default is per-claim. Checkers that talk to a
    /// rate-limited backend (ACP agents) override this to fold many
    /// claims into one prompt.
    fn check_batch(&self, claims: &[Claim]) -> Vec<Result<Option<Violation>, String>> {
        claims.iter().map(|c| self.check(c)).collect()
    }
}

/// Route claims to the checkers that handle their kind.
pub fn run<'a>(
    checkers: &'a [Box<dyn Checker>],
    claims: &'a [Claim],
) -> impl Iterator<Item = Result<Violation, String>> + 'a {
    // group claims by the first checker that handles each kind,
    // then batch through it — one claim still works
    let mut grouped: Vec<(usize, Vec<Claim>)> = Vec::new();
    for c in claims {
        if let Some(i) = checkers
            .iter()
            .position(|ch| ch.handles().contains(&c.kind))
        {
            match grouped.iter_mut().find(|(g, _)| *g == i) {
                Some((_, v)) => v.push(c.clone()),
                None => grouped.push((i, vec![c.clone()])),
            }
        }
    }
    grouped
        .into_iter()
        .flat_map(move |(i, cs)| {
            checkers[i]
                .check_batch(&cs)
                .into_iter()
                .filter_map(|r| r.transpose())
        })
}
