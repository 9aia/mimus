//! Per-file triples cache — ADR-0002 phase 3. `extract_triples`
//! is pure over claims, but claims derive from source files — so
//! the cache sits one level up: per source file, keyed on
//! `(path, content hash, TRIPLES_VERSION)`. Same bytes + same
//! extractor version → sqlite rows instead of re-extraction; an
//! edit changes the hash → the file's rows are rewritten.
//!
//! sqlite at `.mimus/triples.db` — repo-local, same open pattern
//! as the kuda index readers. A `files` marker row is the key so
//! a file yielding *zero* triples still caches (the marker hits,
//! the triples table simply has no rows for it).

use mimus_claims::triples::{extract_triples, Triple};
use mimus_core::Claim;
use rusqlite::{params, Connection, OpenFlags};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Extractor version — bump on any `mimus-claims::triples`
/// change. Rows written by an older version just miss; they are
/// never wrong.
pub const TRIPLES_VERSION: u32 = 1;

/// `files(path, hash, version)` is the presence key;
/// `triples(path, …)` the payload, replaced wholesale on a miss.
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS files(
    path TEXT PRIMARY KEY,
    hash TEXT NOT NULL,
    version INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS triples(
    path TEXT NOT NULL,
    subject TEXT NOT NULL,
    relation TEXT NOT NULL,
    object TEXT NOT NULL,
    line INTEGER NOT NULL
);";

/// sqlite-backed store — one `Connection`, opened (creating) by
/// the caller, so the same cache serves a whole `check` run.
pub struct TriplesCache {
    conn: Connection,
}

impl TriplesCache {
    /// Open the cache at `db`, creating it (and the `.mimus/`
    /// dir) when absent.
    pub fn open(db: &Path) -> Result<Self, String> {
        if let Some(dir) = db.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let conn = Connection::open(db).map_err(|e| e.to_string())?;
        conn.execute_batch(SCHEMA).map_err(|e| e.to_string())?;
        Ok(Self { conn })
    }

    /// Triples for one source file: hash its bytes, then hit →
    /// the stored rows or miss → `extract_triples` + a wholesale
    /// rewrite of the file's rows. An unreadable file skips the
    /// cache entirely — no bytes means no trustworthy key.
    pub fn triples_for(&mut self, file: &Path, claims: &[Claim]) -> Vec<Triple> {
        let Ok(bytes) = std::fs::read(file) else {
            return extract_triples(claims);
        };
        let mut h = DefaultHasher::new();
        bytes.hash(&mut h);
        let hash = format!("{:016x}", h.finish());
        let key = file.to_string_lossy().into_owned();
        if let Some(ts) = self.lookup(&key, &hash, file) {
            return ts;
        }
        let ts = extract_triples(claims);
        // a store failure never fails the run — the cache is a
        // speedup, not a correctness dependency
        let _ = self.store(&key, &hash, &ts);
        ts
    }

    /// `Some` only when the stored `(hash, version)` matches —
    /// any sqlite error degrades to a miss (re-extract), never a
    /// wrong hit.
    fn lookup(&self, key: &str, hash: &str, file: &Path) -> Option<Vec<Triple>> {
        let (h, v): (String, i64) = self
            .conn
            .query_row(
                "SELECT hash,version FROM files WHERE path = ?1",
                params![key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok()?;
        if h != hash || v != i64::from(TRIPLES_VERSION) {
            return None;
        }
        let mut st = self
            .conn
            .prepare(
                "SELECT subject,relation,object,line FROM triples \
                 WHERE path = ?1 ORDER BY rowid",
            )
            .ok()?;
        let rows = st
            .query_map(params![key], |r| {
                Ok(Triple {
                    subject: r.get(0)?,
                    relation: r.get(1)?,
                    object: r.get(2)?,
                    // the group's own PathBuf — identical to what
                    // live extraction would stamp
                    source: file.to_path_buf(),
                    line: r.get::<_, i64>(3)? as usize,
                })
            })
            .ok()?;
        // row-level decode failures drop the row, same flatten()
        // posture as the index readers
        Some(rows.flatten().collect())
    }

    /// Rewrite `key`'s rows in one transaction — the `files`
    /// marker goes in last, so a mid-write crash can't leave a
    /// fresh marker over partial triples.
    fn store(&mut self, key: &str, hash: &str, ts: &[Triple]) -> Result<(), rusqlite::Error> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM files WHERE path = ?1", params![key])?;
        tx.execute("DELETE FROM triples WHERE path = ?1", params![key])?;
        {
            let mut st = tx.prepare("INSERT INTO triples VALUES(?1,?2,?3,?4,?5)")?;
            for t in ts {
                st.execute(params![key, t.subject, t.relation, t.object, t.line as i64])?;
            }
        }
        tx.execute(
            "INSERT INTO files VALUES(?1,?2,?3)",
            params![key, hash, TRIPLES_VERSION],
        )?;
        tx.commit()
    }

    /// Total cached triple rows — the number `mimus doctor`
    /// reports.
    pub fn row_count(&self) -> u64 {
        self.conn
            .query_row("SELECT COUNT(*) FROM triples", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0)
            .max(0) as u64
    }
}

/// Claims → triples through the cache. Claims group by `source`
/// — each file hashes its bytes once: hit → stored rows, miss →
/// extract that file's claims and rewrite its rows. First-seen
/// file order is preserved so output stays deterministic.
pub fn extract_triples_cached(claims: &[Claim], cache: &mut TriplesCache) -> Vec<Triple> {
    let mut order: Vec<PathBuf> = Vec::new();
    let mut by_file: HashMap<PathBuf, Vec<Claim>> = HashMap::new();
    for c in claims {
        match by_file.get_mut(&c.source) {
            Some(v) => v.push(c.clone()),
            None => {
                order.push(c.source.clone());
                by_file.insert(c.source.clone(), vec![c.clone()]);
            }
        }
    }
    let mut out = Vec::new();
    for f in order {
        out.extend(cache.triples_for(&f, &by_file[&f]));
    }
    out
}

/// Row count for `mimus doctor` — `None` when the db is missing
/// or unreadable. Read-only open: doctor never creates the file.
pub fn triples_cache_rows(db: &Path) -> Option<u64> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    conn.query_row("SELECT COUNT(*) FROM triples", [], |r| r.get::<_, i64>(0))
        .ok()
        .map(|n| n.max(0) as u64)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use mimus_core::ClaimKind;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mimus-tcache-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn claim(text: &str, src: &Path, line: u32) -> Claim {
        Claim {
            kind: ClaimKind::Behavior,
            text: text.to_owned(),
            source: src.to_path_buf(),
            line,
        }
    }

    /// Miss → extract + store; hit → the *stored* rows, proven by
    /// planting a sentinel row the extractor would never produce.
    #[test]
    fn miss_extracts_hit_reads_rows() {
        let d = tmp("hit");
        let f = d.join("a.md");
        std::fs::write(&f, "auth uses jwt\n").unwrap();
        let db = d.join("triples.db");
        let mut c = TriplesCache::open(&db).unwrap();
        assert_eq!(c.row_count(), 0);
        let ts = c.triples_for(&f, &[claim("auth uses jwt", &f, 1)]);
        assert_eq!(ts.len(), 1);
        assert_eq!(ts[0].subject, "auth");
        assert_eq!(c.row_count(), 1);
        // plant a sentinel — a hit returns *stored* rows, so the
        // next call must yield the sentinel, not re-extraction
        c.conn
            .execute(
                "UPDATE triples SET object = 'sentinel-obj' WHERE path = ?1",
                params![f.to_string_lossy().as_ref()],
            )
            .unwrap();
        let ts = c.triples_for(&f, &[claim("auth uses jwt", &f, 1)]);
        assert_eq!(
            ts[0].object, "sentinel-obj",
            "cache hit returns stored rows"
        );
        assert_eq!(ts[0].source, f);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Editing a file changes the content hash → miss → its rows
    /// are replaced wholesale; other files' rows are untouched.
    #[test]
    fn edit_invalidates_only_that_file() {
        let d = tmp("edit");
        let a = d.join("a.md");
        let b = d.join("b.md");
        std::fs::write(&a, "auth uses jwt\n").unwrap();
        std::fs::write(&b, "store has sqlite\n").unwrap();
        let mut c = TriplesCache::open(&d.join("triples.db")).unwrap();
        c.triples_for(&a, &[claim("auth uses jwt", &a, 1)]);
        c.triples_for(&b, &[claim("store has sqlite", &b, 1)]);
        assert_eq!(c.row_count(), 2);
        // edit a.md — new bytes → new hash → re-extract
        std::fs::write(&a, "auth uses tokens\n").unwrap();
        let ts = c.triples_for(&a, &[claim("auth uses tokens", &a, 1)]);
        assert_eq!(ts[0].object, "tokens");
        // the stale 'jwt' row is gone — wholesale replace
        let stale: i64 = c
            .conn
            .query_row(
                "SELECT COUNT(*) FROM triples WHERE path = ?1 AND object = 'jwt'",
                params![a.to_string_lossy().as_ref()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stale, 0);
        assert_eq!(c.row_count(), 2, "b.md's row survived the rewrite");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A file yielding zero triples still caches — the `files`
    /// marker hits, so re-running feeds no claims back through
    /// the extractor.
    #[test]
    fn zero_triple_file_still_caches() {
        let d = tmp("zero");
        let f = d.join("a.md");
        std::fs::write(&f, "plain prose, no literals\n").unwrap();
        let mut c = TriplesCache::open(&d.join("triples.db")).unwrap();
        assert!(c
            .triples_for(&f, &[claim("plain prose, no literals", &f, 1)])
            .is_empty());
        // second call: if it hit the extractor again with a
        // triple-bearing claim we'd see rows — the marker hit
        // returns the (empty) stored set instead
        let ts = c.triples_for(&f, &[claim("auth uses jwt", &f, 1)]);
        assert!(
            ts.is_empty(),
            "zero-triple files hit the marker, not the extractor"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A `files` row at an older TRIPLES_VERSION misses — the
    /// extractor ran differently then, so its rows can't be
    /// trusted.
    #[test]
    fn version_bump_forces_reextract() {
        let d = tmp("ver");
        let f = d.join("a.md");
        std::fs::write(&f, "auth uses jwt\n").unwrap();
        let mut c = TriplesCache::open(&d.join("triples.db")).unwrap();
        c.triples_for(&f, &[claim("auth uses jwt", &f, 1)]);
        // simulate a stale-version marker: same hash, old version
        c.conn
            .execute(
                "UPDATE files SET version = ?2 WHERE path = ?1",
                params![f.to_string_lossy().as_ref(), i64::from(TRIPLES_VERSION) - 1],
            )
            .unwrap();
        // re-extracts — and the marker upgrades to the current
        // version, so a third call hits again
        let ts = c.triples_for(&f, &[claim("auth uses jwt", &f, 1)]);
        assert_eq!(ts[0].object, "jwt");
        c.conn
            .execute(
                "UPDATE triples SET object = 'v-sentinel' WHERE path = ?1",
                params![f.to_string_lossy().as_ref()],
            )
            .unwrap();
        let ts = c.triples_for(&f, &[claim("auth uses jwt", &f, 1)]);
        assert_eq!(
            ts[0].object, "v-sentinel",
            "marker re-armed at the new version"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Claims from several files group per source — each file
    /// hashes + extracts once, triples carry the right file.
    #[test]
    fn cached_extract_groups_by_source() {
        let d = tmp("group");
        let a = d.join("a.md");
        let b = d.join("b.md");
        std::fs::write(&a, "auth uses jwt\n").unwrap();
        std::fs::write(&b, "pool runs 4 workers\n").unwrap();
        let mut c = TriplesCache::open(&d.join("triples.db")).unwrap();
        let claims = [
            claim("auth uses jwt", &a, 1),
            claim("pool runs 4 workers", &b, 1),
            claim("more prose here", &a, 2),
        ];
        let ts = extract_triples_cached(&claims, &mut c);
        assert_eq!(ts.len(), 2, "{ts:?}");
        assert_eq!(ts[0].source, a);
        assert_eq!(ts[1].source, b);
        // a second pass hits the cache for both files
        let ts2 = extract_triples_cached(&claims, &mut c);
        assert_eq!(ts, ts2);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A claim whose source can't be read extracts live — no
    /// bytes means no hash, and nothing is written to the cache.
    #[test]
    fn unreadable_source_skips_cache() {
        let d = tmp("unread");
        let f = d.join("gone.md");
        let mut c = TriplesCache::open(&d.join("triples.db")).unwrap();
        let ts = c.triples_for(&f, &[claim("auth uses jwt", &f, 1)]);
        assert_eq!(ts.len(), 1);
        assert_eq!(c.row_count(), 0, "nothing cached for an unhashable file");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// `open` surfaces failures (dir uncreateable); the read-only
    /// doctor probe reports rows or `None`, never creates a db.
    #[test]
    fn open_and_doctor_probe_edges() {
        let d = tmp("edges");
        // .mimus as a *file* → create_dir_all fails
        let blocker = d.join("blocked");
        std::fs::write(&blocker, "x").unwrap();
        assert!(TriplesCache::open(&blocker.join("triples.db")).is_err());
        // parentless path → no dir to create, open fails
        assert!(TriplesCache::open(Path::new("/")).is_err());
        // missing db → None, and the probe doesn't create it
        let db = d.join("triples.db");
        assert!(triples_cache_rows(&db).is_none());
        assert!(!db.exists());
        // populated db → row count
        let mut c = TriplesCache::open(&db).unwrap();
        let f = d.join("a.md");
        std::fs::write(&f, "auth uses jwt\n").unwrap();
        c.triples_for(&f, &[claim("auth uses jwt", &f, 1)]);
        drop(c);
        assert_eq!(triples_cache_rows(&db), Some(1));
        // a non-sqlite file → probe fails → None
        std::fs::write(&db, "not a database").unwrap();
        assert!(triples_cache_rows(&db).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
