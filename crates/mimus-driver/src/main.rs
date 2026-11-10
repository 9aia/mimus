//! tamias-driver-mimus — the pavo view driver for a repo's mimus
//! critic state.
//!
//! Verbs (the tamias driver contract):
//!
//! - `manifest` — print the [`DriverManifest`] JSON (discovery
//!   probe; never touches repo state).
//! - `capabilities` — `{"capabilities": ["pavo"], "verbs": […]}`.
//! - `ops` — the engine ops manifest: every `mimus` CLI verb as a
//!   `cli.<verb>` wire query, derived from the engine's own clap
//!   tree ([`mimus::command`]).
//! - `run <argv>` — engine passthrough: `tamias-driver-mimus run
//!   audit` ≡ `mimus audit`. Also the child side of `cli.*` wire
//!   queries (`ops::run_verb` re-execs this same verb so engine
//!   output never touches the serve stream).
//! - `serve` — ndjson JSON-RPC on stdio: `{"op":"manifest"}`,
//!   `{"op":"views"}`, `{"op":"view","view":…,"params":{…}}`,
//!   `{"op":"query","query":…,"params":{…}}` →
//!   `{"ok":true,"result":…}` / `{"ok":false,"error":…}`.
//!   `query:"ops"` answers the ops manifest; `query:"cli.<verb>"`
//!   runs the engine op.
//!
//! Optional surfaces: features `cli`/`mcp` embed
//! pavo-driver-kit's surface layer — `cli <op> [args]` is a
//! generated CLI over the ops manifest, `mcp` serves the ops as
//! MCP tools on stdio. Both are equivalent to the standalone
//! `tamias-driver-cli`/`tamias-driver-mcp` surface drivers, minus
//! the PATH/discovery hop.
//!
//! Which repo: each `view`/`query` resolves its own target —
//! `params.dir`/`params.root`/`params.repo`/`params.config` →
//! `$MIMUS_CONFIG` → `<dir>/mimus.toml` → `~/.config/mimus/mimus.toml`,
//! exactly the `mimus` CLI's config search; the repo root is the
//! config's parent dir. See `data.rs` for the exact order. Engine
//! ops are the exception by design: `cli.<verb>` re-execs `run` in
//! the resolved dir and lets `mimus::dispatch` own the outcome —
//! writes keep the engine's own semantics (config search,
//! `.mimus/` cache, claim rewrites). `manifest`/`views` are
//! repo-free so pavo can enumerate drivers anywhere.
//!
//! The view side is **read-only**: it reads `mimus.toml`,
//! `rules/*.toml`, and `.kuda` card provenance in-process, and
//! shells out to `mimus report --format json` for the violations
//! view — mimus owns the check pipeline; the driver only ever
//! reads its output.

mod data;
mod ops;
#[cfg(any(feature = "cli", feature = "mcp"))]
mod surface;
mod views;

use pavo_driver_kit as kit;
use pavo_protocol::wire::{Request, Response, ViewSummary};
use serde_json::{json, Value};

fn capabilities_json() -> Value {
    let m = views::manifest();
    // The kit's shape plus the view count every sibling driver
    // advertises.
    let mut v = kit::capabilities_value(&m);
    v["views"] = json!(m.views.len());
    v
}

