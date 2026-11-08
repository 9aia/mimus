//! Secret port — named-secret resolution from a backing store.
//! Impls: `mimus-keyring` (OS keyring), `mimus-dotenv` (KEY=VAL
//! file), `mimus-cuora` (cuora vaults via the `cuora` CLI),
//! [`EnvSecrets`] (process env — always first in the chain).

use std::io;

/// A secret store. `get` returns the value or None (missing is not
/// an error — the caller tries the next store in the chain).
pub trait Secrets: Send + Sync {
    /// Store name — for `doctor` health lines and error messages.
    fn name(&self) -> &str;
    /// Cheap availability probe (binary on PATH, file readable).
    /// `false` means `get` will always miss — the chain still
    /// tries it; this is for `doctor` to report, not to gate on.
    fn available(&self) -> bool {
        true
    }
    /// Look up a secret by name; None means "not here" — the
    /// caller tries the next store, not an error.
    fn get(&self, name: &str) -> Option<String>;
    /// Persist a value into this store (deploy-time migration).
    /// Stores that can't write return an error.
    fn set(&self, name: &str, value: &str) -> io::Result<()>;
}

/// Process-env adapter — the chain's first store. Read-only:
/// `set` always errors, which hands the write to the next store.
pub struct EnvSecrets;

impl Secrets for EnvSecrets {
    fn name(&self) -> &str {
        "env"
    }
    fn get(&self, name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|v| !v.is_empty())
    }
    fn set(&self, _name: &str, _value: &str) -> io::Result<()> {
        Err(io::Error::other(
            "env vars are read-only — can't persist to the process env",
        ))
    }
}

/// Ordered store chain — `get` returns the first hit, `set` writes
/// to the first store that accepts. Composition lives at the
/// binary's root (mimus-cli `secrets()`); the chain itself just
/// walks the order it's given.
///
/// Target order (cuora migration): `env → cuora → keyring → dotenv`
/// — cuora vaults are the canonical store; keyring and the env
/// file are legacy read-throughs kept during the transition.
/// Writes land in cuora because `env` refuses `set`.
pub struct Chain {
    stores: Vec<Box<dyn Secrets>>,
}

impl Chain {
    pub fn new(stores: Vec<Box<dyn Secrets>>) -> Self {
        Self { stores }
    }
    /// The composed stores, in order — `doctor` reads this to
    /// report each store's availability.
    pub fn stores(&self) -> &[Box<dyn Secrets>] {
        &self.stores
    }
}

impl Secrets for Chain {
    fn name(&self) -> &str {
        "chain"
    }
    fn get(&self, name: &str) -> Option<String> {
        self.stores.iter().find_map(|s| s.get(name))
    }
    fn set(&self, name: &str, value: &str) -> io::Result<()> {
        let mut errs = Vec::new();
        for s in &self.stores {
            match s.set(name, value) {
                Ok(()) => return Ok(()),
                Err(e) => errs.push(format!("{}: {e}", s.name())),
            }
        }
        Err(io::Error::other(format!(
            "no store accepted the write ({})",
            errs.join("; ")
        )))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    struct Stub {
        hits: &'static [(&'static str, &'static str)],
        writable: bool,
        nm: &'static str,
    }

    impl Secrets for Stub {
        fn name(&self) -> &str {
            self.nm
        }
        fn get(&self, name: &str) -> Option<String> {
            self.hits
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.to_string())
        }
        fn set(&self, _name: &str, _value: &str) -> io::Result<()> {
            if self.writable {
                Ok(())
            } else {
                Err(io::Error::other("read-only"))
            }
        }
    }

    #[test]
    fn chain_first_hit_wins() {
        let c = Chain::new(vec![
            Box::new(Stub {
                hits: &[],
                writable: false,
                nm: "empty",
            }),
            Box::new(Stub {
                hits: &[("K", "second")],
                writable: false,
                nm: "b",
            }),
            Box::new(Stub {
                hits: &[("K", "third")],
                writable: false,
                nm: "c",
            }),
        ]);
        assert_eq!(c.get("K").as_deref(), Some("second"));
        assert_eq!(c.get("ABSENT"), None);
    }

    #[test]
    fn chain_set_writes_to_first_willing_store() {
        let c = Chain::new(vec![
            Box::new(EnvSecrets),
            Box::new(Stub {
                hits: &[],
                writable: true,
                nm: "cuora",
            }),
            Box::new(Stub {
                hits: &[],
                writable: true,
                nm: "dotenv",
            }),
        ]);
        c.set("K", "v").unwrap(); // env refuses → cuora accepts
        let c = Chain::new(vec![Box::new(EnvSecrets)]);
        assert!(c.set("K", "v").is_err());
    }
}
