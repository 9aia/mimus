//! Triple extraction + bucketed contradiction detection — the
//! phase-2 pure engine of ADR-0002. Prose claims reduce to
//! `(subject, relation, object)` triples with provenance; conflicts
//! fall out of a hash bucket keyed on the canonicalized
//! `(subject, relation)` pair instead of an agent call.
//!
//! Conservative by design (the ADR-0001 evidence bar): only
//! concrete-literal objects — an identifier, a number, or a
//! backtick/double-quoted path or string — produce a triple.
//! Anything fuzzy stays out of the bucket: evidence or silence.

use mimus_core::Claim;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One `(subject, relation, object)` fact with provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Triple {
    /// Raw subject text, canonicalized at bucket time.
    pub subject: String,
    /// Relation kind — `uses`, `is`, `has`, `requires`, `needs`,
    /// `depends_on`, `imports`, `located_in`, `count`.
    pub relation: String,
    /// Concrete-literal object text (quotes stripped).
    pub object: String,
    /// File the triple was extracted from.
    pub source: PathBuf,
    /// 1-based line in `source`.
    pub line: usize,
}

/// Whether a contradiction's evidence spans files or sits in one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContradictionKind {
    /// Conflicting objects asserted in different files — doc↔doc,
    /// code↔doc (ADR-0002 `contradiction` class).
    CrossSource,
    /// One file asserts two different objects for the same slot —
    /// the reflexive `self-contradiction` class.
    Reflexive,
}

/// A bucket with ≥2 distinct objects — a mechanical-grade conflict
/// with inspectable provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contradiction {
    /// The bucket key — `"{canonical subject}|{relation}"`. Every
    /// emitted conflict names the exact key it matched on.
    pub bucket: String,
    /// Distinct objects, each with first-seen `(source, line)`.
    pub objects: Vec<(String, PathBuf, usize)>,
    /// Cross-file evidence or a file contradicting itself.
    pub kind: ContradictionKind,
}

/// A duplicate-abstraction candidate — the same structural
/// fingerprint (shape id) seen at ≥2 distinct locations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateCandidate<F, L> {
    /// The shared fingerprint / shape id.
    pub fingerprint: F,
    /// Distinct locations, first-seen order.
    pub locations: Vec<L>,
}

/// `(phrase, relation)` — longest phrases first so "is defined in"
/// is never split by the shorter "is in"/"is" entries. Matched on
/// word boundaries against the lowered text; interior whitespace is
/// flexible ("depends  on" still matches).
const VERBS: &[(&str, &str)] = &[
    ("is implemented in", "located_in"),
    ("is defined in", "located_in"),
    ("is located in", "located_in"),
    ("depends on", "depends_on"),
    ("lives in", "located_in"),
    ("resides in", "located_in"),
    ("is in", "located_in"),
    ("requires", "requires"),
    ("imports", "imports"),
    ("needs", "needs"),
    ("uses", "uses"),
    ("has", "has"),
    ("is", "is"),
];

/// `` `path` owns|contains|implements|defines|hosts X `` — the
/// reverse ownership pattern: the quoted path claims X, so the
/// triple points X *at* the path (`X located_in path`) to collide
/// with forward `X lives in `path`` claims in the same bucket.
const OWNERS: &[&str] = &["owns", "contains", "implements", "defines", "hosts"];

/// Words that never carry bucket identity — articles and the
/// generic nouns every doc pads subjects with ("the auth service"
/// ≡ "auth"). Short on purpose: missing a stopword only splits a
/// bucket (a miss), never merges two (a false positive).
const STOPWORDS: &[&str] = &[
    "the",
    "a",
    "an",
    "of",
    "service",
    "module",
    "component",
    "system",
];

/// Identifier-shaped tokens that are still not concrete objects —
/// articles, negations, and the vague adjectives docs use where a
/// literal would go ("X is required", "X uses a custom thing").
const OBJECT_STOP: &[&str] = &[
    "a", "an", "the", "not", "no", "only", "just", "also", "very", "used", "required", "needed",
    "able", "meant", "intended",
];

