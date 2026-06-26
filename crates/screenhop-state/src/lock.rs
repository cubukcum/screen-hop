use std::collections::HashMap;

/// Default lease length. Per **D5** this must exceed the per-monitor switch hard-ceiling (15 s)
/// plus margin, so a lease cannot expire mid-switch and admit a second actuator. The holder
/// renews before entering a known-slow push-release.
pub const DEFAULT_LEASE_MS: u64 = 30_000;

/// A granted per-monitor lease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
    pub holder: String,
    pub expires_ms: u64,
}

/// Result of an acquire attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockOutcome {
    Granted(Lease),
    Denied { current_holder: String },
}

/// Per-monitor lease locks (D1: a lease lock, **no** elected coordinator). The lock authority is
/// message/lease-granted; any replicated-store copy is advisory cache only.
#[derive(Debug, Default, Clone)]
pub struct LockManager {
    locks: HashMap<String, Lease>,
}

impl LockManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Acquire (or renew, if already held by `peer`) the lock for `monitor`. Granted when the
    /// monitor is free, the existing lease has expired, or `peer` already holds it.
    pub fn acquire(&mut self, monitor: &str, peer: &str, now_ms: u64, lease_ms: u64) -> LockOutcome {
        if let Some(l) = self.locks.get(monitor) {
            if l.expires_ms > now_ms && l.holder != peer {
                return LockOutcome::Denied {
                    current_holder: l.holder.clone(),
                };
            }
        }
        let lease = Lease {
            holder: peer.to_owned(),
            expires_ms: now_ms + lease_ms,
        };
        self.locks.insert(monitor.to_owned(), lease.clone());
        LockOutcome::Granted(lease)
    }

    /// Extend a lease the peer currently and validly holds. `None` if it doesn't hold it (or it
    /// already expired) — the caller must then re-acquire.
    pub fn renew(&mut self, monitor: &str, peer: &str, now_ms: u64, lease_ms: u64) -> Option<Lease> {
        match self.locks.get(monitor) {
            Some(l) if l.holder == peer && l.expires_ms > now_ms => {
                let lease = Lease {
                    holder: peer.to_owned(),
                    expires_ms: now_ms + lease_ms,
                };
                self.locks.insert(monitor.to_owned(), lease.clone());
                Some(lease)
            }
            _ => None,
        }
    }

    /// Release a lock the peer holds. Returns false if it didn't hold it.
    pub fn release(&mut self, monitor: &str, peer: &str) -> bool {
        if matches!(self.locks.get(monitor), Some(l) if l.holder == peer) {
            self.locks.remove(monitor);
            true
        } else {
            false
        }
    }

    /// The current valid holder of a monitor's lock, if any.
    pub fn holder(&self, monitor: &str, now_ms: u64) -> Option<&str> {
        self.locks
            .get(monitor)
            .filter(|l| l.expires_ms > now_ms)
            .map(|l| l.holder.as_str())
    }

    /// Acquire **all** of a preset's monitors up front, or none (D1: presets grab every involved
    /// lock first). On the first denial, any locks grabbed in this call are released and the
    /// blocking monitor + holder is returned.
    pub fn acquire_all(
        &mut self,
        monitors: &[&str],
        peer: &str,
        now_ms: u64,
        lease_ms: u64,
    ) -> Result<(), (String, String)> {
        let mut acquired: Vec<&str> = Vec::new();
        for &m in monitors {
            match self.acquire(m, peer, now_ms, lease_ms) {
                LockOutcome::Granted(_) => acquired.push(m),
                LockOutcome::Denied { current_holder } => {
                    for a in &acquired {
                        self.release(a, peer);
                    }
                    return Err((m.to_owned(), current_holder));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEASE: u64 = DEFAULT_LEASE_MS;

    #[test]
    fn acquires_a_free_lock() {
        let mut m = LockManager::new();
        assert!(matches!(m.acquire("mon", "A", 0, LEASE), LockOutcome::Granted(_)));
        assert_eq!(m.holder("mon", 1), Some("A"));
    }

    #[test]
    fn denies_when_held_by_another_peer() {
        let mut m = LockManager::new();
        m.acquire("mon", "A", 0, LEASE);
        assert_eq!(
            m.acquire("mon", "B", 100, LEASE),
            LockOutcome::Denied { current_holder: "A".into() }
        );
    }

    #[test]
    fn same_peer_can_reacquire() {
        let mut m = LockManager::new();
        m.acquire("mon", "A", 0, LEASE);
        assert!(matches!(m.acquire("mon", "A", 100, LEASE), LockOutcome::Granted(_)));
    }

    #[test]
    fn expired_lock_is_grantable_to_another() {
        let mut m = LockManager::new();
        m.acquire("mon", "A", 0, LEASE);
        // After the lease elapses, B can take it.
        assert!(matches!(m.acquire("mon", "B", LEASE + 1, LEASE), LockOutcome::Granted(_)));
        assert_eq!(m.holder("mon", LEASE + 2), Some("B"));
    }

    #[test]
    fn renew_extends_only_for_valid_holder() {
        let mut m = LockManager::new();
        m.acquire("mon", "A", 0, LEASE);
        assert!(m.renew("mon", "A", 1_000, LEASE).is_some());
        assert!(m.renew("mon", "B", 1_000, LEASE).is_none()); // not the holder
        assert!(m.renew("mon", "A", LEASE + 5_000, LEASE).is_none()); // already expired
    }

    #[test]
    fn release_only_by_holder() {
        let mut m = LockManager::new();
        m.acquire("mon", "A", 0, LEASE);
        assert!(!m.release("mon", "B"));
        assert!(m.release("mon", "A"));
        assert_eq!(m.holder("mon", 1), None);
    }

    #[test]
    fn holder_respects_expiry() {
        let mut m = LockManager::new();
        m.acquire("mon", "A", 0, LEASE);
        assert_eq!(m.holder("mon", LEASE - 1), Some("A"));
        assert_eq!(m.holder("mon", LEASE + 1), None);
    }

    #[test]
    fn acquire_all_is_all_or_nothing() {
        let mut m = LockManager::new();
        // B holds mon2; A's preset over [mon1, mon2, mon3] must fail and roll back mon1.
        m.acquire("mon2", "B", 0, LEASE);
        let err = m.acquire_all(&["mon1", "mon2", "mon3"], "A", 100, LEASE);
        assert_eq!(err, Err(("mon2".into(), "B".into())));
        assert_eq!(m.holder("mon1", 200), None); // rolled back
        assert_eq!(m.holder("mon3", 200), None); // never acquired
        assert_eq!(m.holder("mon2", 200), Some("B")); // untouched
    }

    #[test]
    fn acquire_all_succeeds_when_all_free() {
        let mut m = LockManager::new();
        assert!(m.acquire_all(&["mon1", "mon2", "mon3"], "A", 0, LEASE).is_ok());
        assert_eq!(m.holder("mon1", 1), Some("A"));
        assert_eq!(m.holder("mon3", 1), Some("A"));
    }
}
