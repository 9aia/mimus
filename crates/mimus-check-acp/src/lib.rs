//! Semantic checker — spawns an ACP agent (e.g. `cline --acp`)
//! to verify prose `behavior` claims a mechanical checker can't
//! reach: card prose, doc assertions about behavior. Advisory
//! tier only — every violation is `Warn`, never gates `check`.

use mimus_check::Checker;
use mimus_core::{Claim, ClaimKind, Severity, Violation};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;

/// Checks `ClaimKind::Behavior` — `new(acp_cmd, repo_root)`.
/// One agent session is reused across claims.
pub struct AcpChecker {
    cmd: String,
    root: PathBuf,
    inner: Mutex<Option<(AcpClient, String)>>,
}

struct AcpClient {
    _child: Child,
    stdin: ChildStdin,
    reader: BufReader<ChildStdout>,
    id: u64,
}

impl AcpChecker {
    pub fn new(cmd: impl Into<String>, root: impl Into<PathBuf>) -> Self {
        Self {
            cmd: cmd.into(),
            root: root.into(),
            inner: Mutex::new(None),
        }
    }

    /// Spawn + handshake + session — shared by check/check_batch.
    fn connect(cmd: &str, root: &PathBuf) -> Result<(AcpClient, String), String> {
        let mut client = AcpClient::spawn(cmd)?;
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
    fn spawn(cmd: &str) -> Result<Self, String> {
        let mut child = Command::new("sh")
            .args(["-c", cmd])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("spawn {cmd}: {e}"))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let reader = BufReader::new(child.stdout.take().ok_or("no stdout")?);
        Ok(Self {
            _child: child,
            stdin,
            reader,
            id: 0,
        })
    }

    fn send(&mut self, msg: &Value) -> Result<(), String> {
        writeln!(self.stdin, "{msg}")
            .and_then(|_| self.stdin.flush())
            .map_err(|e| e.to_string())
    }

    fn request(&mut self, method: &str, params: Value) -> Result<(Value, String), String> {
        self.id += 1;
        let id = self.id;
        self.send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        let mut chunks = String::new();
        let mut line = String::new();
        loop {
            line.clear();
            if self.reader.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
                return Err("agent closed stdout".into());
            }
            let Ok(msg) = serde_json::from_str::<Value>(line.trim()) else {
                continue;
            };
            // agent -> client request (permission prompts etc.) —
            // answer or the prompt deadlocks
            if let (Some(rid), Some(m)) = (msg["id"].as_u64(), msg["method"].as_str()) {
                let resp = if m == "session/request_permission" {
                    let opt = msg["params"]["options"]
                        .as_array()
                        .and_then(|o| o.first())
                        .and_then(|o| o["optionId"].as_str())
                        .unwrap_or("allow_once");
                    json!({"jsonrpc":"2.0","id":rid,
                           "result":{"outcome":{"outcome":"selected","optionId":opt}}})
                } else {
                    json!({"jsonrpc":"2.0","id":rid,
                           "error":{"code":-32601,"message":"method not found"}})
                };
                let _ = self.send(&resp);
                continue;
            }
            if msg["method"].as_str() == Some("session/update") {
                if let Some(t) = msg["params"]["update"]["content"]["text"].as_str() {
                    chunks.push_str(t);
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

fn verify_prompt(claim: &Claim) -> String {
    format!(
        "Verify this claim about the repository. \
         Claim (from {}:{}): \"{}\"\n\n\
         Check the relevant source files if needed. Reply with ONLY one line:\n\
         VERIFIED — if the claim is true\n\
         REFUTED: <one-sentence evidence> — if it is false\n\
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
        let mut guard = self.inner.lock().map_err(|e| e.to_string())?;
        if guard.is_none() {
            *guard = Some(Self::connect(&self.cmd, &self.root)?);
        }
        let (client, sid) = guard.as_mut().unwrap();
        // debounce — one agent, many claims; be polite to the
        // provider (rate limits) rather than fanning out
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let (_res, prose) = client.request(
            "session/prompt",
            json!({"sessionId": sid.as_str(),
                   "prompt":[{"type":"text","text":verify_prompt(claim)}]}),
        )?;
        // only REFUTED becomes a violation — advisory Warn always
        let first = prose.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        if let Some(rest) = first.trim().strip_prefix("REFUTED") {
            let evidence = rest.trim_start_matches(':').trim().to_owned();
            return Ok(Some(Violation {
                claim: claim.clone(),
                evidence: if evidence.is_empty() {
                    "agent refuted this claim".into()
                } else {
                    evidence
                },
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
        // one session, one prompt, numbered verdicts
        let numbered: String = claims
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{}. {}:{}\n{}\n", i + 1, c.source.display(), c.line, c.text))
            .collect();
        let prompt = format!(
            "For each numbered claim about this repository, reply on \
             its own line as `N VERIFIED`, `N REFUTED: <evidence>`, or \
             `N UNVERIFIABLE`. One line per claim, no extras:\n{numbered}"
        );
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(e) => return vec![Err(e.to_string())],
        };
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
        // parse numbered verdicts back to per-claim results
        let mut out: Vec<Result<Option<Violation>, String>> = claims
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let tag = format!("{} ", i + 1);
                let verdict = prose.lines().find(|l| l.trim_start().starts_with(&tag));
                match verdict {
                    Some(l) if l.contains("REFUTED") => Ok(Some(Violation {
                        claim: c.clone(),
                        evidence: l.splitn(2, ':').nth(1).unwrap_or(l).trim().to_owned(),
                        fix: None,
                        severity: Severity::Warn,
                    })),
                    _ => Ok(None),
                }
            })
            .collect();
        // doc↔doc pass: claims sharing a subject token get one
        // "do these contradict?" prompt per cluster — bounded,
        // advisory. Results append as extra violations.
        let mut subject: std::collections::HashMap<String, Vec<&Claim>> =
            std::collections::HashMap::new();
        for c in claims {
            for tok in c.text.split(|ch: char| !(ch.is_alphanumeric() || ch == '_')) {
                let tok = tok.trim_matches(|ch: char| !ch.is_alphanumeric());
                if tok.len() >= 6 && tok.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
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
            let (client, sid) = match guard.as_mut() {
                Some(t) => t,
                None => break,
            };
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
                // any non-NONE answer = a reported contradiction —
                // attach to the first claim in the cluster
                out.push(Ok(Some(Violation {
                    claim: (*uniq[0]).clone(),
                    evidence: format!("contradiction: {t}"),
                    fix: None,
                    severity: Severity::Warn,
                })));
                break;
            }
        }
        out
    }
}
