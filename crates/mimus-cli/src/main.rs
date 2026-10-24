//! mimus — spots the cuckoo eggs in your repo. Composition root:
//! parses mimus.toml, wires checker adapters, runs subcommands.

use clap::{Parser, Subcommand};
use mimus_check::Checker;
use mimus_config::ConfigSource;
use mimus_config_toml::TomlSource;
use mimus_core::{ClaimKind, Config, Severity, Violation};
use std::path::{Path, PathBuf};

const EXAMPLE_TOML: &str = include_str!("../../../examples/mimus.toml.example");

#[derive(Parser)]
#[command(
    name = "mimus",
    version,
    about = "Spots the cuckoo eggs in your repo — claim verification: docs-as-tests, memory audit"
)]
struct Cli {
    /// Config path (else $MIMUS_CONFIG, ./mimus.toml,
    /// ~/.config/mimus/mimus.toml).
    #[arg(long, global = true)]
    config: Option<String>,

    /// Output format: human | json.
    #[arg(long, global = true, default_value = "human")]
    format: String,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Detect sources -> mimus.toml + rules/ scaffold.
    Init,
    /// Verify claims. Mechanical tier by default.
    Check {
        /// Include the semantic tier (advisory only, never gates).
        #[arg(long)]
        all: bool,
        /// Skip command claims — the PATH probe is machine-dependent.
        #[arg(long)]
        no_cmd: bool,
    },
    /// Apply mechanical rewrites for confirmed violations.
    Fix {
        /// Show the plan, write nothing.
        #[arg(long)]
        dry: bool,
        /// Agentic tier: agent proposes corrected claim text for
        /// semantic (REFUTED) findings. Needs [check].acp_cmd.
        #[arg(long)]
        semantic: bool,
    },
    /// Trust-score .kuda/ cards vs source.
    Audit,
    /// Full violations list with evidence.
    Report,
    /// Checker/config/kuda-index health.
    Doctor,
    /// Doc coverage — which indexed symbols no doc ever names.
    /// The reverse direction of `check`: code has it, docs silent.
    Coverage,
    /// Dump extracted claims (kind/source/line) — tuning aid for
    /// the extractor's precision on real docs.
    Claims {
        /// Include card-prose Behavior claims too.
        #[arg(long)]
        all: bool,
    },
    /// Emit CLI reference markdown (docs pipeline).
    #[command(hide = true, name = "gen-cli-docs")]
    GenCliDocs,
}

fn main() {
    let cli = Cli::parse();
    let code = match &cli.cmd {
        Cmd::Init => init(&cli),
        Cmd::Check { all, no_cmd } => check(&cli, *all, *no_cmd),
        Cmd::Fix { dry, semantic } => fix(&cli, *dry, *semantic),
        Cmd::Report => report(&cli),
        Cmd::Audit => audit(&cli),
        Cmd::Doctor => doctor(&cli),
        Cmd::Claims { all } => claims_dump(&cli, *all),
        Cmd::Coverage => coverage(&cli),
        Cmd::GenCliDocs => {
            println!("{}", clap_markdown::help_markdown::<Cli>());
            0
        }
    };
    std::process::exit(code);
}

fn root(cli: &Cli) -> PathBuf {
    cli.config
        .as_deref()
        .map(Path::new)
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| ".".into())
}

fn init(cli: &Cli) -> i32 {
    let dir = root(cli);
    let toml_path = dir.join("mimus.toml");
    if toml_path.exists() {
        eprintln!("mimus.toml already exists — not overwriting");
        return 1;
    }
    if let Err(e) = std::fs::write(&toml_path, EXAMPLE_TOML) {
        eprintln!("write {}: {e}", toml_path.display());
        return 1;
    }
    let rules = dir.join("rules");
    if let Err(e) = std::fs::create_dir_all(&rules) {
        eprintln!("mkdir rules: {e}");
        return 1;
    }
    println!("wrote {} + rules/ — run `mimus check`", toml_path.display());
    0
}

fn load(cli: &Cli) -> Result<Config, String> {
    TomlSource {
        path: cli.config.clone(),
    }
    .load()
}

