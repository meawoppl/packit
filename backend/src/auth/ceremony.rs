//! In-flight WebAuthn ceremonies, held server-side in a bounded map.
//!
//! The client only ever sees a random ceremony id. The library state, the
//! intended account and the operation stay here, and each entry is removed
//! the first time anyone tries to finish it.

use super::{constant_time_eq, hex, random_bytes};
use std::collections::hash_map::{Entry as MapEntry, HashMap};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Register,
    Login,
    AddPasskey,
}

/// Random 128-bit ceremony handle, sent to the client as 32 hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CeremonyId([u8; 16]);

impl CeremonyId {
    pub fn parse(s: &str) -> Option<Self> {
        hex::decode(s).map(Self)
    }

    pub fn to_hex(self) -> String {
        hex::encode(&self.0)
    }
}

/// What a successful [`CeremonyStore::take`] hands back.
pub struct Ceremony<S> {
    pub user_id: Uuid,
    pub state: S,
}

struct Entry<S> {
    op: Operation,
    nonce: [u8; 32],
    user_id: Uuid,
    state: S,
    expires: Instant,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TakeError {
    Missing,
    Expired,
    WrongOperation,
    WrongBrowser,
}

pub struct CeremonyStore<S> {
    cap: usize,
    ttl: Duration,
    entries: Mutex<HashMap<CeremonyId, Entry<S>>>,
}

impl<S> CeremonyStore<S> {
    pub fn new(cap: usize, ttl: Duration) -> Self {
        Self {
            cap,
            ttl,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Store a ceremony bound to `op`, the browser `nonce` and `user_id`.
    /// Returns `None` when the map is full even after dropping expired
    /// entries.
    pub fn insert(
        &self,
        op: Operation,
        nonce: [u8; 32],
        user_id: Uuid,
        state: S,
        now: Instant,
    ) -> Option<CeremonyId> {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        if entries.len() >= self.cap {
            entries.retain(|_, e| e.expires > now);
        }
        if entries.len() >= self.cap {
            return None;
        }
        let entry = Entry {
            op,
            nonce,
            user_id,
            state,
            expires: now + self.ttl,
        };
        loop {
            let id = CeremonyId(random_bytes());
            if let MapEntry::Vacant(slot) = entries.entry(id) {
                slot.insert(entry);
                return Some(id);
            }
        }
    }

    /// Remove the ceremony and return it if it is live, is for `op`, and
    /// `nonce` matches the one bound at start. The entry is consumed even when
    /// a check fails, so every ceremony gets exactly one finish attempt.
    pub fn take(
        &self,
        id: CeremonyId,
        op: Operation,
        nonce: Option<[u8; 32]>,
        now: Instant,
    ) -> Result<Ceremony<S>, TakeError> {
        let entry = self
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id)
            .ok_or(TakeError::Missing)?;
        if entry.expires <= now {
            return Err(TakeError::Expired);
        }
        if entry.op != op {
            return Err(TakeError::WrongOperation);
        }
        if !nonce.is_some_and(|n| constant_time_eq(&n, &entry.nonce)) {
            return Err(TakeError::WrongBrowser);
        }
        Ok(Ceremony {
            user_id: entry.user_id,
            state: entry.state,
        })
    }

    /// Make every stored ceremony expire now, for end-to-end expiry tests.
    #[cfg(test)]
    pub fn expire_all(&self) {
        let now = Instant::now();
        for e in self.entries.lock().unwrap().values_mut() {
            e.expires = now;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    const TTL: Duration = Duration::from_secs(300);
    const NONCE: [u8; 32] = [7; 32];

    fn store(cap: usize) -> CeremonyStore<u32> {
        CeremonyStore::new(cap, TTL)
    }

    #[test]
    fn ids_are_random_and_roundtrip_as_hex() {
        let s = store(10);
        let now = Instant::now();
        let a = s
            .insert(Operation::Login, NONCE, Uuid::nil(), 1, now)
            .unwrap();
        let b = s
            .insert(Operation::Login, NONCE, Uuid::nil(), 2, now)
            .unwrap();
        assert_ne!(a, b);
        assert_eq!(a.to_hex().len(), 32);
        assert_eq!(CeremonyId::parse(&a.to_hex()), Some(a));
        for bad in ["", "00", &"0".repeat(31), &"0".repeat(33), &"g".repeat(32)] {
            assert_eq!(CeremonyId::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn take_returns_bound_user_and_state_once() {
        let s = store(10);
        let now = Instant::now();
        let user = Uuid::new_v4();
        let id = s.insert(Operation::Register, NONCE, user, 42, now).unwrap();
        let c = s.take(id, Operation::Register, Some(NONCE), now).unwrap();
        assert_eq!((c.user_id, c.state), (user, 42));
        assert_eq!(
            s.take(id, Operation::Register, Some(NONCE), now).err(),
            Some(TakeError::Missing)
        );
    }

    #[test]
    fn expires_after_ttl_and_is_consumed() {
        let s = store(10);
        let now = Instant::now();
        let id = s
            .insert(Operation::Login, NONCE, Uuid::nil(), 1, now)
            .unwrap();
        let late = now + TTL;
        assert_eq!(
            s.take(id, Operation::Login, Some(NONCE), late).err(),
            Some(TakeError::Expired)
        );
        assert_eq!(
            s.take(id, Operation::Login, Some(NONCE), now).err(),
            Some(TakeError::Missing)
        );
        let id = s
            .insert(Operation::Login, NONCE, Uuid::nil(), 1, now)
            .unwrap();
        let just_in_time = now + TTL - Duration::from_millis(1);
        assert!(s
            .take(id, Operation::Login, Some(NONCE), just_in_time)
            .is_ok());
    }

    #[test]
    fn wrong_operation_or_browser_fails_and_consumes() {
        let s = store(10);
        let now = Instant::now();
        let id = s
            .insert(Operation::Register, NONCE, Uuid::nil(), 1, now)
            .unwrap();
        assert_eq!(
            s.take(id, Operation::AddPasskey, Some(NONCE), now).err(),
            Some(TakeError::WrongOperation)
        );
        assert_eq!(
            s.take(id, Operation::Register, Some(NONCE), now).err(),
            Some(TakeError::Missing)
        );
        for nonce in [Some([8; 32]), None] {
            let id = s
                .insert(Operation::Login, NONCE, Uuid::nil(), 1, now)
                .unwrap();
            assert_eq!(
                s.take(id, Operation::Login, nonce, now).err(),
                Some(TakeError::WrongBrowser)
            );
            assert_eq!(
                s.take(id, Operation::Login, Some(NONCE), now).err(),
                Some(TakeError::Missing)
            );
        }
    }

    #[test]
    fn cap_evicts_expired_first_then_refuses() {
        let s = store(3);
        let now = Instant::now();
        let first = s
            .insert(Operation::Login, NONCE, Uuid::nil(), 0, now)
            .unwrap();
        for i in 1..3 {
            s.insert(Operation::Login, NONCE, Uuid::nil(), i, now + TTL / 2)
                .unwrap();
        }
        assert!(s
            .insert(Operation::Login, NONCE, Uuid::nil(), 9, now + TTL / 2)
            .is_none());
        // Once the first entry has expired it is evicted to make room, and
        // the live ones are kept.
        let later = now + TTL;
        let id = s
            .insert(Operation::Login, NONCE, Uuid::nil(), 3, later)
            .unwrap();
        assert_eq!(
            s.take(first, Operation::Login, Some(NONCE), later).err(),
            Some(TakeError::Missing)
        );
        // Full of live entries again.
        assert!(s
            .insert(Operation::Login, NONCE, Uuid::nil(), 4, later)
            .is_none());
        // Finishing one frees its slot.
        assert_eq!(
            s.take(id, Operation::Login, Some(NONCE), later)
                .unwrap()
                .state,
            3
        );
        assert!(s
            .insert(Operation::Login, NONCE, Uuid::nil(), 5, later)
            .is_some());
        assert!(s
            .insert(Operation::Login, NONCE, Uuid::nil(), 6, later)
            .is_none());
    }

    #[test]
    fn concurrent_takes_succeed_exactly_once() {
        for _ in 0..50 {
            let s = Arc::new(store(10));
            let now = Instant::now();
            let id = s
                .insert(Operation::Login, NONCE, Uuid::nil(), 1, now)
                .unwrap();
            let barrier = Arc::new(std::sync::Barrier::new(8));
            let wins: usize = (0..8)
                .map(|_| {
                    let (s, barrier) = (s.clone(), barrier.clone());
                    std::thread::spawn(move || {
                        barrier.wait();
                        s.take(id, Operation::Login, Some(NONCE), now).is_ok()
                    })
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|h| h.join().unwrap() as usize)
                .sum();
            assert_eq!(wins, 1);
        }
    }
}
