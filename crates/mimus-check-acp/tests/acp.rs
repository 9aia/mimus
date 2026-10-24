//! AcpChecker through a fake ACP agent — a python stub speaking
//! JSON-RPC over stdio: initialize/session-new handshake, streamed
//! session/update chunks, agent→client requests (permission +
//! unknown method), error responses, and a queue file of canned
//! prompt answers (one line per `session/prompt`; `\n` escapes;
//! `NORESULT` = silent success; `ERROR` = JSON-RPC error; missing
//! or exhausted queue = the agent dies mid-request).

use mimus_check::Checker;
use mimus_check_acp::{probe, propose, AcpChecker};
use mimus_core::{Claim, ClaimKind, Severity};
use std::path::PathBuf;

const STUB: &str = r#"import json, sys

queue = sys.argv[1]

def send(o):
    sys.stdout.write(json.dumps(o) + "\n")
    sys.stdout.flush()

def pop():
    try:
        data = open(queue).read()
    except OSError:
        sys.exit(0)
    if not data.strip():
        sys.exit(0)
    line, _, rest = data.partition("\n")
    open(queue, "w").write(rest)
    return line.replace("\\n", "\n")

for line in sys.stdin:
    try:
        msg = json.loads(line)
    except ValueError:
        continue
    mid, method = msg.get("id"), msg.get("method")
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": mid, "result": {"protocolVersion": 1}})
    elif method == "session/new":
        send({"jsonrpc": "2.0", "id": mid, "result": {"sessionId": "s1"}})
    elif method == "session/prompt":
        send({"jsonrpc": "2.0", "id": 900 + (mid or 0), "method": "session/request_permission",
              "params": {"options": [{"optionId": "proceed_once"}]}})
        send({"jsonrpc": "2.0", "id": 903 + (mid or 0), "method": "session/request_permission",
              "params": {"options": []}})  # no options → "allow_once" fallback
        send({"jsonrpc": "2.0", "id": 901 + (mid or 0), "method": "fs/read_text_file", "params": {}})
        send({"jsonrpc": "2.0", "id": 902 + (mid or 0), "method": "agent/no_such_method"})
        send({"jsonrpc": "2.0", "id": 4242, "result": {}})  # stray response — id never matches
        send({"jsonrpc": "2.0", "method": "session/update",
              "params": {"update": {"content": [{"text": ""}, {"type": "image"}]}}})
        send({"jsonrpc": "2.0", "method": "session/update",
              "params": {"update": {"content": "a bare string, not text-or-blocks"}}})
        sys.stdout.write("not json\n")
        sys.stdout.flush()
        ans = pop()
        if ans == "ERROR":
            send({"jsonrpc": "2.0", "id": mid, "error": {"code": -1, "message": "boom"}})
            continue
        if ans != "NORESULT":
            send({"jsonrpc": "2.0", "method": "session/update",
                  "params": {"update": {"content": {"text": ans}}}})
        send({"jsonrpc": "2.0", "id": mid, "result": {}})
"#;

/// A *tool-enabled* agent: mid-prompt it issues a
/// `session/request_permission` with a STRING id (the shape that
/// deadlocked the old client — `as_u64()` never matched it) and an
/// `fs/read_text_file` it expects the client to serve. It only
/// answers `VERIFIED` if permission came back "allow"; a wrong or
/// absent answer wedges it exactly like a real agent.
const TOOL_STUB: &str = r#"import json, sys

def send(o):
    sys.stdout.write(json.dumps(o) + "\n")
    sys.stdout.flush()

def read_msg():
    line = sys.stdin.readline()
    if not line:
        sys.exit(0)
    return json.loads(line)