/// Doc-glob -> claims (shared by `check` and `claims` dump).
fn doc_claims(cfg: &Config, root: &Path) -> Vec<mimus_core::Claim> {
    let mut claims = Vec::new();
    for pat in &cfg.sources.docs {
        for p in mimus_rules::matching_files(&root, pat) {
            // source globs catch build output too — prune it
            let noisy = p.components().any(|c| {
                let s = c.as_os_str().to_str();
                matches!(
                    s,
                    Some("node_modules" | "dist" | ".cache" | "target")
                ) || s.map(|n| n.ends_with(".files")).unwrap_or(false)
            }) || p
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| {
                    n.starts_with("LICENSE")
                        || n.starts_with("NOTICE")
                        || n.starts_with("CHANGELOG")
                })
                .unwrap_or(false);
            if noisy {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&p) {
                claims.extend(mimus_claims::extract(&text, &p));
            }
        }
    }
    // code comments asserting things -> Behavior claims the
    // semantic tier verifies against the code they sit on.
    if cfg.sources.comments {
        for pat in ["crates/**/*.rs", "src/**/*.rs", "**/*.py", "**/*.ts"] {
            for p in mimus_rules::matching_files(&root, pat) {
                let noisy = p.components().any(|c| {
                    matches!(
                        c.as_os_str().to_str(),
                        Some("node_modules" | "dist" | ".cache" | "target"
                             | "tests" | "fixtures")
                    )
                });
                if noisy {
                    continue;
                }
                if let Ok(text) = std::fs::read_to_string(&p) {
                    claims.extend(mimus_claims::extract_comments(&text, &p));
                }
            }
        }
    }
    claims
}

/// `mimus coverage` — reverse check: symbols present in the index
/// but absent from every doc source. Mechanical (name-mention
/// level); advisory, never gates.
fn coverage(cli: &Cli) -> i32 {
    let cfg = match load(cli) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let Some(idx) = &cfg.check.kuda_index else {
        eprintln!("coverage needs [check].kuda_index — run `kuda sync` first");
        return 1;
    };
    let root = root(cli);
    let syms = match mimus_check_symbol::all_symbols(&root.join(idx)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("index {}: {e}", idx.display());
            return 1;
        }
    };
    // doc corpus: every doc source as text + whole-token index
    let mut corpus = String::new();
    for pat in &cfg.sources.docs {
        for p in mimus_rules::matching_files(&root, pat) {
            if let Ok(t) = std::fs::read_to_string(&p) {
                corpus.push_str(&t);
                corpus.push('\n');
            }
        }
    }
    let tokens: std::collections::HashSet<&str> = corpus
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|t| !t.is_empty())
        .collect();
    // generic names are unmatchable noise — `new`, `get`, `run`…
    const GENERIC: &[&str] = &[
        "new", "get", "set", "run", "main", "default", "from", "into",
        "clone", "fmt", "drop", "len", "push", "iter", "read", "write",
    ];
    let mut by_mod: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    let mut total = 0usize;
    let mut covered = 0usize;
    let mut seen = std::collections::HashSet::new();
    for (name, kind, file, module) in &syms {
        // tests are docs for the suite, not the API — skip
        if file.contains("/tests/") || file.ends_with("_test.rs") {
            continue;
        }
        if name.len() < 4 || GENERIC.contains(&name.as_str()) {
            continue;
        }
        if !seen.insert(name.clone()) {
            continue; // same name in N files counts once
        }
        total += 1;
        let named = tokens.contains(name.as_str())
            || corpus.contains(name.as_str());
        if named {
            covered += 1;
        } else {
            by_mod
                .entry(format!("{module} ({kind})"))
                .or_default()
                .push(name.clone());
        }
    }
    let pct = if total > 0 { covered * 100 / total } else { 0 };
    println!("{covered}/{total} symbols named in docs ({pct}%)");
    for (m, names) in &by_mod {
        let list: Vec<_> = names.iter().take(12).collect();
        let more = names.len().saturating_sub(list.len());
        let more = if more > 0 { format!(" +{more} more") } else { String::new() };
        println!("  {m}: {}", {
            let mut s = list.iter().map(|n| n.as_str()).collect::<Vec<_>>().join(", ");
            s.push_str(&more);
            s
        });
    }
    0
}

/// `mimus claims` — print what the extractor produces; the tuning
/// surface for prose-claim precision.
fn claims_dump(cli: &Cli, all: bool) -> i32 {
    let cfg = match load(cli) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let root = root(cli);
    let mut claims = doc_claims(&cfg, &root);
    if all {
        for card in mimus_rules::matching_files(&root, ".kuda/modules/*.yaml")
                .into_iter()
                .chain(mimus_rules::matching_files(&root, ".kuda/files/**/*.yaml"))
            {
            claims.extend(card_prose_claims(&card));
        }
    }
    for c in &claims {
        println!(
            "{}:{} [{:?}] {}",
            c.source.display(),
            c.line,
            c.kind,
            c.text
        );
    }
    eprintln!("{} claim(s)", claims.len());
    0
}

