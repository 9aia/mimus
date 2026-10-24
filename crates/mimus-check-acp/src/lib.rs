//! Semantic checker — spawns an ACP agent (e.g. `cline --acp`)
//! to verify prose `behavior` claims a mechanical checker can't
//! reach: card prose, doc assertions about behavior. Advisory
//! tier only — every violation is `Warn`, never gates `check`.

use mimus_check::Checker;
use mimus_core::{Claim, ClaimKind, Severity, Violation};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;

/// Checks `ClaimKind::Behavior` — `new(acp_cmd, repo_root)`.
/// One agent session is reused across claims.
pub struct AcpChecker {
    cmd: String,
    root: PathBuf,
    inner: Mutex<Option<(AcpClient, String)>>,
    /// claim-key -> verdict line — `.mimus/verdicts.json`; an
    /// unchanged claim on an unchanged file never re-prompts.
    verdicts: Mutex<std::collections::HashMap<String, String>>,
    cache_path: PathBuf,
    /// `[check].ignore_prefixes` — applied to path-ish tokens found
    /// inside prose claims, same as whole-claim text upstream.
    pub ignore_prefixes: Vec<String>,
}

struct AcpClient {
    child: Child,
    stdin: ChildStdin,
    /// agent stdout, pumped by a reader thread — recv_timeout keeps
    /// a blocked agent (permission it never gets, userActionRequired,
    /// a wedged tool call) from hanging `mimus report` forever.
    lines: std::sync::mpsc::Receiver<Option<std::io::Result<String>>>,
    id: u64,
    timeout: std::time::Duration,
    /// repo root — `fs/read_text_file` requests resolve against it
    root: PathBuf,
}

impl Drop for AcpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `MIMUS_ACP_TIMEOUT_SECS`, default 240 — generous for real agents,
/// overridable so tests don't sit on a silent stub.
fn request_timeout() -> std::time::Duration {
    std::env::var("MIMUS_ACP_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(std::time::Duration::from_secs)
        .unwrap_or(std::time::Duration::from_secs(240))
}

/// REFUTED verdicts must cite grounded evidence — a path-like
/// token in the evidence that actually exists under `root`.
/// A refute without proof is pedantry, not signal (P1).
fn evidence_grounded(root: &std::path::Path, evidence: &str) -> bool {
    evidence.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '/' || c == '.' || c == '-'))
        .filter(|t| t.contains('/') || t.ends_with(".rs") || t.ends_with(".md") || t.ends_with(".toml"))
        .map(|t| t.trim_matches(|c: char| c == '.' || c == '/'))
        .any(|t| !t.is_empty() && root.join(t).exists())
}

/// Claim came from a *code* file — i.e. `extract_comments`, not a
/// doc. Comment prose is chattier than doc prose: `author/assignee`,
/// `accepts/rejects/resolves`, `50/100/200` are English word-pairs,
/// not paths. Doc/card claims keep the old loose token rule.
fn is_code_source(source: &std::path::Path) -> bool {
    matches!(
        source.extension().and_then(|e| e.to_str()),
        Some("rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs"
            | "go" | "java" | "kt" | "c" | "h" | "cc" | "cpp" | "hpp"
            | "rb" | "swift" | "cs" | "php" | "sh" | "bash" | "zsh")
    )
}

/// Last segment looks like a real filename — `file.rs`, `.env` —
/// i.e. it has an extension-ish dot with a name after it.
/// `exhaustive-deps`, `*`, `extractors` don't qualify.
fn filename_shaped(t: &str) -> bool {
    let last = t.rsplit('/').next().unwrap_or(t);
    match last.rfind('.') {
        Some(i) => last[i + 1..]
            .chars()
            .next()
            .map(|c| c.is_alphanumeric())
            .unwrap_or(false),
        None => false,
    }
}

