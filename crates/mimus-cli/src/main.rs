//! mimus — spots the cuckoo eggs in your repo. Composition root:
//! parses mimus.toml, wires checker adapters, runs subcommands.

use clap::{Parser, Subcommand};
use mimus_check::Checker;
use mimus_config::ConfigSource;
use mimus_config_toml::TomlSource;
use mimus_core::{Config, Severity, Violation};
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
        /// Never execute documented commands.
        #[arg(long)]
        no_cmd: bool,
    },
    /// Trust-score .kuda/ cards vs source.
    Audit,
    /// Full violations list with evidence.
    Report,
    /// Checker/config/kuda-index health.
    Doctor,
}

fn main() {
    let cli = Cli::parse();
    let code = match &cli.cmd {
        Cmd::Init => init(&cli),
        Cmd::Check { all, no_cmd } => check(&cli, *all, *no_cmd),
        Cmd::Report => report(&cli),
        Cmd::Audit => audit(&cli),
        Cmd::Doctor => doctor(&cli),
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

/// Extract claims from configured doc sources and run checkers.
fn check(cli: &Cli, _all: bool, _no_cmd: bool) -> i32 {
    let cfg = match load(cli) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let root = root(cli);
    let mut claims = Vec::new();
    for pat in &cfg.sources.docs {
        for p in mimus_rules::matching_files(&root, pat) {
            if let Ok(text) = std::fs::read_to_string(&p) {
                claims.extend(mimus_claims::extract(&text, &p));
            }
        }
    }
    let mut checkers: Vec<Box<dyn Checker>> = vec![
        Box::new(mimus_check_fs::FsChecker::new(&root)),
        Box::new(mimus_check_cmd::CmdChecker::new()),
    ];
    if let Some(idx) = &cfg.check.kuda_index {
        checkers.push(Box::new(mimus_check_symbol::SymbolChecker::new(idx)));
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
    emit(&violations, &cli.format);
    if violations.iter().any(|v| v.severity == Severity::Error) {
        1
    } else {
        0
    }
}

fn report(cli: &Cli) -> i32 {
    check(cli, true, false)
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
        Ok(c) => println!(
            "  config   ok  (mechanical: {}, semantic: {}, rules: {})",
            c.check.mechanical,
            c.check.semantic,
            c.rules.dir.display()
        ),
        Err(e) => println!("  config   --  {e}"),
    }
    println!("  adapters    fs, cmd (symbol/llm pending kuda schema + provider)");
    0
}
