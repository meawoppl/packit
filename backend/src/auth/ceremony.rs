//! In-flight WebAuthn ceremonies, held server-side in a bounded map.
//!
//! The client only ever sees a random ceremony id. The library state and the
//! intended account stay here, and each entry is removed the first time
//! anyone tries to finish it. Each client, and each IPv6 /48, may hold only a
//! few live ceremonies, so no one network can fill the map.

use super::ratelimit::RateKey;
use super::{constant_time_eq, hex, random_bytes};
use std::collections::hash_map::{Entry as MapEntry, HashMap};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};
use uuid::Uuid;

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
pub struct Ceremony<T> {
    pub user_id: Uuid,
    pub state: T,
}

struct Entry<S> {
    client: RateKey,
    nonce: [u8; 32],
    user_id: Uuid,
    state: S,
    expires: Instant,
}

#[derive(Debug, PartialEq, Eq)]
pub enum InsertError {
    /// The map is full of live ceremonies.
    Full,
    /// This client or its IPv6 /48 has too many ceremonies in progress. The
    /// earliest of them expires after this wait.
    Busy(Duration),
}

#[derive(Debug, PartialEq, Eq)]
pub enum TakeError {
    Missing,
    Expired,
    WrongBrowser,
    WrongOperation,
}

pub struct CeremonyStore<S> {
    cap: usize,
    per_client: usize,
    per_site: usize,
    ttl: Duration,
    entries: Mutex<HashMap<CeremonyId, Entry<S>>>,
}