/// Words that can follow a number without being a counted unit —
/// "4 of them", "3 in total" are not `N <units>` facts.
const UNIT_STOP: &[&str] = &[
    "of", "the", "a", "an", "to", "in", "on", "is", "and", "or", "at", "as", "be", "by", "it", "no",
];

/// Reduce claims to triples — prose patterns over claim text:
/// `X uses/is/has/requires/needs/depends on/imports Y`, `N <units>`
/// numeric facts, and quoted-path ownership. Fuzzy objects yield
/// no triple at all.
pub fn extract_triples(claims: &[Claim]) -> Vec<Triple> {
    let mut out = Vec::new();
    for c in claims {
        // byte-wise ASCII lowercase — same length as `text`, so byte
        // positions map back onto the original for slicing
        let lower: Vec<u8> = c.text.bytes().map(|b| b.to_ascii_lowercase()).collect();
        let spans = quoted_spans(&lower);
        verb_triples(
            &c.text,
            &lower,
            &spans,
            &c.source,
            c.line as usize,
            &mut out,
        );
        numeric_triples(
            &c.text,
            &lower,
            &spans,
            &c.source,
            c.line as usize,
            &mut out,
        );
        ownership_triples(
            &c.text,
            &lower,
            &spans,
            &c.source,
            c.line as usize,
            &mut out,
        );
    }
    out
}

/// `subject VERB object` — the subject is the clause tail before
/// the verb, the object is the literal right after it. Verbs inside
/// backtick spans are quoted code, not prose — skipped.
fn verb_triples(
    text: &str,
    lower: &[u8],
    spans: &[(usize, usize)],
    source: &Path,
    line: usize,
    out: &mut Vec<Triple>,
) {
    let mut i = 0;
    while i < lower.len() {
        if is_word_byte(lower[i]) && (i == 0 || !is_word_byte(lower[i - 1])) && !in_span(spans, i) {
            for &(phrase, relation) in VERBS {
                if let Some(end) = match_phrase(lower, i, phrase) {
                    if let Some(object) = object_after(text, end) {
                        if let Some(subject) = subject_before(&text[..i]) {
                            out.push(Triple {
                                subject,
                                relation: relation.to_owned(),
                                object,
                                source: source.to_path_buf(),
                                line,
                            });
                        }
                    }
                    i = end;
                    break;
                }
            }
        }
        i += 1;
    }
}

/// `N <units>` — a numeric fact reduced to `(unit, "count", N)` so
/// "4 workers" and "5 workers" collide in one bucket. Only counts
/// when the unit is a real word — "4 of them" stays silent.
fn numeric_triples(
    text: &str,
    lower: &[u8],
    spans: &[(usize, usize)],
    source: &Path,
    line: usize,
    out: &mut Vec<Triple>,
) {
    let mut i = 0;
    while i < lower.len() {
        if lower[i].is_ascii_digit()
            && (i == 0 || (!is_word_byte(lower[i - 1]) && lower[i - 1] != b'.'))
            && !in_span(spans, i)
        {
            let start = i;
            while i < lower.len() && (lower[i].is_ascii_digit() || lower[i] == b'.') {
                i += 1;
            }
            let mut j = i;
            while j < lower.len() && lower[j].is_ascii_whitespace() {
                j += 1;
            }
            let unit_start = j;
            while j < lower.len() && lower[j].is_ascii_alphabetic() {
                j += 1;
            }
            let unit = &text[unit_start..j];
            let unit_lower = unit.to_lowercase();
            if unit.len() >= 2 && !UNIT_STOP.contains(&unit_lower.as_str()) {
                out.push(Triple {
                    subject: unit.to_owned(),
                    relation: "count".to_owned(),
                    object: text[start..i].to_owned(),
                    source: source.to_path_buf(),
                    line,
                });
            }
            continue;
        }
        i += 1;
    }
}