/// Extract claims from configured doc sources and run checkers
/// + rules — the shared verification pipeline.
fn collect(cli: &Cli, semantic: bool, no_cmd: bool) -> Option<(PathBuf, Vec<Violation>)> {
    let cfg = match load(cli) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return None;
        }
    };
    let root = root(cli);
    let mut claims = doc_claims(&cfg, &root);
    // kind + prefix filters from [check] — tuning signal-to-noise
    // is the operator's call, not the extractor's
    claims.retain(|c| {
        (cfg.check.kinds.is_empty() || cfg.check.kinds.contains(&c.kind))
            && !cfg
                .check
                .ignore_prefixes
                .iter()
                .any(|p| c.text.starts_with(p.as_str()))
    });
    let mut fs = mimus_check_fs::FsChecker::new(&root);
    fs.base_dirs = cfg.check.base_dirs.clone();
    let mut checkers: Vec<Box<dyn Checker>> = vec![Box::new(fs)];
    if !no_cmd {
        let mut cmd = mimus_check_cmd::CmdChecker::new();
        cmd.declared = cfg.check.binaries.clone();
        checkers.push(Box::new(cmd));
    }
    if let Some(idx) = &cfg.check.kuda_index {
        checkers.push(Box::new(mimus_check_symbol::SymbolChecker::new(idx)));
    }
    // semantic tier: card prose -> behavior claims -> agent verify.
    // Advisory only (--all or [check].semantic); never gates.
    if semantic && cfg.check.semantic {
        if let Some(cmd) = &cfg.check.acp_cmd {
            for card in mimus_rules::matching_files(&root, ".kuda/modules/*.yaml")
                .into_iter()
                .chain(mimus_rules::matching_files(&root, ".kuda/files/**/*.yaml"))
            {
                claims.extend(card_prose_claims(&card));
            }
            let mut acp = mimus_check_acp::AcpChecker::new(cmd.clone(), root.clone());
            // interior path-ish tokens in prose claims honor the
            // same ignore list as whole-claim text
            acp.ignore_prefixes = cfg.check.ignore_prefixes.clone();
            checkers.push(Box::new(acp));
        }
    }
    let mut violations: Vec<Violation> = mimus_check::run(&checkers, &claims)
        .filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => {
                eprintln!("checker: {e}");
                None
            }
        })
        .collect();
    // data-driven rules
    if let Ok(rules) = mimus_rules::load_rules(&root.join(&cfg.rules.dir)) {
        for rule in &rules {
            for f in mimus_rules::matching_files(&root, &rule.files) {
                if let Ok(text) = std::fs::read_to_string(&f) {
                    violations.extend(mimus_rules::eval_rule(rule, &f, &text));
                }
            }
        }
    }
    Some((root, violations))
}

fn check(cli: &Cli, all: bool, no_cmd: bool) -> i32 {
    let Some((_root, violations)) = collect(cli, all, no_cmd) else {
        return 2;
    };
    emit(&violations, &cli.format);
    if violations.iter().any(|v| v.severity == Severity::Error) {
        1
    } else {
        0
    }
}

/// Card prose -> Behavior claims: `purpose:` and `notes:` fields
/// of .kuda module cards are agent/machine assertions — the
/// semantic checker can try to falsify them.
fn card_prose_claims(card: &Path) -> Vec<mimus_core::Claim> {
    let Ok(text) = std::fs::read_to_string(card) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut in_notes = false;
    for (i, line) in text.lines().enumerate() {
        let t = line.trim();
        if let Some(p) = t.strip_prefix("purpose:") {
            let p = p.trim().trim_matches('\'').trim_matches('"');
            if !p.is_empty() && p != "''" {
                out.push(mimus_core::Claim {
                    kind: mimus_core::ClaimKind::Behavior,
                    text: p.to_owned(),
                    source: card.to_path_buf(),
                    line: (i + 1) as u32,
                });
            }
            in_notes = false;
            continue;
        }
        if t.starts_with("notes:") {
            in_notes = true;
            continue;
        }
        if in_notes && (line.starts_with("  ") || line.starts_with("\t")) && !t.is_empty() {
            out.push(mimus_core::Claim {
                kind: mimus_core::ClaimKind::Behavior,
                text: t.trim_start_matches('-').trim().to_owned(),
                source: card.to_path_buf(),
                line: (i + 1) as u32,
            });
        } else if in_notes {
            in_notes = false;
        }
    }
    out
}

