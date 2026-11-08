//! Cuora adapter of `Secrets` — resolves names through the `cuora`
//! CLI, never by linking vault internals: env vars stay the
//! interface (cuora ADR-0001). `get` → `cuora get cuora://{ns}/{name}
//! --json`; `set` → `cuora put` with the value on **stdin** (never
//! argv — cuora's own rule: args land in history and `/proc`).
//!
//! cuora's audit ndjson rides the inherited stderr through to the
//! caller — every mimus lookup is a materialization on the wire.

use mimus_secret::Secrets;
use std::io::{Read, Write};
use std::process::{Command, Stdio};

pub struct CuoraSecrets {
    /// The cuora binary — default `cuora` (PATH resolution).
    pub bin: String,
    /// Ref namespace: `get("ANTHROPIC_API_KEY")` resolves
    /// `cuora://{ns}/ANTHROPIC_API_KEY`.
    pub ns: String,
    /// Env context forwarded as `--env`; None = cuora's own
    /// resolution (`CUORA_ENV` → `dev`).
    pub env: Option<String>,
}

impl Default for CuoraSecrets {
    fn default() -> Self {
        Self {
            bin: "cuora".into(),
            ns: "mimus".into(),
            env: None,
        }
    }
}

impl CuoraSecrets {
    fn rref(&self, name: &str) -> String {
        format!("cuora://{}/{name}", self.ns)
    }

    fn cmd(&self) -> Command {
        let mut c = Command::new(&self.bin);
        // Attribution for cuora's audit events — a claimed label,
        // not identity (spoofable, like every local actor claim).
        // Honor a pre-rename ARCA_ACTOR export too.
        if std::env::var_os("CUORA_ACTOR").is_none() && std::env::var_os("ARCA_ACTOR").is_none() {
            c.env("CUORA_ACTOR", "mimus");
        }
        if let Some(e) = &self.env {
            c.args(["--env", e]);
        }
        c
    }
}

impl Secrets for CuoraSecrets {
    fn name(&self) -> &str {
        "cuora"
    }

    fn available(&self) -> bool {
        Command::new(&self.bin)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn get(&self, name: &str) -> Option<String> {
        let mut child = self
            .cmd()
            .args(["get", &self.rref(name), "--json"])
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()) // audit events pass through
            .spawn()
            .ok()?;
        let mut out = String::new();
        child.stdout.take()?.read_to_string(&mut out).ok()?;
        if !child.wait().ok()?.success() {
            return None;
        }
        serde_json::from_str::<serde_json::Value>(&out)
            .ok()?
            .get("value")?
            .as_str()
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    }

    fn set(&self, name: &str, value: &str) -> std::io::Result<()> {
        let mut child = self
            .cmd()
            .args(["put", &self.rref(name)])
            .stdin(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        child.stdin.as_mut().unwrap().write_all(value.as_bytes())?;
        if child.wait().map(|s| s.success()).unwrap_or(false) {
            Ok(())
        } else {
            Err(std::io::Error::other("cuora put failed"))
        }
    }
}