/// `` `path` owns X `` — reverse ownership: the quoted span is the
/// object side, the identifier after the verb is the subject.
fn ownership_triples(
    text: &str,
    lower: &[u8],
    spans: &[(usize, usize)],
    source: &Path,
    line: usize,
    out: &mut Vec<Triple>,
) {
    for &(s, e) in spans {
        let mut j = e + 1; // skip the closing backtick
        while j < lower.len() && lower[j].is_ascii_whitespace() {
            j += 1;
        }
        for verb in OWNERS {
            if lower[j..].starts_with(verb.as_bytes())
                && (j + verb.len() >= lower.len() || !is_word_byte(lower[j + verb.len()]))
            {
                let mut k = j + verb.len();
                while k < lower.len() && lower[k].is_ascii_whitespace() {
                    k += 1;
                }
                let end = k + text[k..]
                    .find(|c: char| !is_object_char(c))
                    .unwrap_or(text.len() - k);
                if let Some(object) = concrete_object(&text[k..end]) {
                    out.push(Triple {
                        subject: object,
                        relation: "located_in".to_owned(),
                        object: text[s..e].to_owned(),
                        source: source.to_path_buf(),
                        line,
                    });
                }
                break;
            }
        }
    }
}

/// Is byte position `i` inside a backtick span?
fn in_span(spans: &[(usize, usize)], i: usize) -> bool {
    spans.iter().any(|&(s, e)| i >= s && i < e)
}

/// Match a (possibly multi-word) verb phrase at `pos`, on word
/// boundaries with flexible interior whitespace. Returns the byte
/// position just past the phrase on match — the consumed length can
/// exceed `phrase.len()` when whitespace runs long.
fn match_phrase(b: &[u8], pos: usize, phrase: &str) -> Option<usize> {
    let mut i = pos;
    for (n, word) in phrase.split(' ').enumerate() {
        if n > 0 {
            if i >= b.len() || !b[i].is_ascii_whitespace() {
                return None;
            }
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
        }
        if !b[i..].starts_with(word.as_bytes()) {
            return None;
        }
        i += word.len();
    }
    if i < b.len() && is_word_byte(b[i]) {
        return None;
    }
    Some(i)
}

/// The clause tail before a verb — back to the last sentence or
/// clause boundary, markdown noise stripped. Rejects empty,
/// over-long, or punctuation-only subjects.
fn subject_before(before: &str) -> Option<String> {
    let start = before
        .rfind(['.', ';', ':', '\n', '(', '[', '|'])
        .map_or(0, |p| p + 1);
    let s = before[start..]
        .trim()
        .trim_start_matches(['-', '*', '>', '#'])
        .trim();
    if s.is_empty() {
        return None;
    }
    if s.split_whitespace().count() > 6 {
        return None;
    }
    if !s.chars().any(|c| c.is_alphanumeric()) {
        return None;
    }
    Some(s.to_owned())
}

/// The literal object right after a verb — a backtick- or
/// double-quoted span, a number, or an identifier token. Anything
/// else is fuzzy: `None`.
fn object_after(text: &str, pos: usize) -> Option<String> {
    let rest = text[pos..].trim_start();
    let quote = *rest.as_bytes().first()?;
    if quote == b'`' || quote == b'"' {
        let inner = &rest[1..];
        let end = inner.find(char::from(quote))?;
        let obj = inner[..end].trim();
        if obj.is_empty() {
            return None;
        }
        return Some(obj.to_owned());
    }
    let end = rest
        .find(|c: char| !is_object_char(c))
        .unwrap_or(rest.len());
    concrete_object(&rest[..end])
}

/// Is `tok` a concrete literal — a number or an identifier-shaped
/// word that isn't a stopword?
fn concrete_object(tok: &str) -> Option<String> {
    let t = tok.trim_matches(['.', ':', '-']);
    if t.is_empty() || t.contains("..") {
        return None;
    }
    if t.bytes().any(|b| b.is_ascii_digit()) && t.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return Some(t.to_owned()); // numeric literal — "4", "1.5"
    }
    if t.len() < 2 || !t.bytes().any(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    if OBJECT_STOP.contains(&t.to_lowercase().as_str()) {
        return None;
    }
    Some(t.to_owned())
}

