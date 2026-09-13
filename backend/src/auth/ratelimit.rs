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

/// The address a request came from. `trusted` says whether it came through
/// the trusted proxy (see [`super::proxy`]); otherwise X-Forwarded-For is
/// ignored.
///
/// This assumes exactly one proxy hop: clients reach the proxy (Traefik)
/// directly, and it appends the address it saw to X-Forwarded-For. So only
/// the rightmost entry of the last X-Forwarded-For header is used; everything
/// to its left is client-supplied. If that entry is empty or not an IP
/// address, the peer is used rather than any other entry.
pub fn client_ip(peer: IpAddr, headers: &HeaderMap, trusted: bool) -> IpAddr {
    let peer = peer.to_canonical();
    if !trusted {
        return peer;
    }
    headers
        .get_all("x-forwarded-for")
        .iter()
        .next_back()
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit(',').next())
        .and_then(|hop| hop.trim().parse::<IpAddr>().ok())
        .map_or(peer, |ip| ip.to_canonical())
}

/// Who a limit applies to. `subnet` is an IPv4 address or an IPv6 /64, the
/// smallest block a single subscriber is usually given. IPv6 clients also
/// carry their /48 `site`, since one site can hold 65,536 /64s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RateKey {
    pub subnet: IpAddr,
    pub site: Option<IpAddr>,
}

impl RateKey {
    pub fn of(ip: IpAddr) -> Self {
        match ip.to_canonical() {
            IpAddr::V4(v4) => Self {
                subnet: IpAddr::V4(v4),
                site: None,
            },
            IpAddr::V6(v6) => Self {
                subnet: v6_prefix(v6, 64),
                site: Some(v6_prefix(v6, 48)),
            },
        }
    }
}

fn v6_prefix(ip: Ipv6Addr, bits: u32) -> IpAddr {
    IpAddr::V6(Ipv6Addr::from(u128::from(ip) & (u128::MAX << (128 - bits))))
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

    const PROXY: &str = "10.0.0.2";
    const REAL: &str = "203.0.113.9";

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn via_proxy(values: &[&str]) -> IpAddr {
        client_ip(ip(PROXY), &xff(values), true)
    }

    #[test]
    fn trusted_proxy_keys_on_the_hop_it_appended() {
        // A client-supplied prefix is ignored in favor of the rightmost entry.
        assert_eq!(via_proxy(&["1.2.3.4, 203.0.113.9"]), ip(REAL));
        assert_eq!(via_proxy(&["1.2.3.4,203.0.113.9"]), ip(REAL));
        assert_eq!(via_proxy(&[" 203.0.113.9 "]), ip(REAL));
        assert_eq!(
            via_proxy(&["1.2.3.4, 5.6.7.8, 2001:db8::1"]),
            ip("2001:db8::1")
        );
    }

    #[test]
    fn with_several_headers_the_last_value_of_the_last_wins() {
        assert_eq!(via_proxy(&["1.2.3.4", "203.0.113.9"]), ip(REAL));
        assert_eq!(
            via_proxy(&["1.2.3.4, 5.6.7.8", "9.9.9.9, 203.0.113.9"]),
            ip(REAL)
        );
        // An earlier header never stands in for a bad last one.
        assert_eq!(via_proxy(&["203.0.113.9", "junk"]), ip(PROXY));
        assert_eq!(via_proxy(&["203.0.113.9", ""]), ip(PROXY));
    }

    #[test]
    fn malformed_or_empty_last_entries_fall_back_to_the_peer() {
        for values in [
            &[][..],
            &[""],
            &[" "],
            &[" , "],
            &["1.2.3.4, "],
            &["1.2.3.4,"],
            &["1.2.3.4, junk"],
            &["1.2.3.4, 203.0.113.9:443"],
            &["1.2.3.4, [2001:db8::1]"],
            &["1.2.3.4, unknown"],
        ] {
            assert_eq!(via_proxy(values), ip(PROXY), "{values:?}");
        }
        // Not valid header text at all.
        let mut h = HeaderMap::new();
        h.insert(
            "x-forwarded-for",
            axum::http::HeaderValue::from_bytes(b"1.2.3.4, \xff").unwrap(),
        );
        assert_eq!(client_ip(ip(PROXY), &h, true), ip(PROXY));
    }

    /// Which requests count as trusted is decided in `proxy`, covering
    /// TRUSTED_PROXY and TRUSTED_PROXY_TOKEN; untrusted ones ignore the header.
    #[test]
    fn forwarded_for_ignored_unless_trusted() {
        let spoof = xff(&["1.2.3.4, 203.0.113.9"]);
        assert_eq!(client_ip(ip(REAL), &spoof, false), ip(REAL));
        assert_eq!(client_ip(ip(PROXY), &spoof, false), ip(PROXY));
    }

    #[test]
    fn mapped_addresses_are_canonical() {
        let mapped_proxy = ip("::ffff:10.0.0.2");
        let h = xff(&["::ffff:203.0.113.9"]);
        assert_eq!(client_ip(mapped_proxy, &h, true), ip(REAL));
        assert_eq!(client_ip(mapped_proxy, &HeaderMap::new(), false), ip(PROXY));
    }

    #[test]
    fn ipv6_keys_group_by_64_and_48() {
        let key = |s: &str| RateKey::of(s.parse().unwrap());
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        let a = key("2001:db8:1:2:aaaa::1");
        assert_eq!(a.subnet, ip("2001:db8:1:2::"));
        assert_eq!(a.site, Some(ip("2001:db8:1::")));
        // Same /64.
        assert_eq!(key("2001:db8:1:2:bbbb::2"), a);
        // Another /64 in the same /48.
        let c = key("2001:db8:1:3::1");
        assert_ne!(c.subnet, a.subnet);
        assert_eq!(c.site, a.site);
        // Another /48.
        assert_ne!(key("2001:db8:2:2::1").site, a.site);
        let v4 = key("203.0.113.9");
        assert_eq!((v4.subnet, v4.site), (ip("203.0.113.9"), None));
        assert_eq!(key("::ffff:203.0.113.9"), v4);
    }
}
