//! Engine ops — `cli.<verb>` wire queries dispatch into the `mimus`
//! engine lib through this binary's own `run` verb: each call
//! re-execs `tamias-driver-mimus run <argv>` with the request's
//! resolved dir as cwd, so `mimus::dispatch` writes to the child's
//! captured stdout and never touches the serve loop's ndjson stream
//! — and every mutation keeps the engine's own write-path
//! semantics (config search, `.mimus/` cache, claim rewrites).
//!
//! `ops` answers the ops manifest as JSON — *derived from the
//! engine's own clap tree* ([`mimus::command`]), so a verb can't
//! exist on the CLI and be missing here: declared args become
//! typed params (`arg`/`opt`/`flag`), doc strings ride along
//! unchanged.
//!
//! mimus has no unbounded verbs today — `surfaces` is empty by
//! design, kept so a future streaming verb (watch mode, a serve
//! loop) has an obvious home: surfaces never return, so they ride
//! `run` passthrough, never request/response ops. Streaming
//! *flags* would join `SKIP_PARAMS` for the same reason.
//!
//! `mutates` is honest: `check`/`report` write `.mimus/triples.db`
//! (the prose-triple cache), `fix` rewrites claim text, `init`
//! scaffolds `mimus.toml` + `rules/` — everything else only reads.

use clap::{Arg, ArgAction, Command};
use pavo_protocol::wire::{QueryResult, Response};
use serde_json::{json, Map, Value};

/// Unbounded surfaces — verbs that never return, so they can
/// never be request/response ops. None today.
const SURFACES: &[&str] = &[];

/// Params excluded from ops — flags that turn a bounded verb into
/// a stream. None today.
const SKIP_PARAMS: &[&str] = &[];

/// Read-only verbs — `init` writes config, `check`/`report` write
/// the `.mimus/` triples cache, `fix` rewrites claims. Mutators
/// belong behind a deliberate trigger, not ambient view polling.
const READ_ONLY: &[&str] = &["audit", "claims", "coverage", "doctor"];

/// `(name, clap spec)` for every leaf verb. Hidden plumbing
/// (`gen-cli-docs`) is a docs-pipeline verb, not an engine op —
/// it stays reachable through `run` passthrough.
fn verbs() -> Vec<(String, Command)> {
    fn walk(prefix: &str, cmd: &Command, out: &mut Vec<(String, Command)>) {
        for sub in cmd.get_subcommands() {
            let name = sub.get_name();
            if name == "help" || sub.is_hide_set() {
                continue;
            }
            let dotted = match prefix {
                "" => name.to_string(),
                _ => format!("{prefix}.{name}"),
            };
            if sub.has_subcommands() {
                walk(&dotted, sub, out);
            } else {
                out.push((dotted, sub.clone()));
            }
        }
    }
    let mut out = Vec::new();
    walk("", &mimus::command(), &mut out);
    out
}

fn op_cmd(name: &str) -> Option<Command> {
    verbs()
        .into_iter()
        .find(|(n, _)| n == name && !SURFACES.contains(&n.as_str()))
        .map(|(_, c)| c)
}

/// Args that can't ride the wire: streaming flags (above), the
/// driver-resolved `--config` global, and clap's own help/version
/// actions.
fn skipped(a: &Arg) -> bool {
    SKIP_PARAMS.contains(&a.get_id().as_str())
        || a.get_id() == "config"
        || matches!(
            a.get_action(),
            ArgAction::Help | ArgAction::HelpShort | ArgAction::HelpLong | ArgAction::Version
        )
}

/// A clap arg → the manifest's typed param kind — `arg` =
/// positional, `opt` = `--name <v>`, `flag` = `--name` — the
/// contract `pavo-driver-kit`'s surface layer renders args from.
fn kind(a: &Arg) -> &'static str {
    if a.is_positional() {
        "arg"
    } else if matches!(
        a.get_action(),
        ArgAction::SetTrue | ArgAction::SetFalse | ArgAction::Count
    ) {
        "flag"
    } else {
        "opt"
    }
}

fn param_json(a: &Arg) -> Value {
    // Wire name = the flag name (`--no-cmd`), not the field id
    // (`no_cmd`) — generated surfaces spell params `--<name>`.
    let id = a.get_id().as_str();
    let name = match a.is_positional() {
        true => id,
        false => a.get_long().unwrap_or(id),
    };
    json!({
        "name": name,
        "kind": kind(a),
        "doc": a.get_help().map(|d| d.to_string()).unwrap_or_default(),
    })
}

