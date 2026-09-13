//! Bounded, self-expiring token buckets, and the client address they are
//! keyed by.

use axum::http::HeaderMap;
use std::collections::HashMap;
use std::hash::Hash;
use std::net::{IpAddr, Ipv6Addr};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

struct Bucket {
    tokens: f64,
    updated: Instant,
}

pub struct RateLimiter<K> {
    burst: f64,
    refill_every: Duration,
    max_keys: usize,
    buckets: Mutex<HashMap<K, Bucket>>,
}

impl<K: Eq + Hash + Clone> RateLimiter<K> {
    /// Allow `burst` requests at once per key, then one more every
    /// `refill_every`. At most `max_keys` keys are tracked.
    pub fn new(burst: u32, refill_every: Duration, max_keys: usize) -> Self {
        Self {
            burst: burst as f64,
            refill_every,
            max_keys,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    fn level(&self, b: &Bucket, now: Instant) -> f64 {
        let refilled = now.saturating_duration_since(b.updated).as_secs_f64()
            / self.refill_every.as_secs_f64();
        (b.tokens + refilled).min(self.burst)
    }

    /// Spend one token for `key`, or return how long until one is available.
    pub fn check(&self, key: &K, now: Instant) -> Result<(), Duration> {
        let mut buckets = self.buckets.lock().unwrap_or_else(PoisonError::into_inner);
        if !buckets.contains_key(key) && buckets.len() >= self.max_keys {
            // A bucket that has refilled completely holds no state; drop it.
            buckets.retain(|_, b| self.level(b, now) < self.burst);
            if buckets.len() >= self.max_keys {
                return Err(self.refill_every);
            }
        }
        let bucket = buckets.entry(key.clone()).or_insert(Bucket {
            tokens: self.burst,
            updated: now,
        });
        bucket.tokens = self.level(bucket, now);
        bucket.updated = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(())
        } else {
            Err(self.refill_every.mul_f64(1.0 - bucket.tokens))
        }
    }
}

/// The address a request came from. X-Forwarded-For is used only when the
/// socket peer is the configured trusted proxy, and then only its last hop,
/// which that proxy appended itself. Anything earlier in the list is
/// client-supplied and never trusted.
pub fn client_ip(peer: IpAddr, headers: &HeaderMap, trusted_proxy: Option<IpAddr>) -> IpAddr {
    let peer = peer.to_canonical();
    if trusted_proxy.map(|p| p.to_canonical()) != Some(peer) {
        return peer;
    }
    headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .rfind(|hop| !hop.is_empty())
        .and_then(|hop| hop.parse::<IpAddr>().ok())
        .map_or(peer, |ip| ip.to_canonical())
}

/// Rate-limit key for an address: IPv6 clients are grouped by /64, the
/// smallest block a single subscriber is usually given.
pub fn rate_key(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(_) => ip,
        IpAddr::V6(v6) => IpAddr::V6(Ipv6Addr::from(u128::from(v6) & !((1u128 << 64) - 1))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEC: Duration = Duration::from_secs(1);

    #[test]
    fn bucket_allows_burst_then_refills() {
        let limiter = RateLimiter::new(3, 2 * SEC, 100);
        let now = Instant::now();
        for _ in 0..3 {
            assert!(limiter.check(&"a", now).is_ok());
        }
        assert_eq!(limiter.check(&"a", now), Err(2 * SEC));
        // Other keys are independent.
        assert!(limiter.check(&"b", now).is_ok());
        // Half a token after one second: wait one more second.
        let wait = limiter.check(&"a", now + SEC).unwrap_err();
        assert!((wait.as_secs_f64() - 1.0).abs() < 1e-6, "{wait:?}");
        assert!(limiter.check(&"a", now + 2 * SEC).is_ok());
        assert!(limiter.check(&"a", now + 2 * SEC).is_err());
        // Never refills beyond the burst.
        let much_later = now + 1000 * SEC;
        for _ in 0..3 {
            assert!(limiter.check(&"a", much_later).is_ok());
        }
        assert!(limiter.check(&"a", much_later).is_err());
    }

    #[test]
    fn key_count_is_bounded_and_idle_keys_expire() {
        let limiter = RateLimiter::new(2, SEC, 2);
        let now = Instant::now();
        assert!(limiter.check(&1, now).is_ok());
        assert!(limiter.check(&2, now).is_ok());
        // Both tracked buckets are still draining, so a third key is refused
        // rather than evicting live state.
        assert_eq!(limiter.check(&3, now), Err(SEC));
        // Known keys keep working.
        assert!(limiter.check(&1, now).is_ok());
        assert!(limiter.check(&1, now).is_err());
        // Once key 2 has refilled it is dropped to make room.
        let later = now + SEC;
        assert!(limiter.check(&3, later).is_ok());
        assert_eq!(limiter.buckets.lock().unwrap().len(), 2);
        assert!(limiter.buckets.lock().unwrap().contains_key(&1));
    }

    fn xff(values: &[&str]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for v in values {
            h.append("x-forwarded-for", v.parse().unwrap());
        }
        h
    }

    #[test]
    fn forwarded_for_ignored_unless_peer_is_the_trusted_proxy() {
        let proxy: IpAddr = "10.0.0.2".parse().unwrap();
        let client: IpAddr = "203.0.113.9".parse().unwrap();
        let spoof = xff(&["198.51.100.1, 203.0.113.9"]);
        // No proxy configured: always the peer.
        assert_eq!(client_ip(client, &spoof, None), client);
        // A proxy configured, but this peer is someone else.
        assert_eq!(client_ip(client, &spoof, Some(proxy)), client);
        // Through the proxy: the last hop, never the client-supplied first.
        let via = xff(&["198.51.100.1", "203.0.113.9"]);
        assert_eq!(client_ip(proxy, &via, Some(proxy)), client);
        assert_eq!(client_ip(proxy, &spoof, Some(proxy)), client);
        // Through the proxy with a missing or unparsable last hop: the proxy.
        for h in [xff(&[]), xff(&["203.0.113.9, junk"]), xff(&[" , "])] {
            assert_eq!(client_ip(proxy, &h, Some(proxy)), proxy);
        }
    }

    #[test]
    fn mapped_addresses_are_canonical() {
        let proxy: IpAddr = "10.0.0.2".parse().unwrap();
        let mapped_proxy: IpAddr = "::ffff:10.0.0.2".parse().unwrap();
        let h = xff(&["::ffff:203.0.113.9"]);
        assert_eq!(
            client_ip(mapped_proxy, &h, Some(proxy)),
            "203.0.113.9".parse::<IpAddr>().unwrap()
        );
        assert_eq!(client_ip(mapped_proxy, &HeaderMap::new(), None), proxy);
    }

    #[test]
    fn ipv6_keys_group_by_64() {
        let a: IpAddr = "2001:db8:1:2:aaaa::1".parse().unwrap();
        let b: IpAddr = "2001:db8:1:2:bbbb::2".parse().unwrap();
        let c: IpAddr = "2001:db8:1:3::1".parse().unwrap();
        assert_eq!(rate_key(a), rate_key(b));
        assert_ne!(rate_key(a), rate_key(c));
        assert_eq!(rate_key(a), "2001:db8:1:2::".parse::<IpAddr>().unwrap());
        let v4: IpAddr = "203.0.113.9".parse().unwrap();
        assert_eq!(rate_key(v4), v4);
    }
}