fn report(cli: &Cli) -> i32 {
    check(cli, true, false)
}

/// Apply mechanical rewrites for confirmed violations.
/// v1 scope: dead `path` claims where exactly one same-basename
/// file exists in the repo (a moved file) — everything else is
/// flagged for a human, never silently "fixed".
fn fix(cli: &Cli, dry: bool, semantic: bool) -> i32 {
    let cfg = load(cli).ok();
    let acp_cmd = cfg
        .as_ref()
        .and_then(|c| c.check.acp_cmd.clone());
    let Some((root, violations)) =
        collect(cli, semantic && acp_cmd.is_some(), false)
    else {
        return 2;
    };
    let files = repo_files(&root);
    let (mut applied, mut flagged) = (0usize, 0usize);
    for v in &violations {
        // agentic tier: refuted-with-evidence claim -> agent
        // proposes the corrected line (prose docs only)
        if semantic && v.severity == Severity::Warn && v.claim.kind == ClaimKind::Behavior {
            if let Some(cmd) = &acp_cmd {
                match mimus_check_acp::propose(cmd, &root, &v.claim.text, &v.evidence) {
                    Ok(new_line) => {
                        println!(
                            "{}:{} `{}` -> `{new_line}`{}",
                            v.claim.source.display(),
                            v.claim.line,
                            v.claim.text,
                            if dry { " (dry)" } else { "" }
                        );
                        if !dry {
                            if let Ok(text) = std::fs::read_to_string(root.join(&v.claim.source)) {
                                let patched = text.replacen(&v.claim.text, &new_line, 1);
                                let _ = std::fs::write(root.join(&v.claim.source), patched);
                            }
                        }
                        applied += 1;
                        continue;
                    }
                    Err(e) => eprintln!("propose {}:{}: {e}", v.claim.source.display(), v.claim.line),
                }
            }
        }
    }
    for v in &violations {
        let rewrite = match v.claim.kind {
            mimus_core::ClaimKind::Path => dead_path_candidate(&root, &files, &v.claim.text),
            _ => None,
        };
        match rewrite {
            Some(new) => {
                println!(
                    "{}:{} `{}` -> `{}`",
                    v.claim.source.display(),
                    v.claim.line,
                    v.claim.text,
                    new
                );
                if !dry {
                    rewrite_claim(&v.claim, &new);
                }
                applied += 1;
            }
            None => {
                flagged += 1;
                if dry {
                    println!(
                        "flagged {}:{} `{}` — {}",
                        v.claim.source.display(),
                        v.claim.line,
                        v.claim.text,
                        v.evidence
                    );
                }
            }
        }
    }
    println!("{applied} fixable{}, {flagged} flagged for human", if dry { " (dry)" } else { " applied" });
    0
}

/// All regular files under root (skips noise dirs).
fn repo_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if !matches!(
                    e.file_name().to_str(),
                    Some(".git" | "target" | "node_modules" | ".kuda")
                ) {
                    stack.push(p);
                }
            } else {
                out.push(p);
            }
        }
    }
    out
}

/// Dead path claim -> unique same-basename candidate, preferring a
/// matching parent dir name. Returns repo-relative path.
fn dead_path_candidate(root: &Path, files: &[PathBuf], claim: &str) -> Option<String> {
    let claim_p = Path::new(claim.trim_start_matches("./"));
    let base = claim_p.file_name()?;
    let parent = claim_p.parent()?.file_name().map(|f| f.to_string_lossy());
    let mut hits: Vec<&PathBuf> = files
        .iter()
        .filter(|f| f.file_name() == Some(base))
        .collect();
    // prefer the candidate whose parent dir name matches the claim's
    if hits.len() > 1 {
        if let Some(par) = &parent {
            let narrowed: Vec<_> = hits
                .iter()
                .filter(|f| {
                    f.parent()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy() == *par)
                        .unwrap_or(false)
                })
                .copied()
                .collect();
            if narrowed.len() == 1 {
                hits = narrowed;
            }
        }
    }
    if hits.len() != 1 {
        return None; // zero or ambiguous — human decides
    }
    hits[0]
        .strip_prefix(root)
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