/// A dead-path token inside a claim — mechanical triage: provable
/// without the agent, so don't spend a prompt on it (P3).
fn dead_path_token(
    root: &std::path::Path,
    claim: &Claim,
    ignore_prefixes: &[String],
) -> Option<String> {
    let strict = is_code_source(&claim.source);
    for t in claim
        .text
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '/' || c == '.' || c == '-'))
    {
        let t = t.trim_matches(|c: char| c == '.');
        if t.contains("://") || t.len() < 5 || !t.contains('/') {
            continue;
        }
        // `/api/v1/x`, `/etc/foo` — routes or host-absolute paths are
        // unprovable against a repo root; FsChecker already treats
        // leading-`/` path claims the same way → silence
        if t.starts_with('/') {
            continue;
        }
        // `[check].ignore_prefixes` applies to tokens inside prose
        // claims exactly as it does to whole-claim text upstream
        if ignore_prefixes.iter().any(|p| t.starts_with(p.as_str())) {
            continue;
        }
        // comment claims: only tokens ending in a filename-looking
        // last segment count — word-pairs aren't paths
        if strict && !filename_shaped(t) {
            continue;
        }
        if !root.join(t).exists() {
            return Some(t.to_owned());
        }
    }
    None
}

/// Verdict line -> Violation. Grounded refutes only — pedantry
/// without a real citation becomes silence.
fn refuted_violation(
    root: &std::path::Path,
    claim: &Claim,
    verdict: &str,
) -> Option<Violation> {
    let rest = verdict.strip_prefix("REFUTED")?;
    let evidence = rest.trim_start_matches(':').trim().to_owned();
    if evidence.is_empty() || !evidence_grounded(root, &evidence) {
        return None;
    }
    Some(Violation {
        claim: claim.clone(),
        evidence,
        fix: None,
        severity: Severity::Warn,
    })
}

/// `mimus doctor` probe — spawn the agent, send one canned
/// prompt, confirm text actually streams back (P6).
pub fn probe(cmd: &str, root: &PathBuf) -> Result<String, String> {
    let (mut client, sid) = AcpChecker::connect(cmd, root)?;
    let (_res, prose) = client.request(
        "session/prompt",
        json!({"sessionId": sid, "prompt":[{"type":"text","text":"Reply with only the word: ok"}]}),
    )?;
    if prose.trim().is_empty() {
        Err("agent answered but streamed no text".into())
    } else {
        Ok(format!("ok — {} chars streamed", prose.len()))
    }
}

/// `mimus fix --semantic` — propose a corrected claim line (P4).
/// Single prompt, no session state needed beyond connect.
pub fn propose(cmd: &str, root: &PathBuf, claim_text: &str, evidence: &str) -> Result<String, String> {
    let (mut client, sid) = AcpChecker::connect(cmd, root)?;
    let prompt = format!(
        "This claim about the repository is FALSE.\n\
         Claim: \"{claim_text}\"\nEvidence: {evidence}\n\n\
         Write the corrected claim text — one line, same style, \
         grounded in the actual code. No preamble, no quotes."
    );
    let (_res, prose) = client.request(
        "session/prompt",
        json!({"sessionId": sid, "prompt":[{"type":"text","text":prompt}]}),
    )?;
    let line = prose.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    if line.is_empty() {
        Err("agent proposed nothing".into())
    } else {
        Ok(line.to_owned())
    }
}

impl AcpChecker {
    pub fn new(cmd: impl Into<String>, root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let cache_path = root.join(".mimus/verdicts.json");
        let verdicts = std::fs::read_to_string(&cache_path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self {
            cmd: cmd.into(),
            root,
            inner: Mutex::new(None),
            verdicts: Mutex::new(verdicts),
            cache_path,
            ignore_prefixes: Vec::new(),
        }
    }

    /// Cache key: claim text + the bytes of its source file.
    /// Same claim on same content -> same verdict, no prompt.
    fn claim_key(&self, claim: &Claim) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        claim.text.hash(&mut h);
        if let Ok(b) = std::fs::read(self.root.join(&claim.source)) {
            b.hash(&mut h);
        }
        format!("{:x}", h.finish())
    }

    fn cache_get(&self, key: &str) -> Option<String> {
        self.verdicts.lock().ok()?.get(key).cloned()
    }

    fn cache_put(&self, key: String, verdict: String) {
        if let Ok(mut v) = self.verdicts.lock() {
            v.insert(key, verdict);
            let _ = std::fs::create_dir_all(
                self.cache_path.parent().unwrap_or(&self.root),
            );
            let _ = std::fs::write(
                &self.cache_path,
                serde_json::to_string_pretty(&*v).unwrap_or_default(),
            );
        }
    }