while True:
    msg = read_msg()
    mid, method = msg.get("id"), msg.get("method")
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": mid, "result": {"protocolVersion": 1}})
    elif method == "session/new":
        send({"jsonrpc": "2.0", "id": mid, "result": {"sessionId": "s1"}})
    elif method == "session/prompt":
        send({"jsonrpc": "2.0", "id": "perm-1", "method": "session/request_permission",
              "params": {"options": [{"optionId": "deny", "kind": "reject_once"},
                                     {"optionId": "go", "kind": "allow_once"}]}})
        ans = read_msg()
        if ans.get("id") != "perm-1":
            sys.exit(0)  # nobody answered the permission -> we stall
        if ans.get("result", {}).get("outcome", {}).get("optionId") != "go":
            sys.exit(0)  # a reject was picked — nothing more to say
        send({"jsonrpc": "2.0", "id": "fs-1", "method": "fs/read_text_file",
              "params": {"sessionId": "s1", "path": "doc.md"}})
        ans = read_msg()
        content = ans.get("result", {}).get("content", "")
        verdict = "REFUTED: doc.md contradicts the claim" if content else "VERIFIED"
        send({"jsonrpc": "2.0", "method": "session/update",
              "params": {"update": {"content": {"text": verdict}}}})
        send({"jsonrpc": "2.0", "id": mid, "result": {"stopReason": "end_turn"}})
"#;

/// Handshakes then goes silent on session/prompt — the blocked-agent
/// shape. Only the request timeout lets `check` return at all.
const SILENT_STUB: &str = r#"import json, sys, time

def send(o):
    sys.stdout.write(json.dumps(o) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    try:
        msg = json.loads(line)
    except ValueError:
        continue
    mid, method = msg.get("id"), msg.get("method")
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": mid, "result": {"protocolVersion": 1}})
    elif method == "session/new":
        send({"jsonrpc": "2.0", "id": mid, "result": {"sessionId": "s1"}})
    elif method == "session/prompt":
        time.sleep(600)  # userActionRequired — waits forever
"#;

/// `session/new` answers with no `sessionId` — the client must
/// fall back to its "mimus" default rather than choke.
const NOSID_STUB: &str = r#"import json, sys

def send(o):
    sys.stdout.write(json.dumps(o) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    try:
        msg = json.loads(line)
    except ValueError:
        continue
    mid, method = msg.get("id"), msg.get("method")
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": mid, "result": {"protocolVersion": 1}})
    elif method == "session/new":
        send({"jsonrpc": "2.0", "id": mid, "result": {}})
    elif method == "session/prompt":
        send({"jsonrpc": "2.0", "method": "session/update",
              "params": {"update": {"content": {"text": "VERIFIED"}}}})
        send({"jsonrpc": "2.0", "id": mid, "result": {}})
"#;

/// Emits a non-UTF-8 line mid-prompt — read_line fails, the pump
/// forwards the io error instead of a line.
const BADUTF8_STUB: &str = r#"import json, sys

def send(o):
    sys.stdout.write(json.dumps(o) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    try:
        msg = json.loads(line)
    except ValueError:
        continue
    mid, method = msg.get("id"), msg.get("method")
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": mid, "result": {"protocolVersion": 1}})
    elif method == "session/new":
        send({"jsonrpc": "2.0", "id": mid, "result": {"sessionId": "s1"}})
    elif method == "session/prompt":
        sys.stdout.buffer.write(b"\xff\xfe not utf8\n")
        sys.stdout.flush()
        send({"jsonrpc": "2.0", "id": mid, "result": {}})
"#;

/// Answers `initialize` then exits on `session/new` — the
/// handshake's second request fails.
const HALF_STUB: &str = r#"import json, sys

for line in sys.stdin:
    try:
        msg = json.loads(line)
    except ValueError:
        continue
    if msg.get("method") == "initialize":
        sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": msg.get("id"),
                                    "result": {"protocolVersion": 1}}) + "\n")
        sys.stdout.flush()
    elif msg.get("method") == "session/new":
        sys.exit(0)
"#;

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mimus-acp-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// `(cmd, queue_path)` — cmd runs the stub against `queue`'s file.
fn agent(dir: &PathBuf, answers: &[&str]) -> String {
    let stub = dir.join("agent.py");
    std::fs::write(&stub, STUB).unwrap();
    let queue = dir.join("queue");
    std::fs::write(&queue, answers.join("\n") + "\n").unwrap();
    format!("python3 {} {}", stub.display(), queue.display())
}