/// Rewrite one claim occurrence inside its source line.
fn rewrite_claim(claim: &mimus_core::Claim, new: &str) {
    let path = &claim.source;
    let Ok(text) = std::fs::read_to_string(path) else { return };
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let idx = (claim.line as usize).saturating_sub(1);
    if let Some(line) = lines.get_mut(idx) {
        if line.contains(&claim.text) {
            *line = line.replacen(&claim.text, new, 1);
            let _ = std::fs::write(path, lines.join("\n") + "\n");
        }
    }
}

/// Trust-score `.kuda/` cards: verify files exist + provenance
/// hashes match current content. Advisory — reports, never gates.
fn audit(cli: &Cli) -> i32 {
    let root = root(cli);
    let cards = mimus_rules::matching_files(&root, ".kuda/modules/*.yaml");
    if cards.is_empty() {
        println!("no .kuda cards — run `kuda sync` first");
        return 0;
    }
    let (mut fresh, mut drifted, mut contradicted) = (0, 0, 0);
    for p in cards {
        let Ok(v) = serde_yaml::from_str::<serde_yaml::Value>(
            &std::fs::read_to_string(&p).unwrap_or_default(),
        ) else {
            continue;
        };
        let name = v["name"].as_str().unwrap_or("?").to_owned();
        let files: Vec<String> = v["provenance"]["files"]
            .as_sequence()
            .map(|s| s.iter().filter_map(|f| f.as_str().map(str::to_owned)).collect())
            .unwrap_or_default();
        let hashes: Vec<String> = v["provenance"]["hashes"]
            .as_sequence()
            .map(|s| s.iter().filter_map(|f| f.as_str().map(str::to_owned)).collect())
            .unwrap_or_default();
        let mut score = "fresh";
        for (f, h) in files.iter().zip(&hashes) {
            let fp = root.join(f);
            if !fp.exists() {
                score = "contradicted";
                break;
            }
            let cur = std::process::Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(["hash-object"])
                .arg(f)
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
                .unwrap_or_default();
            if cur != *h {
                score = "drifted";
            }
        }
        match score {
            "contradicted" => contradicted += 1,
            "drifted" => drifted += 1,
            _ => fresh += 1,
        }
        println!("{score:<12} {name}");
    }
    println!("{fresh} fresh, {drifted} drifted, {contradicted} contradicted");
    0
}

fn emit(violations: &[Violation], format: &str) {
    if format == "json" {
        println!("{}", serde_json::to_string_pretty(violations).unwrap_or_default());
        return;
    }
    if violations.is_empty() {
        println!("no violations — {} claims checked", "all");
        return;
    }
    for v in violations {
        println!(
            "{}:{} [{}] `{}` — {}{}",
            v.claim.source.display(),
            v.claim.line,
            match v.severity {
                Severity::Error => "error",
                Severity::Warn => "warn",
            },
            v.claim.text,
            v.evidence,
            v.fix.as_ref().map(|f| format!(" (fix: {f})")).unwrap_or_default(),
        );
    }
    println!("{} violation(s)", violations.len());
}

fn doctor(cli: &Cli) -> i32 {
    println!("mimus doctor");
    match load(cli) {
        Ok(c) => {
            println!(
                "  config   ok  (mechanical: {}, semantic: {}, rules: {})",
                c.check.mechanical,
                c.check.semantic,
                c.rules.dir.display()
            );
            // verifier-vs-writer diversity: the same agent that writes
            // prose auditing it shares biases. Warn when acp_cmd
            // matches the summarizer's (kuda.toml or workspace).
            if let Some(verify_cmd) = &c.check.acp_cmd {
                for cfg_path in ["kuda.toml", ".kuda/kuda.toml"] {
                    if let Ok(text) = std::fs::read_to_string(root(cli).join(cfg_path)) {
                        if text.contains("acp_cmd") && text.contains(verify_cmd.as_str()) {
                            println!(
                                "  diversity  !!  verifier acp_cmd matches the summarizer's — \
                                 same model writes and audits; use a different agent/model"
                            );
                        }
                    }
                }
            }
        }
        Err(e) => println!("  config   --  {e}"),
    }
    println!("  adapters    fs, cmd (symbol/llm pending kuda schema + provider)");
    // P6: live ACP probe — the agent path fails loudly here
    // rather than mid-check.
    if let Ok(c) = load(cli) {
        if let Some(cmd) = &c.check.acp_cmd {
            match mimus_check_acp::probe(cmd, &root(cli)) {
                Ok(msg) => println!("  acp        ok  {msg}"),
                Err(e) => println!("  acp        !!  {e}"),
            }
        }
    }
    0
}
