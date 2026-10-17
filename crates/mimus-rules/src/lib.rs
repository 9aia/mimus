//! Convention rules as data — `rules/*.toml` files evaluated
//! against source/doc files (ADR-0001). A new convention is a
//! file, not an engine change.

use mimus_core::{Claim, ClaimKind, Severity, Violation};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct RuleDef {
    pub name: String,
    pub message: String,
    #[serde(default = "default_severity")]
    pub severity: Severity,
    pub files: String,
    #[serde(rename = "match")]
    pub match_: RuleMatch,
}

fn default_severity() -> Severity {
    Severity::Warn
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RuleMatch {
    /// Regex over matched files' contents.
    SourcePattern { pattern: String },
}

/// Load every *.toml under `dir` (non-recursive is fine for v1).
pub fn load_rules(dir: &Path) -> Result<Vec<RuleDef>, String> {
    let mut rules = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(rules); // no rules dir = no rules
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().map(|x| x == "toml").unwrap_or(false) {
            let text = std::fs::read_to_string(&p)
                .map_err(|err| format!("read {}: {err}", p.display()))?;
            rules.push(
                toml::from_str(&text).map_err(|err| format!("parse {}: {err}", p.display()))?,
            );
        }
    }
    Ok(rules)
}

/// Evaluate one rule against one file. Returns violations found.
pub fn eval_rule(rule: &RuleDef, path: &Path, text: &str) -> Vec<Violation> {
    match &rule.match_ {
        RuleMatch::SourcePattern { pattern } => {
            let Ok(re) = regex::Regex::new(pattern) else {
                return Vec::new(); // bad rule = silence, reported by doctor
            };
            text.lines()
                .enumerate()
                .filter(|(_, l)| re.is_match(l))
                .map(|(i, l)| Violation {
                    claim: Claim {
                        kind: ClaimKind::Behavior,
                        text: l.trim().to_owned(),
                        source: path.to_path_buf(),
                        line: (i + 1) as u32,
                    },
                    evidence: rule.message.clone(),
                    fix: None,
                    severity: rule.severity,
                })
                .collect()
        }
    }
}

/// Files matching a rule's `files` glob under `root`.
pub fn matching_files(root: &Path, pattern: &str) -> Vec<PathBuf> {
    glob::glob(&root.join(pattern).to_string_lossy())
        .map(|paths| paths.flatten().collect())
        .unwrap_or_default()
}