fn claim(text: &str, source: &str, line: u32) -> Claim {
    Claim {
        kind: ClaimKind::Behavior,
        text: text.into(),
        source: source.into(),
        line,
    }
}

#[test]
fn handles_and_dead_path_triage() {
    let dir = tmp("triage");
    // cmd `false` — if the agent were spawned it would fail; triage
    // must decide without it.
    let c = AcpChecker::new("false", &dir);
    assert_eq!(c.handles(), &[ClaimKind::Behavior]);
    let v = c
        .check(&claim(
            "see missing/dead-file.rs for the details",
            "doc.md",
            3,
        ))
        .unwrap()
        .unwrap();
    assert_eq!(v.severity, Severity::Warn);
    assert!(
        v.evidence.contains("missing/dead-file.rs"),
        "{}",
        v.evidence
    );
    // no dead token → the agent path runs → `false` dies → Err
    assert!(c
        .check(&claim("the engine is very fast indeed", "doc.md", 1))
        .is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_verdicts_and_cache() {
    let dir = tmp("verdicts");
    std::fs::write(dir.join("doc.md"), "x").unwrap();
    std::fs::create_dir_all(dir.join("real")).unwrap();
    std::fs::write(dir.join("real/file.rs"), "x").unwrap();
    let c = AcpChecker::new(
        agent(
            &dir,
            &[
                "VERIFIED",
                "REFUTED: doc.md says the opposite",
                "REFUTED: ghost/missing.md proves it",
                "REFUTED",
                "anything",
            ],
        ),
        &dir,
    );
    // VERIFIED → silence
    let a = claim("the engine is very fast indeed", "doc.md", 1);
    assert!(c.check(&a).unwrap().is_none());
    // same claim again → cache hit, no second prompt
    assert!(c.check(&a).unwrap().is_none());
    // grounded REFUTED → violation
    let b = claim("writes always go through the journal", "doc.md", 2);
    let v = c.check(&b).unwrap().unwrap();
    assert_eq!(v.severity, Severity::Warn);
    assert!(v.evidence.contains("doc.md"), "{}", v.evidence);
    // cached REFUTED → still a violation
    assert!(c.check(&b).unwrap().is_some());
    // ungrounded refute → silence; bare REFUTED → silence
    assert!(c
        .check(&claim("claim three is wrong", "doc.md", 3))
        .unwrap()
        .is_none());
    assert!(c
        .check(&claim("claim four is wrong too", "doc.md", 4))
        .unwrap()
        .is_none());
    // a token with '/' that exists under root → not a dead path → agent
    assert!(c
        .check(&claim("reads from real/file.rs today", "doc.md", 5))
        .unwrap()
        .is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verdicts_cache_file_loading() {
    // corrupt cache file → treated as empty
    let dir = tmp("corrupt");
    std::fs::create_dir_all(dir.join(".mimus")).unwrap();
    std::fs::write(dir.join(".mimus/verdicts.json"), "{not json").unwrap();
    let c = AcpChecker::new("false", &dir);
    assert!(c.check(&claim("x is y", "s.md", 1)).is_err());
    // valid cache file → loads; unknown keys just miss
    let dir = tmp("cached");
    std::fs::create_dir_all(dir.join(".mimus")).unwrap();
    std::fs::write(
        dir.join(".mimus/verdicts.json"),
        "{\"deadbeef\": \"VERIFIED\"}",
    )
    .unwrap();
    let c = AcpChecker::new("false", &dir);
    assert!(c.check(&claim("x is y", "s.md", 1)).is_err());
    let _ = std::fs::remove_dir_all(tmp("corrupt").as_path());
    let _ = std::fs::remove_dir_all(tmp("cached").as_path());
}

#[test]
fn batch_singles_and_early_return() {
    let dir = tmp("batch");
    std::fs::write(dir.join("doc.md"), "x").unwrap();
    let c = AcpChecker::new(agent(&dir, &["VERIFIED"]), &dir);
    // len <= 1 → delegates to check (triage decides here)
    let dead = claim("see dead/nothing-here.rs for it", "doc.md", 1);
    let out = c.check_batch(&[dead]);
    assert_eq!(out.len(), 1);
    assert!(out[0].as_ref().unwrap().is_some());
    // seed the cache with a VERIFIED claim, then a batch where every
    // claim resolves via triage or cache → no prompt at all
    let done = claim("the engine is very fast indeed", "doc.md", 2);
    assert!(c.check(&done).unwrap().is_none());
    let out = c.check_batch(&[claim("see dead/absent-thing.rs here", "doc.md", 3), done]);
    assert_eq!(out.len(), 2);
    assert!(out[0].as_ref().unwrap().is_some());
    assert!(out[1].as_ref().unwrap().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn batch_numbered_verdicts_and_clusters() {
    let dir = tmp("numbered");
    std::fs::write(dir.join("doc.md"), "x").unwrap();
    let c = AcpChecker::new(
        agent(
            &dir,
            &[
                "1 VERIFIED\\n2 REFUTED: doc.md contradicts it\\n3 UNVERIFIABLE\\n4 REFUTED: ghost/missing.md proves it",
                "NONE",
                "1 VERIFIED",
            ],
        ),
        &dir,
    );
    let claims = vec![
        claim("cache_value is stored in memory", "doc.md", 1),
        claim("cache_value resets on every boot", "doc.md", 2),
        claim("dup_token mentions dup_token twice", "doc.md", 3),
        claim("a fourth pending claim entirely", "doc.md", 4),
    ];
    let out = c.check_batch(&claims);
    assert_eq!(out.len(), 4);
    assert!(out[0].as_ref().unwrap().is_none()); // VERIFIED
    let v = out[1].as_ref().unwrap().as_ref().unwrap(); // grounded REFUTED
    assert!(v.evidence.contains("doc.md"), "{}", v.evidence);
    assert!(out[2].as_ref().unwrap().is_none()); // UNVERIFIABLE
    assert!(out[3].as_ref().unwrap().is_none()); // ungrounded REFUTED → silence
                                                 // self-cleaning: a batch whose key set excludes earlier cached
                                                 // claims drops them (rewrites verdicts.json); both claims here
                                                 // resolve via triage/cache → no prompt needed
    let out = c.check_batch(&[
        claims[0].clone(),
        claim("see dead/absent-thing.rs here", "doc.md", 9),
    ]);
    assert_eq!(out.len(), 2);
    assert!(out[0].as_ref().unwrap().is_none());
    assert!(out[1].as_ref().unwrap().is_some());
    // session reuse: a later batch with a fresh pending claim skips
    // connect entirely (the guard is already Some)
    let out = c.check_batch(&[
        claim("a fresh unverified claim appears", "doc.md", 11),
        claim("see dead/absent-thing.rs here", "doc.md", 12),
    ]);
    assert_eq!(out.len(), 2);
    assert!(out[0].as_ref().unwrap().is_none());
    assert!(out[1].as_ref().unwrap().is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn batch_contradiction_and_dead_agent() {
    let dir = tmp("contradict");
    std::fs::write(dir.join("doc.md"), "x").unwrap();
    let c = AcpChecker::new(
        agent(&dir, &["1 VERIFIED\\n2 VERIFIED", "1 vs 2: they disagree"]),
        &dir,
    );
    let claims = vec![
        claim("cache_value is stored in memory", "doc.md", 1),
        claim("cache_value resets on every boot", "doc.md", 2),
    ];
    let out = c.check_batch(&claims);
    // 2 verdicts + 1 appended contradiction citing the other claim
    assert_eq!(out.len(), 3);
    let v = out[2].as_ref().unwrap().as_ref().unwrap();
    assert!(
        v.evidence.contains("contradicts doc.md:2"),
        "{}",
        v.evidence
    );

    // cluster prompt with a dead agent → break, keep verdicts
    let dir = tmp("deadmid");
    std::fs::write(dir.join("doc.md"), "x").unwrap();
    let c = AcpChecker::new(agent(&dir, &["1 VERIFIED\\n2 VERIFIED"]), &dir);
    let out = c.check_batch(&[
        claim("cache_value is stored in memory", "doc.md", 1),
        claim("cache_value resets on every boot", "doc.md", 2),
    ]);
    assert_eq!(out.len(), 2);

    // prompt request fails (queue empty → agent exits) → Err
    let dir = tmp("promptfail");
    std::fs::write(dir.join("doc.md"), "x").unwrap();
    let c = AcpChecker::new(agent(&dir, &[]), &dir);
    let out = c.check_batch(&[
        claim("the engine is very fast indeed", "doc.md", 1),
        claim("the store is very durable too", "doc.md", 2),
    ]);
    assert_eq!(out.len(), 1);
    assert!(out[0].is_err());
    let _ = std::fs::remove_dir_all(tmp("contradict").as_path());
    let _ = std::fs::remove_dir_all(tmp("deadmid").as_path());
    let _ = std::fs::remove_dir_all(tmp("promptfail").as_path());
}

/// Every malformed `A vs B` shape is rejected — extra tokens,
/// non-numeric ids, out-of-range/zero indexes, empty what.
#[test]
fn vs_line_rejects_malformed_shapes() {
    let dir = tmp("vsmalformed");
    std::fs::write(dir.join("doc.md"), "x").unwrap();
    let c = AcpChecker::new(
        agent(
            &dir,
            &[
                "1 VERIFIED\\n2 VERIFIED",
                "1 vs 2 extra token: junk\\nx vs 2: badnum\\n1 vs z: badnum2\\nx: onetok\\n1 vs: twotok\\n9 vs 9: out of range\\n0 vs 1: zero idx\\n1 vs 2:\\n: blank head\\n1 vz 2: wrong middle\\nNONE",
            ],
        ),
        &dir,
    );
    let out = c.check_batch(&[
        claim("SharedThing stores the value safely", "doc.md", 1),
        claim("SharedThing resets on every boot now", "doc.md", 2),
    ]);
    assert_eq!(out.len(), 2, "{out:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// `session/new` with no `sessionId` → the "mimus" fallback; the
/// check still completes normally.
#[test]
fn session_without_id_falls_back() {
    let dir = tmp("nosid");
    let c = AcpChecker::new(stub_agent(&dir, NOSID_STUB), &dir);
    assert!(c
        .check(&claim("the engine is very fast indeed", "doc.md", 1))
        .unwrap()
        .is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Non-UTF-8 bytes on stdout → read_line fails → the io error
/// surfaces as the check error, not a hang or a panic.
#[test]
fn invalid_utf8_agent_output_errors() {
    let dir = tmp("badutf8");
    let c = AcpChecker::new(stub_agent(&dir, BADUTF8_STUB), &dir);
    assert!(c
        .check(&claim("the engine is very fast indeed", "doc.md", 1))
        .is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Agent dies answering `session/new` — connect fails there, the
/// error surfaces from check/probe.
#[test]
fn session_new_failure_errors() {
    let dir = tmp("halfhand");
    let cmd = stub_agent(&dir, HALF_STUB);
    assert!(probe(&cmd, &dir).is_err());
    let c = AcpChecker::new(cmd.clone(), &dir);
    assert!(c
        .check(&claim("the engine is very fast indeed", "doc.md", 1))
        .is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn batch_connect_failure() {
    let dir = tmp("connfail");
    let c = AcpChecker::new("false", &dir);
    let out = c.check_batch(&[
        claim("the engine is very fast indeed", "doc.md", 1),
        claim("the store is very durable too", "doc.md", 2),
    ]);
    assert_eq!(out.len(), 1);
    assert!(out[0].is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn error_response_propagates() {
    let dir = tmp("jrpcerr");
    std::fs::write(dir.join("doc.md"), "x").unwrap();
    let c = AcpChecker::new(agent(&dir, &["ERROR"]), &dir);
    let e = c
        .check(&claim("the engine is very fast indeed", "doc.md", 1))
        .unwrap_err();
    assert!(e.contains("boom"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn probe_and_propose() {
    // ok
    let dir = tmp("probeok");
    let cmd = agent(&dir, &["ok"]);
    let msg = probe(&cmd, &dir).unwrap();
    assert!(msg.contains("ok"), "{msg}");
    // answered but no text → error
    let dir = tmp("probeempty");
    let cmd = agent(&dir, &["NORESULT"]);
    assert!(probe(&cmd, &dir).unwrap_err().contains("no text"));
    // dead agent → error
    assert!(probe("false", &tmp("probedead")).is_err());
    // agent answers the prompt with a JSON-RPC error → propagates
    let dir = tmp("probejrpc");
    let cmd = agent(&dir, &["ERROR"]);
    assert!(probe(&cmd, &dir).unwrap_err().contains("boom"));

    // propose returns the agent's line
    let dir = tmp("proposeok");
    let cmd = agent(&dir, &["The corrected claim text"]);
    assert_eq!(
        propose(&cmd, &dir, "old claim", "evidence").unwrap(),
        "The corrected claim text"
    );
    // nothing proposed → error
    let dir = tmp("proposeempty");
    let cmd = agent(&dir, &["NORESULT"]);
    assert!(propose(&cmd, &dir, "old", "ev")
        .unwrap_err()
        .contains("proposed nothing"));
    // prompt request itself errors → Err propagates
    let dir = tmp("proposejrpc");
    let cmd = agent(&dir, &["ERROR"]);
    assert!(propose(&cmd, &dir, "old", "ev")
        .unwrap_err()
        .contains("boom"));
    // dead agent → connect fails before any prompt
    assert!(propose("false", &tmp("proposedead"), "old", "ev").is_err());
    let _ = std::fs::remove_dir_all(tmp("probeok").as_path());
    let _ = std::fs::remove_dir_all(tmp("probeempty").as_path());
    let _ = std::fs::remove_dir_all(tmp("probedead").as_path());
    let _ = std::fs::remove_dir_all(tmp("proposeok").as_path());
    let _ = std::fs::remove_dir_all(tmp("proposeempty").as_path());
}

fn stub_agent(dir: &PathBuf, stub: &str) -> String {
    let path = dir.join("agent.py");
    std::fs::write(&path, stub).unwrap();
    format!("python3 {}", path.display())
}

/// A tool-enabled agent that permissions + fs-reads mid-prompt
/// completes — this used to deadlock `mimus report` forever.
#[test]
fn tool_enabled_agent_completes() {
    let dir = tmp("toolagent");
    std::fs::write(dir.join("doc.md"), "the doc says otherwise").unwrap();
    let c = AcpChecker::new(stub_agent(&dir, TOOL_STUB), &dir);
    let v = c
        .check(&claim("the engine is very fast indeed", "doc.md", 1))
        .unwrap()
        .expect("agent grounded its refute in doc.md");
    assert!(v.evidence.contains("doc.md"), "{}", v.evidence);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A blocked agent (silent on prompt) degrades to an Err inside the
/// request timeout instead of hanging forever.
#[test]
fn blocked_agent_times_out() {
    let dir = tmp("silent");
    unsafe {
        std::env::set_var("MIMUS_ACP_TIMEOUT_SECS", "3");
    }
    let c = AcpChecker::new(stub_agent(&dir, SILENT_STUB), &dir);
    let t0 = std::time::Instant::now();
    let r = c.check(&claim("the engine is very fast indeed", "doc.md", 1));
    let e = r.expect_err("silent agent must surface an error, not hang");
    assert!(e.contains("blocked"), "{e}");
    assert!(t0.elapsed().as_secs() < 60, "{:?}", t0.elapsed());
    unsafe {
        std::env::remove_var("MIMUS_ACP_TIMEOUT_SECS");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Comment-sourced claims: slash word-pairs and routes are not
/// paths — triage must stay silent. `false` as the agent proves
/// triage decided nothing (agent spawn would fail → Err).
#[test]
fn comment_slash_pairs_are_not_paths() {
    let dir = tmp("comments");
    let c = AcpChecker::new("false", &dir);
    for text in [
        "the author/assignee can update the row directly",
        "the handler fetch/CSRF tokens from the cookie jar",
        "this will change/remove the stale entries entirely",
        "the store accepts/rejects/resolves each pending op",
        "we retry at 50/100/200 ms backoff steps here",
        "responses include text/* content types upstream",
        "the react-hooks/exhaustive-deps rule fires here",
        "the /api/v1/extractors route handles all of it",
        "the /users/me/avatar endpoint caches its reads",
    ] {
        let cl = claim(text, "src/handler.rs", 7);
        assert!(c.check(&cl).is_err(), "triage flagged: {text}");
    }
    // a genuine filename inside a comment still triages
    let cl = claim(
        "reads its state from old/dead-file.rs always",
        "src/handler.rs",
        2,
    );
    let v = c.check(&cl).unwrap().unwrap();
    assert!(v.evidence.contains("old/dead-file.rs"), "{}", v.evidence);
    // code-source claim whose slash token has no extension →
    // filename_shaped's no-dot arm → skipped, falls to dead agent
    let cl = claim("mentions dead/nofile in passing here", "src/x.rs", 5);
    assert!(c.check(&cl).is_err());
    // source with no extension at all → is_code_source's None arm
    let cl = claim("the engine is very fast indeed", "Makefile", 1);
    assert!(c.check(&cl).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

/// `ignore_prefixes` reaches interior tokens of prose claims, not
/// just whole-claim text — the same suppression docs already got.
#[test]
fn ignore_prefixes_cover_comment_claim_tokens() {
    let dir = tmp("ignpref");
    let mut c = AcpChecker::new("false", &dir);
    c.ignore_prefixes = vec!["api/".into(), "internal/".into()];
    // filename-shaped dead token under an ignored prefix → silence
    // (falls through to the dead agent → Err, proving triage skipped)
    let cl = claim("the api/v1/routes.py module handles auth", "src/x.rs", 3);
    assert!(c.check(&cl).is_err());
    // doc-source claim — interior token suppressed the same way
    let cl = claim(
        "the internal/tools/daily cron job runs nightly",
        "doc.md",
        1,
    );
    assert!(c.check(&cl).is_err());
    // outside the prefixes → still triages
    let cl = claim(
        "the app/api/public/routes.py module is exposed",
        "src/x.rs",
        4,
    );
    let v = c.check(&cl).unwrap().unwrap();
    assert!(
        v.evidence.contains("app/api/public/routes.py"),
        "{}",
        v.evidence
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A contradiction reply must cite `A vs B: <what disagrees>` —
/// agent preamble ("Identifying contradictions") is not a finding.
#[test]
fn contradiction_needs_structured_line() {
    let dir = tmp("vsline");
    std::fs::write(dir.join("doc.md"), "x").unwrap();
    let c = AcpChecker::new(
        agent(
            &dir,
            &[
                "1 VERIFIED\\n2 VERIFIED",
                "Identifying contradictions between the claims now",
            ],
        ),
        &dir,
    );
    let claims = vec![
        claim("cache_value is stored in memory", "doc.md", 1),
        claim("cache_value resets on every boot", "doc.md", 2),
    ];
    let out = c.check_batch(&claims);
    assert_eq!(out.len(), 2, "preamble became a finding: {out:?}");

    // a real `A vs B:` line lands on claim A and cites claim B
    let dir = tmp("vsline2");
    std::fs::write(dir.join("doc.md"), "x").unwrap();
    let c = AcpChecker::new(
        agent(
            &dir,
            &[
                "1 VERIFIED\\n2 VERIFIED",
                "2 vs 1: one persists, one resets",
            ],
        ),
        &dir,
    );
    let claims = vec![
        claim("cache_value is stored in memory", "doc.md", 1),
        claim("cache_value resets on every boot", "doc.md", 2),
    ];
    let out = c.check_batch(&claims);
    assert_eq!(out.len(), 3, "{out:?}");
    let v = out[2].as_ref().unwrap().as_ref().unwrap();
    assert_eq!(v.claim.text, claims[1].text); // attached to claim 2
    assert!(v.evidence.contains("doc.md:1"), "{}", v.evidence);
    assert!(v.evidence.contains("persists"), "{}", v.evidence);
    let _ = std::fs::remove_dir_all(tmp("vsline").as_path());
    let _ = std::fs::remove_dir_all(tmp("vsline2").as_path());
}