    /// Spawn + handshake + session — shared by check/check_batch.
    fn connect(cmd: &str, root: &PathBuf) -> Result<(AcpClient, String), String> {
        let mut client = AcpClient::spawn(cmd, root)?;
        client.request(
            "initialize",
            json!({"protocolVersion":1,"clientCapabilities":{},"clientInfo":{"name":"mimus","version":"0.1.0"}}),
        )?;
        let (session, _) = client.request(
            "session/new",
            json!({"cwd": root, "mcpServers": []}),
        )?;
        let sid = session["sessionId"].as_str().unwrap_or("mimus").to_owned();
        Ok((client, sid))
    }
}

impl AcpClient {
    fn spawn(cmd: &str, root: &PathBuf) -> Result<Self, String> {
        let mut child = Command::new("sh")
            .args(["-c", cmd])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("spawn {cmd}: {e}"))?;
        // stdio is piped above — take() can't return None here
        let stdin = child.stdin.take().expect("stdin piped");
        let mut reader = BufReader::new(child.stdout.take().expect("stdout piped"));
        // pump stdout on its own thread so reads can time out —
        // a blocked BufReader::read_line has no deadline otherwise
        let (tx, lines) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    // None = clean EOF — the agent closed stdout
                    Ok(0) => {
                        let _ = tx.send(None);
                        break;
                    }
                    Ok(_) => {
                        if tx.send(Some(Ok(line.clone()))).is_err() {
                            break; // client dropped — stop pumping
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Some(Err(e)));
                        break;
                    }
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            lines,
            id: 0,
            timeout: request_timeout(),
            root: root.clone(),
        })
    }

    fn send(&mut self, msg: &Value) -> Result<(), String> {
        writeln!(self.stdin, "{msg}")
            .and_then(|_| self.stdin.flush())
            .map_err(|e| e.to_string())
    }

    /// `Ok(result)` / `Err(error-object)` for an agent→client call.
    /// Answering is load-bearing: an unanswered request stalls the
    /// agent — that was the `request_permission` deadlock.
    fn agent_request_result(&self, method: &str, params: &Value) -> Result<Value, Value> {
        match method {
            // tool-enabled agents gate on permission; auto-answer or
            // the prompt stalls. Prefer an allow-kind option over a
            // reject — a checker that denies every tool call gets
            // nothing back.
            "session/request_permission" => {
                let opt = params["options"]
                    .as_array()
                    .and_then(|os| {
                        os.iter()
                            .find(|o| {
                                o["kind"]
                                    .as_str()
                                    .map(|k| k.starts_with("allow"))
                                    .unwrap_or(false)
                            })
                            .or_else(|| os.first())
                    })
                    .and_then(|o| o["optionId"].as_str())
                    .unwrap_or("allow_once");
                Ok(json!({"outcome":{"outcome":"selected","optionId":opt}}))
            }
            // agents that read via the client's fs capability instead
            // of shell tools — serve reads under root, refuse the rest
            "fs/read_text_file" => {
                let path = params["path"].as_str().unwrap_or("");
                match std::fs::read_to_string(self.root.join(path)) {
                    Ok(content) => Ok(json!({"content": content})),
                    Err(e) => Err(json!({"code":-32602,"message":e.to_string()})),
                }
            }
            _ => Err(json!({"code":-32601,"message":"method not found"})),
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Result<(Value, String), String> {
        self.id += 1;
        let id = self.id;
        self.send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        let mut chunks = String::new();
        loop {
            let line = match self.lines.recv_timeout(self.timeout) {
                Ok(Some(Ok(l))) => l,
                Ok(Some(Err(e))) => return Err(e.to_string()),
                // EOF or a dead pump thread — the agent is gone
                Ok(None) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("agent closed stdout".into());
                }
                // a blocked agent (permission nobody answers,
                // userActionRequired, a wedged tool) degrades to an
                // error — never an infinite hang
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    return Err(format!(
                        "{method}: no agent reply within {}s — agent blocked",
                        self.timeout.as_secs()
                    ));
                }
            };
            let Ok(msg) = serde_json::from_str::<Value>(line.trim()) else {
                continue;
            };
            // agent -> client request (permission prompts, fs reads):
            // it has BOTH an id and a method — and the id may be a
            // STRING, so match presence, not `as_u64()` (the old code
            // let string-id permissions through → agent waited forever)
            if msg["method"].is_string() && msg.get("id").is_some() && !msg["id"].is_null() {
                let rid = msg["id"].clone(); // echo verbatim — any JSON id type
                let m = msg["method"].as_str().unwrap_or("");
                let resp = match self.agent_request_result(m, &msg["params"]) {
                    Ok(result) => json!({"jsonrpc":"2.0","id":rid,"result":result}),
                    Err(error) => json!({"jsonrpc":"2.0","id":rid,"error":error}),
                };
                let _ = self.send(&resp);
                continue;
            }
            if msg["method"].as_str() == Some("session/update") {
                let content = &msg["params"]["update"]["content"];
                // a chunk is one content block or a list of them
                if let Some(t) = content["text"].as_str() {
                    chunks.push_str(t);
                } else if let Some(blocks) = content.as_array() {
                    for b in blocks {
                        if let Some(t) = b["text"].as_str() {
                            chunks.push_str(t);
                        }
                    }
                }
                continue;
            }
            if msg["id"].as_u64() == Some(id) {
                if let Some(err) = msg.get("error") {
                    return Err(format!("{method}: {err}"));
                }
                return Ok((msg["result"].clone(), chunks));
            }
        }
    }
}