/// The `ops` query/`ops` argv verb payload — self-describing so
/// hosts and agents discover the engine surface without argv help.
/// `dir`/`root`/`repo`/`config` are the driver's globals —
/// accepted by every op, listed once on the manifest itself.
pub fn manifest() -> Value {
    json!({
        "engine": "mimus",
        "invoke": "query `cli.<verb>` with the listed params, or argv `tamias-driver-mimus run <mimus argv>`",
        "globals": [
            {"name": "dir|root|repo", "kind": "opt", "doc": "Search dir the config search starts from — same resolution the view queries use (param → $MIMUS_CONFIG → <dir>/mimus.toml → ~/.config/mimus/); the repo root is the config's parent dir."},
            {"name": "config", "kind": "opt", "doc": "mimus.toml path — the driver's own chain resolves it and hands the child an absolute --config."},
            {"name": "format", "kind": "opt", "doc": "Engine output format — human | json (the CLI's --format global)."},
        ],
        "ops": verbs().iter()
            .filter(|(n, _)| !SURFACES.contains(&n.as_str()))
            .map(|(n, c)| json!({
                "query": format!("cli.{n}"),
                "doc": c.get_about().map(|d| d.to_string()).unwrap_or_default(),
                "params": c.get_arguments()
                    .filter(|a| !skipped(a))
                    .map(param_json)
                    .collect::<Vec<_>>(),
                "mutates": !READ_ONLY.contains(&n.as_str()),
            }))
            .collect::<Vec<_>>(),
        "surfaces": SURFACES,
    })
}

/// Is `query` an engine op (`cli.<verb>`)? Returns the verb name.
pub fn op_of(query: &str) -> Option<String> {
    query
        .strip_prefix("cli.")
        .filter(|n| op_cmd(n).is_some())
        .map(str::to_string)
}