impl<S> CeremonyStore<S> {
    /// At most `cap` ceremonies in all, `per_client` live ones per
    /// [`RateKey::subnet`] and `per_site` per IPv6 /48.
    pub fn new(cap: usize, per_client: usize, per_site: usize, ttl: Duration) -> Self {
        Self {
            cap,
            per_client,
            per_site,
            ttl,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Store a ceremony for `client`, bound to the browser `nonce` and
    /// `user_id`.
    pub fn insert(
        &self,
        client: RateKey,
        nonce: [u8; 32],
        user_id: Uuid,
        state: S,
        now: Instant,
    ) -> Result<CeremonyId, InsertError> {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let (mut own, mut site, mut soonest) = (0, 0, None::<Instant>);
        for e in entries.values().filter(|e| e.expires > now) {
            let same_client = e.client.subnet == client.subnet;
            let same_site = client.site.is_some() && e.client.site == client.site;
            own += usize::from(same_client);
            site += usize::from(same_site);
            if same_client || same_site {
                soonest = Some(soonest.map_or(e.expires, |t| t.min(e.expires)));
            }
        }
        if own >= self.per_client || site >= self.per_site {
            return Err(InsertError::Busy(soonest.map_or(self.ttl, |t| t - now)));
        }
        if entries.len() >= self.cap {
            entries.retain(|_, e| e.expires > now);
        }
        if entries.len() >= self.cap {
            return Err(InsertError::Full);
        }
        let entry = Entry {
            client,
            nonce,
            user_id,
            state,
            expires: now + self.ttl,
        };
        loop {
            let id = CeremonyId(random_bytes());
            if let MapEntry::Vacant(slot) = entries.entry(id) {
                slot.insert(entry);
                return Ok(id);
            }
        }
    }

    /// Remove the ceremony and return it if it is live, `nonce` matches the
    /// one bound at start, and `pick` accepts its state (the operation). The
    /// entry is consumed even when a check fails, so every ceremony gets
    /// exactly one finish attempt.
    pub fn take<T>(
        &self,
        id: CeremonyId,
        nonce: Option<[u8; 32]>,
        now: Instant,
        pick: impl FnOnce(S) -> Option<T>,
    ) -> Result<Ceremony<T>, TakeError> {
        let entry = self
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id)
            .ok_or(TakeError::Missing)?;
        if entry.expires <= now {
            return Err(TakeError::Expired);
        }
        if !nonce.is_some_and(|n| constant_time_eq(&n, &entry.nonce)) {
            return Err(TakeError::WrongBrowser);
        }
        let state = pick(entry.state).ok_or(TakeError::WrongOperation)?;
        Ok(Ceremony {
            user_id: entry.user_id,
            state,
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
    use std::net::{IpAddr, Ipv6Addr};
    use std::sync::Arc;

    const TTL: Duration = Duration::from_secs(300);
    const SEC: Duration = Duration::from_secs(1);
    const NONCE: [u8; 32] = [7; 32];

    #[derive(Debug, PartialEq)]
    enum Op {
        Register(u32),
        Login(u32),
    }

    fn login(op: Op) -> Option<u32> {
        match op {
            Op::Login(v) => Some(v),
            Op::Register(_) => None,
        }
    }

    fn register(op: Op) -> Option<u32> {
        match op {
            Op::Register(v) => Some(v),
            Op::Login(_) => None,
        }
    }

    fn v4(host: u8) -> RateKey {
        RateKey::of(IpAddr::from([192, 0, 2, host]))
    }

    fn v6(site: u16, subnet: u16, host: u16) -> RateKey {
        RateKey::of(Ipv6Addr::new(0x2001, 0xdb8, site, subnet, 0, 0, 0, host).into())
    }

    fn store(cap: usize) -> CeremonyStore<Op> {
        CeremonyStore::new(cap, 5, 50, TTL)
    }

    fn add(
        s: &CeremonyStore<Op>,
        client: RateKey,
        op: Op,
        now: Instant,
    ) -> Result<CeremonyId, InsertError> {
        s.insert(client, NONCE, Uuid::nil(), op, now)
    }

    #[test]
    fn ids_are_random_and_roundtrip_as_hex() {
        let s = store(10);
        let now = Instant::now();
        let a = add(&s, v4(1), Op::Login(1), now).unwrap();
        let b = add(&s, v4(1), Op::Login(2), now).unwrap();
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
        let id = s.insert(v4(1), NONCE, user, Op::Register(42), now).unwrap();
        let c = s.take(id, Some(NONCE), now, register).unwrap();
        assert_eq!((c.user_id, c.state), (user, 42));
        assert_eq!(
            s.take(id, Some(NONCE), now, register).err(),
            Some(TakeError::Missing)
        );
    }

    #[test]
    fn expires_after_ttl_and_is_consumed() {
        let s = store(10);
        let now = Instant::now();
        let id = add(&s, v4(1), Op::Login(1), now).unwrap();
        assert_eq!(
            s.take(id, Some(NONCE), now + TTL, login).err(),
            Some(TakeError::Expired)
        );
        assert_eq!(
            s.take(id, Some(NONCE), now, login).err(),
            Some(TakeError::Missing)
        );
        let id = add(&s, v4(1), Op::Login(1), now).unwrap();
        let just_in_time = now + TTL - Duration::from_millis(1);
        assert!(s.take(id, Some(NONCE), just_in_time, login).is_ok());
    }

    #[test]
    fn wrong_operation_or_browser_fails_and_consumes() {
        let s = store(10);
        let now = Instant::now();
        let id = add(&s, v4(1), Op::Register(1), now).unwrap();
        assert_eq!(
            s.take(id, Some(NONCE), now, login).err(),
            Some(TakeError::WrongOperation)
        );
        assert_eq!(
            s.take(id, Some(NONCE), now, register).err(),
            Some(TakeError::Missing)
        );
        for nonce in [Some([8; 32]), None] {
            let id = add(&s, v4(1), Op::Login(1), now).unwrap();
            assert_eq!(
                s.take(id, nonce, now, login).err(),
                Some(TakeError::WrongBrowser)
            );
            assert_eq!(
                s.take(id, Some(NONCE), now, login).err(),
                Some(TakeError::Missing)
            );
        }
    }

    #[test]
    fn cap_evicts_expired_first_then_refuses() {
        let s = store(3);
        let now = Instant::now();
        let first = add(&s, v4(1), Op::Login(0), now).unwrap();
        for i in 1..3 {
            add(&s, v4(i as u8 + 1), Op::Login(i), now + TTL / 2).unwrap();
        }
        assert_eq!(
            add(&s, v4(9), Op::Login(9), now + TTL / 2).err(),
            Some(InsertError::Full)
        );
        // Once the first entry has expired it is evicted to make room, and
        // the live ones are kept.
        let later = now + TTL;
        let id = add(&s, v4(4), Op::Login(3), later).unwrap();
        assert_eq!(
            s.take(first, Some(NONCE), later, login).err(),
            Some(TakeError::Missing)
        );
        // Full of live entries again.
        assert_eq!(
            add(&s, v4(5), Op::Login(4), later).err(),
            Some(InsertError::Full)
        );
        // Finishing one frees its slot.
        assert_eq!(s.take(id, Some(NONCE), later, login).unwrap().state, 3);
        assert!(add(&s, v4(5), Op::Login(5), later).is_ok());
        assert!(add(&s, v4(6), Op::Login(6), later).is_err());
    }

    #[test]
    fn each_client_holds_a_few_live_ceremonies() {
        let s = CeremonyStore::new(1000, 2, 3, TTL);
        let now = Instant::now();
        let client = v4(1);
        let first = add(&s, client, Op::Login(1), now).unwrap();
        add(&s, client, Op::Login(2), now + SEC).unwrap();
        let later = now + 2 * SEC;
        // Busy until the earliest of its ceremonies expires.
        assert_eq!(
            add(&s, client, Op::Login(3), later).err(),
            Some(InsertError::Busy(TTL - 2 * SEC))
        );
        // Another client is unaffected.
        assert!(add(&s, v4(2), Op::Login(4), later).is_ok());
        // Finishing one frees a slot.
        assert!(s.take(first, Some(NONCE), later, login).is_ok());
        assert!(add(&s, client, Op::Login(5), later).is_ok());
        assert!(add(&s, client, Op::Login(6), later).is_err());
        // Expired ceremonies don't count.
        assert!(add(&s, client, Op::Login(7), now + SEC + TTL).is_ok());
    }

    #[test]
    fn an_ipv6_site_shares_a_cap_across_its_64s() {
        let s = CeremonyStore::new(1000, 2, 3, TTL);
        let now = Instant::now();
        for subnet in 0..3 {
            add(&s, v6(1, subnet, 1), Op::Login(1), now).unwrap();
        }
        // A fresh /64 in the same /48 is refused...
        assert!(matches!(
            add(&s, v6(1, 9, 1), Op::Login(1), now),
            Err(InsertError::Busy(_))
        ));
        // ...while another /48 and IPv4 clients are not.
        assert!(add(&s, v6(2, 0, 1), Op::Login(1), now).is_ok());
        assert!(add(&s, v4(1), Op::Login(1), now).is_ok());
        // Hosts within one /64 are one client.
        let s = CeremonyStore::new(1000, 2, 3, TTL);
        add(&s, v6(1, 0, 1), Op::Login(1), now).unwrap();
        add(&s, v6(1, 0, 2), Op::Login(1), now).unwrap();
        assert!(matches!(
            add(&s, v6(1, 0, 3), Op::Login(1), now),
            Err(InsertError::Busy(_))
        ));
    }

    #[test]
    fn concurrent_takes_succeed_exactly_once() {
        for _ in 0..50 {
            let s = Arc::new(store(10));
            let now = Instant::now();
            let id = add(&s, v4(1), Op::Login(1), now).unwrap();
            let barrier = Arc::new(std::sync::Barrier::new(8));
            let wins: usize = (0..8)
                .map(|_| {
                    let (s, barrier) = (s.clone(), barrier.clone());
                    std::thread::spawn(move || {
                        barrier.wait();
                        s.take(id, Some(NONCE), now, login).is_ok()
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
