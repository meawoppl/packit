//! Passkey (WebAuthn) accounts: relying-party setup, server-side ceremony
//! state, sessions and rate limits. Nothing outside `/api/auth` is gated.

pub mod ceremony;
pub mod proxy;
pub mod ratelimit;
pub mod session;
pub mod username;

#[cfg(test)]
mod tests;

use crate::config::PublicOrigin;
use axum::http::HeaderMap;
use ceremony::CeremonyStore;
use proxy::ProxyTrust;
use ratelimit::{client_ip, RateKey, RateLimiter};
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use webauthn_rs::prelude::{
    PasskeyAuthentication, PasskeyRegistration, Url, Webauthn, WebauthnBuilder,
};

/// Ceremonies expire after five minutes, matching the WebAuthn timeout.
pub const CEREMONY_TTL: Duration = Duration::from_secs(300);
const CEREMONY_CAP: usize = 10_000;
/// Live ceremonies per client (IPv4 address or IPv6 /64), and per IPv6 /48.
const CEREMONIES_PER_CLIENT: usize = 5;
const CEREMONIES_PER_SITE: usize = 50;
/// Per client, shared by every start and finish endpoint.
const IP_BURST: u32 = 30;
const IP_REFILL: Duration = Duration::from_secs(2);
/// Per IPv6 /48, across all of its /64s.
const SITE_BURST: u32 = 120;
const SITE_REFILL: Duration = Duration::from_millis(500);
/// Per username and client, on login start.
const USERNAME_BURST: u32 = 10;
const USERNAME_REFILL: Duration = Duration::from_secs(30);
const MAX_RATE_KEYS: usize = 100_000;

/// Library state of an in-flight ceremony, plus anything the finish step
/// must take from the server rather than the client. The variant is the
/// operation.
pub enum Pending {
    Register {
        username: String,
        state: PasskeyRegistration,
    },
    Login(PasskeyAuthentication),
    AddPasskey(PasskeyRegistration),
}

pub struct Auth {
    pub webauthn: Webauthn,
    pub origin: PublicOrigin,
    proxy: ProxyTrust,
    pub ceremonies: CeremonyStore<Pending>,
    ip_limiter: RateLimiter<IpAddr>,
    site_limiter: RateLimiter<IpAddr>,
    username_limiter: RateLimiter<(String, IpAddr)>,
    warned_forwarded: AtomicBool,
    warned_token: AtomicBool,
}

impl Auth {
    pub fn new(origin: PublicOrigin, proxy: ProxyTrust) -> anyhow::Result<Self> {
        let rp_origin = Url::parse(&origin.origin)?;
        // Exactly this origin: subdomains and other ports stay disallowed.
        let webauthn = WebauthnBuilder::new(&origin.host, &rp_origin)?
            .rp_name("packit")
            .build()?;
        Ok(Self {
            webauthn,
            origin,
            proxy,
            ceremonies: CeremonyStore::new(
                CEREMONY_CAP,
                CEREMONIES_PER_CLIENT,
                CEREMONIES_PER_SITE,
                CEREMONY_TTL,
            ),
            ip_limiter: RateLimiter::new(IP_BURST, IP_REFILL, MAX_RATE_KEYS),
            site_limiter: RateLimiter::new(SITE_BURST, SITE_REFILL, MAX_RATE_KEYS),
            username_limiter: RateLimiter::new(USERNAME_BURST, USERNAME_REFILL, MAX_RATE_KEYS),
            warned_forwarded: AtomicBool::new(false),
            warned_token: AtomicBool::new(false),
        })
    }

    /// Whether a request from `peer` came through the trusted proxy. A token
    /// header that doesn't match is logged, once per process, without its
    /// value.
    pub fn via_trusted_proxy(&self, peer: Option<IpAddr>, headers: &HeaderMap) -> bool {
        let verdict = self.proxy.check(peer, headers);
        if verdict.bad_token && !self.warned_token.swap(true, Ordering::Relaxed) {
            tracing::warn!(
                "a request carried an X-Packit-Proxy-Token that TRUSTED_PROXY_TOKEN doesn't \
                 match, or none is configured; its X-Forwarded-For was not trusted on that basis"
            );
        }
        verdict.trusted
    }

    /// The rate-limit identity of a request from `peer`; `trusted` is
    /// [`Self::via_trusted_proxy`], decided at the edge.
    pub fn client(&self, peer: IpAddr, headers: &HeaderMap, trusted: bool) -> RateKey {
        if !self.proxy.is_configured()
            && headers.contains_key("x-forwarded-for")
            && !self.warned_forwarded.swap(true, Ordering::Relaxed)
        {
            tracing::warn!(
                "requests carry X-Forwarded-For but neither TRUSTED_PROXY nor \
                 TRUSTED_PROXY_TOKEN is set, so every client behind the proxy shares one \
                 rate limit"
            );
        }
        RateKey::of(client_ip(peer, headers, trusted))
    }

    /// Spend a token from the client's bucket and, for IPv6, its /48's.
    pub fn check_client(&self, client: RateKey, now: Instant) -> Result<(), Duration> {
        self.ip_limiter.check(&client.subnet, now)?;
        match client.site {
            Some(site) => self.site_limiter.check(&site, now),
            None => Ok(()),
        }
    }

    /// Spend a login-start token for `username` from this client. Keyed on
    /// both, so nobody can lock another client out of an account.
    pub fn check_username(
        &self,
        username: &str,
        client: RateKey,
        now: Instant,
    ) -> Result<(), Duration> {
        self.username_limiter
            .check(&(username.to_string(), client.subnet), now)
    }
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0; N];
    getrandom::getrandom(&mut buf).expect("the OS random number generator failed");
    buf
}

pub mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Decode exactly `N` bytes from `2 * N` hex digits.
    pub fn decode<const N: usize>(s: &str) -> Option<[u8; N]> {
        let (pairs, rest) = s.as_bytes().as_chunks::<2>();
        if pairs.len() != N || !rest.is_empty() {
            return None;
        }
        let nibble = |c: u8| (c as char).to_digit(16);
        let mut out = [0; N];
        for (o, [hi, lo]) in out.iter_mut().zip(pairs) {
            *o = (nibble(*hi)? * 16 + nibble(*lo)?) as u8;
        }
        Some(out)
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn roundtrips_and_rejects_malformed() {
            let bytes = [0x00, 0x7f, 0xab, 0xff];
            assert_eq!(super::encode(&bytes), "007fabff");
            assert_eq!(super::decode::<4>("007fabff"), Some(bytes));
            assert_eq!(super::decode::<4>("007FABFF"), Some(bytes));
            for bad in [
                "",
                "007fab",
                "007fabff00",
                "007fabfg",
                "+07fabff",
                "007fab f",
            ] {
                assert_eq!(super::decode::<4>(bad), None, "{bad}");
            }
        }
    }
}