/// Backtick spans as `(inner_start, inner_end)` byte positions —
/// like `backtick_spans` in `lib.rs` but positioned, since the
/// extraction passes need to know where quoted code sits.
fn quoted_spans(b: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'`' {
            // a run of 2+ backticks is a code fence, not a span
            if b.get(i + 1) == Some(&b'`') {
                i += 1;
                while i < b.len() && b[i] == b'`' {
                    i += 1;
                }
                continue;
            }
            if let Some(off) = b[i + 1..].iter().position(|&c| c == b'`') {
                out.push((i + 1, i + 1 + off));
                i += off + 2;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn is_object_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ':' | '.')
}

/// Bucket-key normalization: lowercase, split on non-alphanumerics,
/// drop stopwords, stem plurals, then resolve the alias map
/// (`.mimus/aliases.toml` wiring lands with the caller — the map is
/// `(key, replacement)` pairs, keys normalized the same way).
/// Deterministic and inspectable: every emitted conflict can name
/// the exact bucket key it matched on.
pub fn canon_subject(raw: &str, aliases: &[(String, String)]) -> String {
    let canon = canon_no_alias(raw);
    for (k, v) in aliases {
        if canon_no_alias(k) == canon {
            return v.clone();
        }
    }
    canon
}

/// Lowercase → tokenize → stopword-strip → stem → join.
fn canon_no_alias(raw: &str) -> String {
    let mut toks = Vec::new();
    for tok in raw.split(|c: char| !c.is_ascii_alphanumeric()) {
        if tok.is_empty() {
            continue;
        }
        let t = stem(&tok.to_lowercase());
        if STOPWORDS.contains(&t.as_str()) {
            continue;
        }
        toks.push(t);
    }
    toks.join(" ")
}

/// Simple plural/inflection stem — `workers`→`worker`,
/// `policies`→`policy`, `classes`→`class`. Deliberately shallow:
/// `status`/`this`/`class` keep their final `s`.
fn stem(w: &str) -> String {
    let n = w.len();
    if n > 4 && w.ends_with("ies") {
        return format!("{}y", &w[..n - 3]);
    }
    if n > 4
        && (w.ends_with("sses")
            || w.ends_with("xes")
            || w.ends_with("ches")
            || w.ends_with("shes")
            || w.ends_with("zes"))
    {
        return w[..n - 2].to_owned();
    }
    if n > 3 && w.ends_with('s') && !w.ends_with("ss") && !w.ends_with("us") && !w.ends_with("is") {
        return w[..n - 1].to_owned();
    }
    w.to_owned()
}

/// Bucket triples by `(canonical subject, relation)` — groups with
/// ≥2 distinct objects are contradictions (ADR-0002's
/// `contradiction`/`self-contradiction` classes).
pub fn contradictions(triples: &[Triple]) -> Vec<Contradiction> {
    contradictions_with_aliases(triples, &[])
}

/// Same bucketing with an alias map applied at canonicalization —
/// `.mimus/aliases.toml` wiring lands with the caller.
pub fn contradictions_with_aliases(
    triples: &[Triple],
    aliases: &[(String, String)],
) -> Vec<Contradiction> {
    let mut buckets: BTreeMap<(String, &str), Vec<&Triple>> = BTreeMap::new();
    for t in triples {
        buckets
            .entry((canon_subject(&t.subject, aliases), t.relation.as_str()))
            .or_default()
            .push(t);
    }
    let mut out = Vec::new();
    for ((subject, relation), ts) in buckets {
        // one entry per distinct object: files asserting it + first line
        let mut objs: Vec<(String, Vec<&PathBuf>, usize)> = Vec::new();
        for t in &ts {
            if let Some((_, files, _)) = objs.iter_mut().find(|(o, _, _)| *o == t.object) {
                if !files.contains(&&t.source) {
                    files.push(&t.source);
                }
            } else {
                objs.push((t.object.clone(), vec![&t.source], t.line));
            }
        }
        if objs.len() < 2 {
            continue;
        }
        // reflexive when a single file asserts ≥2 distinct objects —
        // otherwise the disagreement lives across files
        let mut by_file: Vec<(&PathBuf, Vec<&String>)> = Vec::new();
        for (obj, files, _) in &objs {
            for f in files {
                match by_file.iter_mut().find(|(bf, _)| bf == f) {
                    Some((_, os)) => os.push(obj),
                    None => by_file.push((*f, vec![obj])),
                }
            }
        }
        let kind = if by_file.iter().any(|(_, os)| os.len() >= 2) {
            ContradictionKind::Reflexive
        } else {
            ContradictionKind::CrossSource
        };
        out.push(Contradiction {
            bucket: format!("{subject}|{relation}"),
            objects: objs
                .into_iter()
                .map(|(o, files, l)| (o, files.first().copied().cloned().unwrap_or_default(), l))
                .collect(),
            kind,
        });
    }
    out
}

/// Duplicate-abstraction candidates: group `(fingerprint, location)`
/// pairs by fingerprint — ids seen at ≥2 *distinct* locations are
/// candidates. The phase-1 index supplies real shape ids; the pass
/// itself is generic over id and location types.
pub fn same_fingerprint<F, L>(groups: &[(F, L)]) -> Vec<DuplicateCandidate<F, L>>
where
    F: Eq + Clone,
    L: Eq + Clone,
{
    let mut by_fp: Vec<(F, Vec<L>)> = Vec::new();
    for (fp, loc) in groups {
        match by_fp.iter_mut().find(|(f, _)| f == fp) {
            Some((_, locs)) => {
                if !locs.contains(loc) {
                    locs.push(loc.clone());
                }
            }
            None => by_fp.push((fp.clone(), vec![loc.clone()])),
        }
    }
    by_fp
        .into_iter()
        .filter(|(_, locs)| locs.len() >= 2)
        .map(|(fingerprint, locations)| DuplicateCandidate {
            fingerprint,
            locations,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mimus_core::ClaimKind;

    fn claim(text: &str, src: &str, line: u32) -> Claim {
        Claim {
            kind: ClaimKind::Behavior,
            text: text.to_owned(),
            source: Path::new(src).to_path_buf(),
            line,
        }
    }

    fn triples(texts: &[&str]) -> Vec<Triple> {
        let claims: Vec<_> = texts.iter().map(|t| claim(t, "d.md", 1)).collect();
        extract_triples(&claims)
    }

    #[test]
    fn verb_relations_extract() {
        let ts = triples(&[
            "auth uses jwt",
            "the auth service is jwt",
            "store has sqlite",
            "api requires auth",
            "svc needs redis",
            "web depends on api",
            "web imports api_core",
        ]);
        let got: Vec<(&str, &str, &str)> = ts
            .iter()
            .map(|t| (t.subject.as_str(), t.relation.as_str(), t.object.as_str()))
            .collect();
        assert!(got.contains(&("auth", "uses", "jwt")), "{got:?}");
        assert!(got.contains(&("the auth service", "is", "jwt")), "{got:?}");
        assert!(got.contains(&("store", "has", "sqlite")), "{got:?}");
        assert!(got.contains(&("api", "requires", "auth")), "{got:?}");
        assert!(got.contains(&("svc", "needs", "redis")), "{got:?}");
        assert!(got.contains(&("web", "depends_on", "api")), "{got:?}");
        assert!(got.contains(&("web", "imports", "api_core")), "{got:?}");
    }

    #[test]
    fn located_in_family_extracts() {
        for text in [
            "auth lives in `src/auth.rs`",
            "auth is defined in `src/auth.rs`",
            "auth is implemented in `src/auth.rs`",
            "auth is located in `src/auth.rs`",
            "auth resides in `src/auth.rs`",
            "auth is in `src/auth.rs`",
        ] {
            let ts = triples(&[text]);
            assert_eq!(ts.len(), 1, "{text}");
            assert_eq!(ts[0].relation, "located_in", "{text}");
            assert_eq!(ts[0].object, "src/auth.rs", "{text}");
        }
        // flexible interior whitespace still matches
        let ts = triples(&["auth lives   in `src/auth.rs`"]);
        assert_eq!(ts[0].object, "src/auth.rs");
    }

    #[test]
    fn phrase_boundaries_hold() {
        // no word-boundary after phrase → not a verb
        assert!(triples(&["dependson api"]).is_empty());
        assert!(triples(&["dependents on api"]).is_empty());
        // "is" must not match inside a longer word — `isx` stays a
        // subject, not a verb
        let ts = triples(&["isx uses jwt"]);
        assert_eq!(ts.len(), 1, "{ts:?}");
        assert_eq!(ts[0].subject, "isx");
        assert_eq!(ts[0].relation, "uses");
        assert!(triples(&["users hate jwt"]).is_empty());
        // verb at end of text → no object → silence
        assert!(triples(&["the auth service uses"]).is_empty());
    }

    #[test]
    fn subject_gates() {
        // empty subject (verb at text start)
        assert!(triples(&["uses jwt"]).is_empty());
        // over-long subject — a whole clause, not a slot name
        assert!(triples(&["one two three four five six seven uses jwt"]).is_empty());
        // punctuation-only subject
        assert!(triples(&["!!! uses jwt"]).is_empty());
        // clause boundary cuts the subject tail
        let ts = triples(&["note: auth uses jwt"]);
        assert_eq!(ts[0].subject, "auth");
        // bullet markers strip
        let ts = triples(&["- store uses sqlite"]);
        assert_eq!(ts[0].subject, "store");
        // sentence boundary: only the tail clause is the subject
        let ts = triples(&["preamble text. store uses sqlite"]);
        assert_eq!(ts[0].subject, "store");
    }

    #[test]
    fn object_literal_gates() {
        // quoted objects — backtick and double-quote
        assert_eq!(triples(&["auth uses `jwt-lib`"])[0].object, "jwt-lib");
        assert_eq!(triples(&["auth uses \"jwt-lib\""])[0].object, "jwt-lib");
        // unterminated quote → silence
        assert!(triples(&["auth uses `jwt"]).is_empty());
        // empty quoted span → silence
        assert!(triples(&["auth uses ``"]).is_empty());
        // number object
        assert_eq!(triples(&["pool uses 42"])[0].object, "42");
        // single-char / stopword / non-alpha / `..` objects → silence
        assert!(triples(&["pool uses x"]).is_empty());
        assert!(triples(&["pool uses the thing"]).is_empty());
        assert!(triples(&["pool uses ___"]).is_empty());
        assert!(triples(&["pool uses foo..bar"]).is_empty());
        // trailing punctuation trims off the literal
        assert_eq!(triples(&["pool uses jwt."])[0].object, "jwt");
        // crate-path identifiers keep `::`
        assert_eq!(triples(&["pool uses mimus::core"])[0].object, "mimus::core");
    }

    #[test]
    fn numeric_facts_extract() {
        let ts = triples(&["pool runs 4 workers"]);
        assert!(
            ts.iter()
                .any(|t| t.relation == "count" && t.subject == "workers" && t.object == "4"),
            "{ts:?}"
        );
        // unit stopwords and one-letter units stay silent
        let ts = triples(&["4 of them shipped"]);
        assert!(!ts.iter().any(|t| t.relation == "count"), "{ts:?}");
        let ts = triples(&["rated 4 x faster"]);
        assert!(!ts.iter().any(|t| t.relation == "count"), "{ts:?}");
        // digit inside a word or after '.' is not a count start
        let ts = triples(&["the utf8 encoding"]);
        assert!(!ts.iter().any(|t| t.relation == "count"), "{ts:?}");
        let ts = triples(&["version x.5 ships"]);
        assert!(!ts.iter().any(|t| t.relation == "count"), "{ts:?}");
        // number at end of text — no unit follows
        let ts = triples(&["the limit is 99"]);
        assert!(!ts.iter().any(|t| t.relation == "count"), "{ts:?}");
        // numbers inside quoted code spans are code, not facts
        let ts = triples(&["the loop runs `4 passes` here"]);
        assert!(!ts.iter().any(|t| t.relation == "count"), "{ts:?}");
    }

    #[test]
    fn quoted_code_is_not_a_fact() {
        // a verb inside a backtick span emits nothing
        assert!(triples(&["the `foo uses bar` example"]).is_empty());
        // and a path-shaped span doesn't leak an `is` verb
        assert!(triples(&["`src/is.rs` exists today"]).is_empty());
        // but a backticked subject still canonicalizes
        let ts = triples(&["`auth` uses jwt"]);
        assert_eq!(ts[0].subject, "`auth`");
        assert_eq!(ts[0].object, "jwt");
    }

    #[test]
    fn ownership_extracts_reversed() {
        // `path` owns X → X located_in path
        for text in [
            "`src/auth.rs` owns auth",
            "`src/auth.rs` contains auth",
            "`src/auth.rs` implements auth",
            "`src/auth.rs` defines auth",
            "`src/auth.rs` hosts auth",
        ] {
            let ts = triples(&[text]);
            assert_eq!(ts.len(), 1, "{text}");
            assert_eq!(ts[0].subject, "auth", "{text}");
            assert_eq!(ts[0].relation, "located_in", "{text}");
            assert_eq!(ts[0].object, "src/auth.rs", "{text}");
        }
        // non-ownership tail → no located_in triple (a plain `is`
        // claim on the path is still legitimately extracted)
        let ts = triples(&["`src/a.rs` is old"]);
        assert!(!ts.iter().any(|t| t.relation == "located_in"), "{ts:?}");
        // ownership verb but fuzzy subject → silence
        assert!(triples(&["`src/a.rs` owns the stuff"]).is_empty());
        // ownership verb at end → no object → silence
        assert!(triples(&["`src/a.rs` owns"]).is_empty());
        // verb-prefix without boundary (`containsx`) → no match
        assert!(triples(&["`src/a.rs` containsx auth"]).is_empty());
        // backtick run (``fence``) is not a span
        assert!(triples(&["``src/a.rs`` owns auth"]).is_empty());
        // unterminated backtick → no span
        assert!(triples(&["`src/a.rs owns auth"]).is_empty());
    }

    #[test]
    fn canon_normalizes_subjects() {
        let no_alias: [(String, String); 0] = [];
        assert_eq!(canon_subject("The Auth Service", &no_alias), "auth");
        assert_eq!(canon_subject("workers", &no_alias), "worker");
        assert_eq!(canon_subject("policies", &no_alias), "policy");
        assert_eq!(canon_subject("classes", &no_alias), "class");
        // ss/us/is endings keep their final s
        assert_eq!(canon_subject("status", &no_alias), "status");
        assert_eq!(canon_subject("this", &no_alias), "this");
        assert_eq!(canon_subject("glass", &no_alias), "glass");
        // short `s` words don't stem
        assert_eq!(canon_subject("has", &no_alias), "has");
        // punctuation/backticks are token boundaries
        assert_eq!(canon_subject("the `auth` module", &no_alias), "auth");
        // all-stopword subject → empty bucket key
        assert_eq!(canon_subject("the service module", &no_alias), "");
        // aliases resolve on the canonical form, keys normalized too
        let aliases = vec![("Auth SVC".to_owned(), "auth".to_owned())];
        assert_eq!(canon_subject("auth svc", &aliases), "auth");
        assert_eq!(canon_subject("other", &aliases), "other");
    }

    #[test]
    fn contradictions_bucket_and_flag() {
        // empty in → empty out
        assert!(contradictions(&[]).is_empty());
        // same object asserted twice → no conflict (dedup path)
        let same = triples(&["auth uses jwt", "the auth service uses jwt"]);
        assert!(contradictions(&same).is_empty(), "{same:?}");
        // same file, two objects → reflexive self-contradiction
        let claims = vec![
            claim("auth uses jwt", "a.md", 1),
            claim("auth uses tokens", "a.md", 2),
        ];
        let cs = contradictions(&extract_triples(&claims));
        assert_eq!(cs.len(), 1, "{cs:?}");
        assert_eq!(cs[0].kind, ContradictionKind::Reflexive);
        assert_eq!(cs[0].bucket, "auth|uses");
        assert_eq!(cs[0].objects.len(), 2);
        // different files → cross-source
        let claims = vec![
            claim("auth uses jwt", "a.md", 1),
            claim("auth uses tokens", "b.md", 9),
        ];
        let cs = contradictions(&extract_triples(&claims));
        assert_eq!(cs.len(), 1, "{cs:?}");
        assert_eq!(cs[0].kind, ContradictionKind::CrossSource);
        assert_eq!(cs[0].objects[0].1, PathBuf::from("a.md"));
        assert_eq!(cs[0].objects[1].1, PathBuf::from("b.md"));
        assert_eq!(cs[0].objects[1].2, 9);
        // one file asserting both objects is reflexive even when
        // another file repeats one of them
        let claims = vec![
            claim("auth uses jwt", "a.md", 1),
            claim("auth uses tokens", "a.md", 2),
            claim("auth uses jwt", "b.md", 3),
        ];
        let cs = contradictions(&extract_triples(&claims));
        assert_eq!(cs[0].kind, ContradictionKind::Reflexive, "{cs:?}");
        // different relations don't collide
        let claims = vec![
            claim("auth uses jwt", "a.md", 1),
            claim("auth is jwt", "b.md", 2),
        ];
        assert!(contradictions(&extract_triples(&claims)).is_empty());
    }

    #[test]
    fn aliases_merge_buckets() {
        let claims = vec![
            claim("auth svc uses jwt", "a.md", 1),
            claim("the auth service uses tokens", "b.md", 2),
        ];
        let ts = extract_triples(&claims);
        // without aliases the two subjects bucket apart → silence
        assert!(contradictions(&ts).is_empty());
        let aliases = vec![("auth svc".to_owned(), "auth".to_owned())];
        let cs = contradictions_with_aliases(&ts, &aliases);
        assert_eq!(cs.len(), 1, "{cs:?}");
        assert_eq!(cs[0].bucket, "auth|uses");
    }

    #[test]
    fn fingerprint_pass_finds_duplicates() {
        // empty in → empty out
        assert!(same_fingerprint::<u64, &str>(&[]).is_empty());
        // same id, same location → deduped, not a candidate
        let g = same_fingerprint(&[(7u64, "a.rs:10"), (7u64, "a.rs:10")]);
        assert!(g.is_empty(), "{g:?}");
        // same id, distinct locations → candidate
        let g = same_fingerprint(&[(7u64, "a.rs:10"), (7u64, "b.rs:22"), (9u64, "c.rs:1")]);
        assert_eq!(g.len(), 1, "{g:?}");
        assert_eq!(g[0].fingerprint, 7);
        assert_eq!(g[0].locations, vec!["a.rs:10", "b.rs:22"]);
        // multiple colliding ids come out separately
        let g = same_fingerprint(&[
            ("fp-a", "x.rs:1"),
            ("fp-b", "y.rs:2"),
            ("fp-a", "z.rs:3"),
            ("fp-b", "w.rs:4"),
        ]);
        assert_eq!(g.len(), 2, "{g:?}");
        // Debug/Clone/Eq coverage on the public types
        let c = Contradiction {
            bucket: "s|r".into(),
            objects: vec![],
            kind: ContradictionKind::CrossSource,
        };
        assert!(format!("{:?}", c.clone()).contains("bucket"));
        assert_eq!(c.kind, ContradictionKind::CrossSource);
        assert_ne!(c.kind, ContradictionKind::Reflexive);
    }
}