/// `A vs B: <what disagrees>` -> 1-based indexes within the cluster.
/// Anything else (preamble, summaries, malformed) is not a finding.
fn parse_vs_line(line: &str, cluster_len: usize) -> Option<(usize, usize, &str)> {
    let (head, what) = line.split_once(':')?;
    let mut p = head.split_whitespace();
    let (a, vs, b) = (p.next()?, p.next()?, p.next()?);
    if vs != "vs" || p.next().is_some() {
        return None;
    }
    let (a, b) = (a.parse::<usize>().ok()?, b.parse::<usize>().ok()?);
    if a == 0 || b == 0 || a > cluster_len || b > cluster_len {
        return None;
    }
    let what = what.trim();
    if what.is_empty() {
        return None;
    }
    Some((a, b, what))
}

fn verify_prompt(claim: &Claim) -> String {
    format!(
        "Try to FALSIFY this claim about the repository. \
         Claim (from {}:{}): \"{}\"\n\n\
         Open the relevant source files and hunt for counter-evidence — \
         your job is to prove it false, not confirm it. Reply with ONLY one line:\n\
         VERIFIED — only if you actively tried to break it and could not\n\
         REFUTED: <one-sentence evidence> — with the file/line you found\n\
         UNVERIFIABLE — if you cannot prove either\n\
         No preamble, no explanation beyond the one line.",
        claim.source.display(),
        claim.line,
        claim.text
    )
}

impl Checker for AcpChecker {
    fn handles(&self) -> &[ClaimKind] {
        &[ClaimKind::Behavior]
    }

    fn check(&self, claim: &Claim) -> Result<Option<Violation>, String> {
        // triage: dead-path tokens are decidable for free (P3)
        if let Some(dead) = dead_path_token(&self.root, claim, &self.ignore_prefixes) {
            return Ok(Some(Violation {
                claim: claim.clone(),
                evidence: format!("`{dead}` does not exist"),
                fix: None,
                severity: Severity::Warn,
            }));
        }
        let key = self.claim_key(claim);
        let first = match self.cache_get(&key) {
            Some(v) => v,
            None => {
                let mut guard = self.inner.lock().map_err(|e| e.to_string())?;
                if guard.is_none() {
                    *guard = Some(Self::connect(&self.cmd, &self.root)?);
                }
                let (client, sid) = guard.as_mut().unwrap();
                // debounce — one agent, many claims; be polite to
                // the provider (rate limits) rather than fanning out
                std::thread::sleep(std::time::Duration::from_millis(1500));
                let (_res, prose) = client.request(
                    "session/prompt",
                    json!({"sessionId": sid.as_str(),
                           "prompt":[{"type":"text","text":verify_prompt(claim)}]}),
                )?;
                let v = prose
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("")
                    .trim()
                    .to_owned();
                self.cache_put(key, v.clone());
                v
            }
        };
        // only grounded REFUTED becomes a violation — a refute
        // without a real citation is pedantry, not signal
        if let Some(rest) = first.strip_prefix("REFUTED") {
            let evidence = rest.trim_start_matches(':').trim().to_owned();
            if evidence.is_empty() || !evidence_grounded(&self.root, &evidence) {
                return Ok(None); // unproven refute -> silence
            }
            return Ok(Some(Violation {
                claim: claim.clone(),
                evidence,
                fix: None,
                severity: Severity::Warn,
            }));
        }
        Ok(None) // VERIFIED or UNVERIFIABLE — silence
    }