/// One parsed wire [`Request`] → response. The kit's serve loop owns
/// the ndjson plumbing and turns parse failures into `ok:false`;
/// ops this driver doesn't implement (`render`, `submit`, `jobs`,
/// `notifications`) get the same `ok:false` convention every
/// non-provider driver answers.
fn dispatch(req: &Request) -> Response {
    match req {
        Request::Manifest => Response::ok_typed(&views::manifest()),
        Request::Views => {
            let summaries: Vec<ViewSummary> =
                views::specs().iter().map(ViewSummary::from).collect();
            Response::ok_typed(&summaries)
        }
        Request::View { view, params } => match views::view_result(view, params) {
            Ok(result) => Response::ok_typed(&result),
            Err(e) => Response::err(e),
        },
        Request::Query { query, params } => {
            // `ops` and `cli.<verb>` are engine ops; everything
            // else is a view query — unknown names fail the op,
            // degraded snapshots travel as `{"error": …}` data
            // (the marker renderers understand).
            if query == "ops" {
                Response::ok_typed(&pavo_protocol::wire::QueryResult {
                    query: query.clone(),
                    data: ops::manifest(),
                })
            } else if let Some(v) = ops::op_of(query) {
                ops::run_verb(&v, params)
            } else if views::known_query(query) {
                let snap = data::load(params);
                let payload = views::query_data(query, params, &snap);
                Response::ok_typed(&pavo_protocol::wire::QueryResult {
                    query: query.clone(),
                    data: payload,
                })
            } else {
                Response::err(format!("unknown query: {query}"))
            }
        }
        // `render` — this driver ships no renderer plugins; hosts
        // only call it for kinds the manifest claims, so answering
        // an explicit error keeps the wire honest.
        Request::Render { kind, .. } => Response::err(format!(
            "tamias-driver-mimus claims no renderers (kind '{kind}')"
        )),
        // The protocol grew `submit`/`jobs`/`notifications` ops after
        // this driver was written; mutations go through `cli.<verb>`,
        // never `submit`, so they answer `ok:false` like any
        // unsupported op.
        Request::Submit { .. } | Request::Jobs | Request::Notifications => {
            Response::err("tamias-driver-mimus implements no submit/jobs/notifications ops")
        }
    }
}

fn serve() -> i32 {
    // The kit owns the ndjson loop — stdin EOF and a dead client
    // both end it quietly, and either way the driver exits 0.
    let _ = kit::serve_stdio(dispatch);
    0
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("manifest") => {
            println!("{}", kit::manifest_json(&views::manifest()));
            0
        }
        Some("capabilities") => {
            println!(
                "{}",
                serde_json::to_string_pretty(&capabilities_json()).unwrap_or_default()
            );
            0
        }
        Some("ops") => {
            println!("{}", ops::manifest());
            0
        }
        // Engine passthrough — `tamias-driver-mimus run audit` is
        // `mimus audit`. Also the child side of `cli.*` wire
        // queries (ops::run_verb re-execs this same verb).
        Some("run") => {
            let argv = std::iter::once("mimus".to_string()).chain(args.iter().skip(1).cloned());
            std::process::exit(mimus::run_from(argv));
        }
        // Embedded surface verbs (feature-gated, optional): the
        // pavo-driver-kit surface layer renders this driver's own
        // ops manifest — `cli` = generated clap CLI (op →
        // subcommand), `mcp` = ops → MCP tools over stdio. The
        // standalone tamias-driver-cli/-mcp binaries do the same
        // over process dispatch; these verbs need nothing on PATH.
        #[cfg(feature = "cli")]
        Some("cli") => {
            let m = kit::surface::OpsManifest::from_value(&ops::manifest()).unwrap_or_default();
            let argv: Vec<String> = std::iter::once("mimus".to_string())
                .chain(args.iter().skip(1).cloned())
                .collect();
            kit::surface::cli::run("mimus", &m, &argv, &mut surface::Engine)
        }
        #[cfg(feature = "mcp")]
        Some("mcp") => {
            let m = kit::surface::OpsManifest::from_value(&ops::manifest()).unwrap_or_default();
            match kit::surface::mcp::serve_stdio(&m, &mut surface::Engine) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("tamias-driver-mimus mcp: {e}");
                    1
                }
            }
        }
        Some("serve") | None => serve(),
        _ => {
            eprintln!(
                "usage: tamias-driver-mimus manifest|capabilities|ops|run <argv>|serve\n       (feature `cli` adds `cli <op> [args]`; feature `mcp` adds `mcp`)"
            );
            1
        }
    };
    std::process::exit(code);
}
