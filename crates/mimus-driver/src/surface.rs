//! Embedded surface dispatch — the driver serving its own engine's
//! ops through `pavo-driver-kit`'s surface layer, no second binary
//! needed. The `mcp` argv verb (feature-gated) plugs this
//! [`Surface`] impl into `surface::mcp::serve_stdio`.
//!
//! `op` reuses the same `cli.*` path wire queries take
//! (`ops::run_verb` re-execs `run` in the resolved dir — output
//! stays captured off the serve stream). `surface` dispatches
//! in-process: should mimus grow an unbounded verb it lands in
//! `ops::SURFACES` and argv goes straight to `mimus::dispatch`
//! with stdio streaming.

use pavo_driver_kit::surface::{OpOutcome, OpSpec, Surface};
use serde_json::{Map, Value};

use crate::ops;

/// mimus's [`Surface`] — engine dispatch, already linked.
pub struct Engine;

impl Surface for Engine {
    /// Same path as the `cli.<verb>` wire query: re-exec `run` in
    /// the resolved dir, captured output → `OpOutcome`.
    fn op(&mut self, op: &OpSpec, params: &Map<String, Value>) -> OpOutcome {
        OpOutcome::from_response(&ops::run_op(&op.name, params))
    }

    /// `mimus <name> <args>` in-process — for future unbounded
    /// surfaces (none today); bounded ops never reach here.
    fn surface(&mut self, name: &str, args: &[String]) -> i32 {
        let argv = std::iter::once("mimus".to_string())
            .chain(std::iter::once(name.to_string()))
            .chain(args.iter().cloned());
        mimus::dispatch(&mimus::parse_from(argv))
    }
}