    /// One prompt per claim batch — folds N claims into a single
    /// numbered verify prompt so we don't spam the provider.
    fn check_batch(&self, claims: &[Claim]) -> Vec<Result<Option<Violation>, String>> {
        if claims.len() <= 1 {
            return claims.iter().map(|c| self.check(c)).collect();
        }
        // self-cleaning cache — keys are claim+file hashes; claims
        // no longer in the set are dropped so the file can't grow
        {
            let keys: std::collections::HashSet<String> =
                claims.iter().map(|c| self.claim_key(c)).collect();
            // `.map` over lock() — a poisoned lock skips pruning via
            // Result's internals, no dead `if let` else-arm here
            let _ = self.verdicts.lock().map(|mut v| {
                let before = v.len();
                v.retain(|k, _| keys.contains(k));
                if v.len() != before {
                    let _ = std::fs::write(
                        &self.cache_path,
                        serde_json::to_string_pretty(&*v).unwrap_or_default(),
                    );
                }
            });
        }
        // triage + cache first — dead paths are free verdicts,
        // cached claims never re-prompt
        let mut out: Vec<Option<Result<Option<Violation>, String>>> =
            Vec::with_capacity(claims.len());
        let mut pending: Vec<(usize, &Claim)> = Vec::new();
        for (i, c) in claims.iter().enumerate() {
            if let Some(dead) = dead_path_token(&self.root, c, &self.ignore_prefixes) {
                out.push(Some(Ok(Some(Violation {
                    claim: c.clone(),
                    evidence: format!("`{dead}` does not exist"),
                    fix: None,
                    severity: Severity::Warn,
                }))));
                continue;
            }
            let key = self.claim_key(c);
            if let Some(v) = self.cache_get(&key) {
                out.push(Some(Ok(refuted_violation(&self.root, c, &v))));
                continue;
            }
            out.push(None);
            pending.push((i, c));
        }
        if pending.is_empty() {
            return out.into_iter().map(|o| o.unwrap()).collect();
        }
        // one session, one prompt, numbered verdicts
        let numbered: String = pending
            .iter()
            .enumerate()
            .map(|(i, (_, c))| format!("{}. {}:{}\n{}\n", i + 1, c.source.display(), c.line, c.text))
            .collect();
        let prompt = format!(
            "For each numbered claim about this repository, reply on \
             its own line as `N VERIFIED`, `N REFUTED: <evidence>`, or \
             `N UNVERIFIABLE`. One line per claim, no extras:\n{numbered}"
        );
        // a poisoned session lock is still usable — take the guard
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            match Self::connect(&self.cmd, &self.root) {
                Ok(s) => *guard = Some(s),
                Err(e) => return vec![Err(e)],
            }
        }
        let (client, sid) = guard.as_mut().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let prose = match client.request(
            "session/prompt",
            json!({"sessionId": sid.as_str(),
                   "prompt":[{"type":"text","text":prompt}]}),
        ) {
            Ok((_, prose)) => prose,
            Err(e) => return vec![Err(e)],
        };
        // parse numbered verdicts back — cache every verdict,
        // surface only grounded refutes
        for (i, (orig, c)) in pending.iter().enumerate() {
            let tag = format!("{} ", i + 1);
            // strip the `N ` tag — verdicts must look exactly like the
            // single-claim replies (`REFUTED: …`, `VERIFIED`, …)
            let verdict = prose
                .lines()
                .map(str::trim_start)
                .find(|l| l.starts_with(&tag))
                .map(|l| l[tag.len()..].trim().to_owned())
                .unwrap_or_default();
            self.cache_put(self.claim_key(c), verdict.clone());
            out[*orig] = Some(Ok(refuted_violation(&self.root, c, &verdict)));
        }
        let mut out: Vec<Result<Option<Violation>, String>> =
            out.into_iter().map(|o| o.unwrap()).collect();
        // doc↔doc pass: claims sharing a subject token get one
        // "do these contradict?" prompt per cluster — bounded,
        // advisory. Results append as extra violations.
        let mut subject: std::collections::HashMap<String, Vec<&Claim>> =
            std::collections::HashMap::new();
        for c in claims {
            for tok in c.text.split(|ch: char| !(ch.is_alphanumeric() || ch == '_')) {
                let tok = tok.trim_matches(|ch: char| !ch.is_alphanumeric());
                // identifier-shaped subjects only — snake_case or
                // Capitalized, ≥6 chars. Filters natural words so
                // clusters are genuinely about the same thing.
                let identish = tok.len() >= 6
                    && (tok.contains('_')
                        || tok.chars().next().map(|c| c.is_uppercase()).unwrap_or(false));
                if identish {
                    subject.entry(tok.to_owned()).or_default().push(c);
                }
            }
        }
        let mut clusters: Vec<&Vec<&Claim>> = subject
            .values()
            .filter(|v| v.len() >= 2)
            .collect();
        clusters.sort_by_key(|v| std::cmp::Reverse(v.len()));
        for cluster in clusters.into_iter().take(8) {
            // dedupe by (source,line) — same claim can share
            // several subject tokens
            let mut seen = std::collections::HashSet::new();
            let uniq: Vec<&&Claim> = cluster
                .iter()
                .filter(|c| seen.insert((&c.source, c.line)))
                .collect();
            if uniq.len() < 2 {
                continue;
            }
            let body: String = uniq
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    format!("{}. {}:{} — {}\n", i + 1, c.source.display(), c.line, c.text)
                })
                .collect();
            let q = format!(
                "These claims are about the same subject. Do any pair \
                 contradict each other? Reply per contradiction as \
                 `A vs B: <what disagrees>` or exactly `NONE`:\n{body}"
            );
            let (client, sid) = guard.as_mut().unwrap(); // connected above
            std::thread::sleep(std::time::Duration::from_millis(1500));
            let Ok((_, ans)) = client.request(
                "session/prompt",
                json!({"sessionId": sid.as_str(),
                       "prompt":[{"type":"text","text":q}]}),
            ) else { break };
            for l in ans.lines() {
                let t = l.trim();
                if t == "NONE" || t.is_empty() {
                    continue;
                }
                // structured replies only — `A vs B: <what disagrees>`.
                // Preamble like "Identifying contradictions" carries
                // no claim+proof → evidence-or-silence drops it (P1)
                let Some((a, b, what)) = parse_vs_line(t, uniq.len()) else {
                    continue;
                };
                let other = uniq[b - 1];
                out.push(Ok(Some(Violation {
                    claim: (*uniq[a - 1]).clone(),
                    evidence: format!(
                        "contradicts {}:{} — {}",
                        other.source.display(),
                        other.line,
                        what
                    ),
                    fix: None,
                    severity: Severity::Warn,
                })));
                break;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pump's arms: Ok(_) send when the receiver is alive, the
    /// send-error `break` once it's dead, and the `Err(e)` read arm
    /// on non-UTF-8 output — all deterministic via a delayed agent.
    #[test]
    fn pump_breaks_after_client_drop() {
        // Agent echoes "a" immediately, "b" after a delay. The "a"
        // line lands a successful send; the receiver swap happens in
        // the quiet window; "b" then lands on a dead channel →
        // is_err → break. No sleeps needed for ordering.
        let mut client = AcpClient::spawn(
            "echo a; sleep 1; echo b; sleep 60",
            &PathBuf::from("."),
        )
        .expect("spawn echoer");
        let first = client
            .lines
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("a line arrives");
        assert!(first.is_some());
        let (_tx, dead) = std::sync::mpsc::channel();
        let gone = std::mem::replace(&mut client.lines, dead);
        drop(gone);
        // the delayed "b" send now fails for sure; give the pump a
        // beat to observe it before the client (and its stdin) drop
        std::thread::sleep(std::time::Duration::from_millis(1500));
        drop(client);
        // belt & suspenders: the flood variant too
        for _ in 0..3 {
            let client = AcpClient::spawn(
                "head -c 100000000 /dev/zero | tr '\\0' 'x'",
                &PathBuf::from("."),
            )
            .expect("spawn flooder");
            std::thread::sleep(std::time::Duration::from_millis(10));
            drop(client);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // Err(e) arm: invalid UTF-8 on stdout → read_line errors →
        // send(Err) → break. recv the Err line so the arm provably
        // ran before the client drops (a bare drop races the pump).
        let mut client = AcpClient::spawn("printf '\\377'", &PathBuf::from("."))
            .expect("spawn binary junk");
        let msg = client
            .lines
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the bad byte surfaces");
        assert!(matches!(msg, Some(Err(_))), "{msg:?}");
        drop(client);
        // `sh` can't be spawned with an empty PATH → connect fails
        // at the spawn `?`. Env mutation lives inside this test so
        // sibling tests (which tolerate spawn failure) stay sane.
        let old_path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", "");
        let root = PathBuf::from(".");
        assert!(AcpClient::spawn("anything", &root).is_err());
        assert!(AcpChecker::connect("anything", &root).is_err());
        std::env::set_var("PATH", old_path);
    }

    /// `MIMUS_ACP_TIMEOUT_SECS` set to garbage → parse fails →
    /// falls back to the 240s default rather than panicking.
    #[test]
    fn garbage_timeout_env_falls_back() {
        unsafe {
            std::env::set_var("MIMUS_ACP_TIMEOUT_SECS", "not-a-number");
        }
        let c = AcpClient::spawn("sleep 2", &PathBuf::from(".")).expect("spawn");
        assert_eq!(c.timeout, std::time::Duration::from_secs(240));
        unsafe {
            std::env::remove_var("MIMUS_ACP_TIMEOUT_SECS");
        }
    }

    /// Writing to an agent that closed its stdin → `send` fails,
    /// and `request` propagates the Err.
    #[test]
    fn send_to_closed_stdin_errors() {
        let mut client = AcpClient::spawn("exec 0<&-; sleep 5", &PathBuf::from("."))
            .expect("spawn");
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert!(client.send(&json!({"x":1})).is_err());
        assert!(client.request("initialize", json!({})).is_err());
    }

    /// A poisoned `verdicts` lock — the batch cache-prune block is
    /// skipped rather than panicking, and `cache_get`/`cache_put`
    /// degrade to no-cache rather than failing.
    #[test]
    fn poisoned_verdicts_lock_skips_prune() {
        let dir = std::env::temp_dir()
            .join(format!("mimus-acp-poison-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let c = AcpChecker::new("false", &dir);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = c.verdicts.lock().unwrap();
            panic!("poison the lock");
        }));
        // lock poisoned → cache_get misses (`.ok()?` → None), the
        // claim falls through to the (dead) agent → Err
        let cl = Claim {
            kind: ClaimKind::Behavior,
            text: "the engine is very fast indeed".into(),
            source: "doc.md".into(),
            line: 1,
        };
        assert!(c.check(&cl).is_err());
        // direct cache_put on a poisoned lock is a silent no-op
        c.cache_put("k".into(), "VERIFIED".into());
        let claims = vec![
            Claim {
                kind: ClaimKind::Behavior,
                text: "see dead/a-file.rs for it".into(),
                source: "doc.md".into(),
                line: 1,
            },
            Claim {
                kind: ClaimKind::Behavior,
                text: "see dead/b-file.rs for it".into(),
                source: "doc.md".into(),
                line: 2,
            },
        ];
        let out = c.check_batch(&claims);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|r| r.as_ref().unwrap().is_some()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A poisoned `inner` lock — `check` propagates an Err, while
    /// `check_batch` recovers the guard via `into_inner()` and keeps
    /// going (to the dead agent, here).
    #[test]
    fn poisoned_session_lock_paths() {
        let dir = std::env::temp_dir()
            .join(format!("mimus-acp-poison2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let c = AcpChecker::new("false", &dir);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = c.inner.lock().unwrap();
            panic!("poison the lock");
        }));
        let cl = Claim {
            kind: ClaimKind::Behavior,
            text: "the engine is very fast indeed".into(),
            source: "doc.md".into(),
            line: 1,
        };
        assert!(c.check(&cl).is_err()); // map_err arm — poison surfaces
        // batch recovers the poisoned guard, then connect fails
        let out = c.check_batch(&[cl.clone(), cl]);
        assert_eq!(out.len(), 1);
        assert!(out[0].is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