/// A wire scalar → its argv token (numbers/bools stringify).
fn scalar(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// All values of a param — arrays flatten, scalars wrap.
fn values(v: &Value) -> Vec<String> {
    match v {
        Value::Array(xs) => xs.iter().filter_map(scalar).collect(),
        other => scalar(other).into_iter().collect(),
    }
}

/// `cli.<verb>` params → mimus argv (no argv[0] — `run` prepends
/// it). Globals are resolved by [`run_verb`] (`--config` prepends
/// here; the child cwd is the resolved dir, which is how the CLI
/// finds its repo — mimus has no `--repo` flag). The dotted verb
/// name splits into the group path; declared params map by kind —
/// positionals land last, in schema order.
fn argv(
    config: Option<&str>,
    name: &str,
    cmd: &Command,
    params: &Map<String, Value>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(c) = config {
        out.extend(["--config".into(), c.into()]);
    }
    // --format is a clap *global*: it never appears in a
    // subcommand's get_arguments(), so the param loop below can't
    // see it — map it by hand. Same for --config (resolved, above).
    if let Some(f) = params.get("format").and_then(scalar) {
        out.extend(["--format".into(), f]);
    }
    out.extend(name.split('.').map(String::from));
    for a in cmd.get_arguments() {
        if skipped(a) || a.is_positional() {
            continue;
        }
        let id = a.get_id().as_str();
        let long = a.get_long().unwrap_or(id);
        let raw = params
            .get(long)
            .or_else(|| params.get(id))
            .or_else(|| params.get(id.replace('_', "-").as_str()));
        let Some(raw) = raw else { continue };
        match kind(a) {
            "flag" => {
                if raw.as_bool().unwrap_or(false) {
                    out.push(format!("--{long}"));
                }
            }
            _ => {
                for s in values(raw) {
                    out.push(format!("--{long}"));
                    out.push(s);
                }
            }
        }
    }
    let mut positionals: Vec<String> = Vec::new();
    for a in cmd.get_positionals() {
        if skipped(a) {
            continue;
        }
        let id = a.get_id().as_str();
        if let Some(raw) = params
            .get(id)
            .or_else(|| params.get(id.replace('_', "-").as_str()))
        {
            positionals.extend(values(raw));
        }
    }
    out.extend(positionals);
    out
}

/// The bare-name entry the embedded surfaces use (`OpSpec.name` is
/// `cli.`-stripped); the wire path goes `op_of` → `run_verb`.
#[cfg(feature = "mcp")]
pub fn run_op(op: &str, params: &Map<String, Value>) -> Response {
    match op_of(&format!("cli.{}", op.strip_prefix("cli.").unwrap_or(op))) {
        Some(name) => run_verb(&name, params),
        None => Response::err(format!("unknown op: {op}")),
    }
}

/// Run one engine op by name: re-exec `run` in a child whose cwd is
/// the request's resolved dir (same resolution the views use), and
/// hand back `{op, argv, code, stdout, stderr}` as the query
/// payload. `ok:false` is reserved for spawn failures — a nonzero
/// `code` is the engine's own exit status (e.g. `check` findings),
/// honest data.
pub fn run_verb(name: &str, params: &Map<String, Value>) -> Response {
    let Some(cmd) = op_cmd(name) else {
        return Response::err(format!("unknown op: {name}"));
    };
    let (dir, config) = crate::data::resolve(params);
    let args = argv(config.as_deref(), name, &cmd, params);
    let out = std::env::current_exe().and_then(|exe| {
        std::process::Command::new(exe)
            .arg("run")
            .args(&args)
            .current_dir(&dir)
            .output()
    });
    let data = match out {
        Ok(o) => json!({
            "op": name,
            "argv": args,
            "code": o.status.code().unwrap_or(-1),
            "stdout": String::from_utf8_lossy(&o.stdout),
            "stderr": String::from_utf8_lossy(&o.stderr),
        }),
        Err(e) => return Response::err(format!("run {name}: {e}")),
    };
    Response::ok_typed(&QueryResult {
        query: format!("cli.{name}"),
        data,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use serde_json::json;

    #[test]
    fn op_of_accepts_cli_prefix_only() {
        assert_eq!(op_of("cli.audit").unwrap(), "audit");
        assert_eq!(op_of("cli.check").unwrap(), "check");
        assert!(op_of("cli.gen-cli-docs").is_none()); // hidden plumbing
        assert!(op_of("audit").is_none()); // view query, not an op
        assert!(op_of("cli.bogus").is_none());
    }

    #[test]
    fn manifest_lists_every_leaf_as_cli_query() {
        let m = manifest();
        let ops = m["ops"].as_array().unwrap();
        for (n, _) in verbs()
            .iter()
            .filter(|(n, _)| !SURFACES.contains(&n.as_str()))
        {
            assert!(
                ops.iter().any(|o| o["query"] == json!(format!("cli.{n}"))),
                "{n} missing from ops manifest"
            );
        }
        assert_eq!(m["surfaces"], json!(SURFACES));
        for o in ops {
            for p in o["params"].as_array().unwrap() {
                assert!(["arg", "opt", "flag"].contains(&p["kind"].as_str().unwrap()));
                // streaming flags never ride the wire
                assert!(!SKIP_PARAMS.contains(&p["name"].as_str().unwrap()));
                // --config is a driver-resolved global, never a per-op param
                assert_ne!(p["name"], json!("config"));
            }
        }
        // mutates honesty — the write set is exactly these
        let mutators: Vec<&str> = ops
            .iter()
            .filter(|o| o["mutates"] == json!(true))
            .filter_map(|o| o["query"].as_str())
            .collect();
        assert_eq!(
            mutators,
            vec!["cli.init", "cli.check", "cli.fix", "cli.report"]
        );
    }

    #[test]
    fn argv_maps_params_to_mimus_flags() {
        let cmd = op_cmd("check").unwrap();
        let mut p = Map::new();
        p.insert("all".into(), json!(true));
        p.insert("no-cmd".into(), json!(true));
        p.insert("diff".into(), json!("origin/main...HEAD"));
        assert_eq!(
            argv(Some("/cfg/mimus.toml"), "check", &cmd, &p),
            vec![
                "--config",
                "/cfg/mimus.toml",
                "check",
                "--all",
                "--no-cmd",
                "--diff",
                "origin/main...HEAD",
            ]
        );
    }

    #[test]
    fn argv_omits_config_when_unresolved() {
        let cmd = op_cmd("fix").unwrap();
        let mut p = Map::new();
        p.insert("dry".into(), json!(true));
        assert_eq!(argv(None, "fix", &cmd, &p), vec!["fix", "--dry"]);
    }
}
