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
            let mut client = AcpClient::spawn(&self.cmd)?;
            client.request(
                "initialize",
                json!({"protocolVersion":1,"clientCapabilities":{},"clientInfo":{"name":"mimus","version":"0.1.0"}}),
            )?;
            let (session, _) = client.request(
                "session/new",
                json!({"cwd": self.root, "mcpServers": []}),
            )?;
            let sid = session["sessionId"].as_str().unwrap_or("mimus").to_owned();
            *guard = Some((client, sid));
        }
        let (client, sid) = guard.as_mut().unwrap();
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
}
