//! Passkey (WebAuthn) accounts: relying-party setup, server-side ceremony
//! state, sessions and rate limits. Nothing outside `/api/auth` is gated.

pub mod ceremony;
pub mod ratelimit;
pub mod session;
pub mod username;

#[cfg(test)]
mod tests;

use crate::config::PublicOrigin;
use ceremony::CeremonyStore;
use ratelimit::RateLimiter;
use std::net::IpAddr;
use std::time::Duration;
use webauthn_rs::prelude::{
    PasskeyAuthentication, PasskeyRegistration, Url, Webauthn, WebauthnBuilder,
};

/// Ceremonies expire after five minutes, matching the WebAuthn timeout.
pub const CEREMONY_TTL: Duration = Duration::from_secs(300);
const CEREMONY_CAP: usize = 10_000;
/// Per client address, shared by every start and finish endpoint.
const IP_BURST: u32 = 30;
const IP_REFILL: Duration = Duration::from_secs(2);
/// Per username, on login start.
const USERNAME_BURST: u32 = 10;
const USERNAME_REFILL: Duration = Duration::from_secs(30);
const MAX_RATE_KEYS: usize = 100_000;

/// Library state of an in-flight ceremony, plus anything the finish step
/// must take from the server rather than the client.
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
    pub trusted_proxy: Option<IpAddr>,
    pub ceremonies: CeremonyStore<Pending>,
    pub ip_limiter: RateLimiter<IpAddr>,
    pub username_limiter: RateLimiter<String>,
}

impl Auth {
    pub fn new(origin: PublicOrigin, trusted_proxy: Option<IpAddr>) -> anyhow::Result<Self> {
        let rp_origin = Url::parse(&origin.origin)?;
        // Exactly this origin: subdomains and other ports stay disallowed.
        let webauthn = WebauthnBuilder::new(&origin.host, &rp_origin)?
            .rp_name("packit")
            .build()?;
        Ok(Self {
            webauthn,
            origin,
            trusted_proxy,
            ceremonies: CeremonyStore::new(CEREMONY_CAP, CEREMONY_TTL),
            ip_limiter: RateLimiter::new(IP_BURST, IP_REFILL, MAX_RATE_KEYS),
            username_limiter: RateLimiter::new(USERNAME_BURST, USERNAME_REFILL, MAX_RATE_KEYS),
        })
    }
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0; N];
    getrandom::getrandom(&mut buf).expect("the OS random number generator failed");
    buf
}

pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
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
