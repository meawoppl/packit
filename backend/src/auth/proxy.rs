//! Which requests may name their client through X-Forwarded-For.
//!
//! The deployment contract: clients reach the backend only through one
//! Traefik hop. Traefik appends the address it saw to X-Forwarded-For, and
//! overwrites X-Packit-Proxy-Token with the shared `TRUSTED_PROXY_TOKEN` on
//! every request. Trust is decided once, at the outermost layer, which then
//! strips the token header so nothing below it, logs included, can see it.
//!
//! - Token configured: X-Forwarded-For is trusted only when exactly one token
//!   header carries exactly the token. If `TRUSTED_PROXY` is also set, the
//!   socket peer must match it too.
//! - Only `TRUSTED_PROXY`: trusted when the socket peer is that address.
//! - Neither: never trusted; both headers are ignored.

use crate::AppState;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, HeaderName};
use axum::middleware::Next;
use axum::response::Response;
use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use subtle::ConstantTimeEq;

pub const TOKEN_HEADER: HeaderName = HeaderName::from_static("x-packit-proxy-token");

/// The secret the proxy presents. Its `Debug` never shows the value.
#[derive(Clone)]
pub struct ProxyToken(String);

impl ProxyToken {
    /// 32 to 256 visible ASCII characters. Whitespace and commas are refused,
    /// since combined header values are comma-separated.
    pub fn parse(raw: &str) -> Result<Self, &'static str> {
        let valid = (32..=256).contains(&raw.len())
            && raw.bytes().all(|b| b.is_ascii_graphic() && b != b',');
        if valid {
            Ok(Self(raw.to_string()))
        } else {
            Err("TRUSTED_PROXY_TOKEN must be 32 to 256 visible ASCII characters, without whitespace or commas")
        }
    }
}

impl fmt::Debug for ProxyToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ProxyToken(<redacted>)")
    }
}

/// How to recognize requests from the trusted proxy.
#[derive(Debug, Clone, Default)]
pub struct ProxyTrust {
    pub ip: Option<IpAddr>,
    pub token: Option<ProxyToken>,
}

/// The outcome of [`ProxyTrust::check`].
#[derive(Debug, PartialEq, Eq)]
pub struct Verdict {
    /// X-Forwarded-For on this request can be believed.
    pub trusted: bool,
    /// A token header arrived that no configured token matches.
    pub bad_token: bool,
}

impl ProxyTrust {
    pub fn is_configured(&self) -> bool {
        self.ip.is_some() || self.token.is_some()
    }

    pub fn check(&self, peer: Option<IpAddr>, headers: &HeaderMap) -> Verdict {
        let from_proxy_ip =
            |ip: IpAddr| peer.is_some_and(|p| p.to_canonical() == ip.to_canonical());
        let mut sent = headers.get_all(TOKEN_HEADER).iter();
        let (first, extra) = (sent.next(), sent.next());
        match (&self.token, first) {
            (None, header) => Verdict {
                trusted: self.ip.is_some_and(from_proxy_ip),
                bad_token: header.is_some(),
            },
            (Some(_), None) => Verdict {
                trusted: false,
                bad_token: false,
            },
            (Some(token), Some(value)) => {
                let exact = extra.is_none()
                    && !value.as_bytes().contains(&b',')
                    && bool::from(token.0.as_bytes().ct_eq(value.as_bytes()));
                Verdict {
                    trusted: exact && self.ip.is_none_or(from_proxy_ip),
                    bad_token: !exact,
                }
            }
        }
    }
}

/// Whether the edge layer found this request to come from the trusted proxy.
#[derive(Debug, Clone, Copy)]
pub struct ViaTrustedProxy(pub bool);

/// The outermost layer: decide proxy trust, then strip the token header
/// before any other layer, handler or log sees the request.
pub async fn edge(State(state): State<Arc<AppState>>, mut req: Request, next: Next) -> Response {
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip());
    let trusted = state.auth.via_trusted_proxy(peer, req.headers());
    req.headers_mut().remove(TOKEN_HEADER);
    req.extensions_mut().insert(ViaTrustedProxy(trusted));
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PublicOrigin;
    use crate::test_support::{unconnected_pool, TEST_URL};
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use axum::routing::get;
    use axum::{middleware, Router};
    use std::sync::atomic::Ordering;
    use std::sync::Mutex;
    use tower::ServiceExt;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const PROXY: &str = "10.0.0.2";
    const REAL: &str = "203.0.113.9";

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn token() -> Option<ProxyToken> {
        Some(ProxyToken::parse(TOKEN).unwrap())
    }

    fn headers(tokens: &[&[u8]], forwarded: Option<&str>) -> HeaderMap {
        let mut h = HeaderMap::new();
        for t in tokens {
            h.append(
                TOKEN_HEADER,
                axum::http::HeaderValue::from_bytes(t).unwrap(),
            );
        }
        if let Some(f) = forwarded {
            h.append("x-forwarded-for", f.parse().unwrap());
        }
        h
    }

    fn auth(proxy: ProxyTrust) -> crate::auth::Auth {
        crate::auth::Auth::new(PublicOrigin::parse(TEST_URL, false).unwrap(), proxy).unwrap()
    }

    /// The address a request is rate limited by, deciding trust as the edge
    /// layer does.
    fn keyed_on(auth: &crate::auth::Auth, peer: &str, h: &HeaderMap) -> IpAddr {
        let trusted = auth.via_trusted_proxy(Some(ip(peer)), h);
        auth.client(ip(peer), h, trusted).subnet
    }

    #[test]
    fn tokens_must_be_long_visible_ascii_without_commas() {
        for good in [
            TOKEN,
            &"x".repeat(32),
            &"~".repeat(256),
            "!#$%&'()*+-./0123456789:;<=>?@AZ[]^_`az{|}",
        ] {
            assert!(ProxyToken::parse(good).is_ok(), "{good}");
        }
        for bad in [
            "",
            &"x".repeat(31),
            &"x".repeat(257),
            &format!("{} ", &TOKEN[..40]),
            &format!("{}\t", &TOKEN[..40]),
            &format!("{},{}", &TOKEN[..20], &TOKEN[..20]),
            &format!("{}\u{e9}", &TOKEN[..40]),
        ] {
            assert!(ProxyToken::parse(bad).is_err(), "{bad:?}");
        }
        assert_eq!(
            format!(
                "{:?}",
                ProxyTrust {
                    ip: None,
                    token: token()
                }
            ),
            "ProxyTrust { ip: None, token: Some(ProxyToken(<redacted>)) }"
        );
    }

    #[test]
    fn token_mode_trusts_only_the_exact_token() {
        let trust = ProxyTrust {
            ip: None,
            token: token(),
        };
        let check = |tokens: &[&[u8]]| trust.check(Some(ip(PROXY)), &headers(tokens, None));
        assert_eq!(
            check(&[TOKEN.as_bytes()]),
            Verdict {
                trusted: true,
                bad_token: false
            }
        );
        // Missing: a direct client, which is expected and not suspicious.
        assert_eq!(
            check(&[]),
            Verdict {
                trusted: false,
                bad_token: false
            }
        );
        let forged = TOKEN.replace('0', "1");
        let combined = format!("{TOKEN}, {TOKEN}");
        for tokens in [
            &[forged.as_bytes()][..],
            &[&TOKEN.as_bytes()[..63]],
            &[format!("{TOKEN}0").as_bytes()],
            &[b""],
            &[combined.as_bytes()],
            &[TOKEN.as_bytes(), TOKEN.as_bytes()],
            &[TOKEN.as_bytes(), b"other"],
        ] {
            assert_eq!(
                check(tokens),
                Verdict {
                    trusted: false,
                    bad_token: true
                },
                "{tokens:?}"
            );
        }
        // Without a socket peer the token alone decides.
        let h = headers(&[TOKEN.as_bytes()], None);
        assert!(trust.check(None, &h).trusted);
    }

    #[test]
    fn token_and_ip_must_both_match() {
        let trust = ProxyTrust {
            ip: Some(ip(PROXY)),
            token: token(),
        };
        let h = headers(&[TOKEN.as_bytes()], None);
        assert!(trust.check(Some(ip(PROXY)), &h).trusted);
        assert!(trust.check(Some(ip("::ffff:10.0.0.2")), &h).trusted);
        assert!(!trust.check(Some(ip(REAL)), &h).trusted);
        assert!(!trust.check(None, &h).trusted);
        // The right peer without the token isn't enough either.
        assert!(!trust.check(Some(ip(PROXY)), &headers(&[], None)).trusted);
    }

    #[test]
    fn ip_only_mode_trusts_the_peer() {
        let trust = ProxyTrust {
            ip: Some(ip(PROXY)),
            token: None,
        };
        assert!(trust.check(Some(ip(PROXY)), &headers(&[], None)).trusted);
        assert!(!trust.check(Some(ip(REAL)), &headers(&[], None)).trusted);
        assert!(!trust.check(None, &headers(&[], None)).trusted);
        // A stray token header changes nothing, but is reported.
        let stray = trust.check(Some(ip(PROXY)), &headers(&[TOKEN.as_bytes()], None));
        assert_eq!(
            stray,
            Verdict {
                trusted: true,
                bad_token: true
            }
        );
    }

    #[test]
    fn with_nothing_configured_both_headers_are_ignored() {
        let auth = auth(ProxyTrust::default());
        let h = headers(&[TOKEN.as_bytes()], Some("1.2.3.4, 203.0.113.9"));
        assert_eq!(keyed_on(&auth, PROXY, &h), ip(PROXY));
        assert_eq!(keyed_on(&auth, REAL, &h), ip(REAL));
    }

    #[test]
    fn a_trusted_request_keys_on_the_rightmost_forwarded_entry() {
        let auth = auth(ProxyTrust {
            ip: None,
            token: token(),
        });
        let good = |f: &str| headers(&[TOKEN.as_bytes()], Some(f));
        assert_eq!(
            keyed_on(&auth, PROXY, &good("1.2.3.4, 203.0.113.9")),
            ip(REAL)
        );
        assert_eq!(
            keyed_on(&auth, PROXY, &good("1.2.3.4, ::ffff:203.0.113.9")),
            ip(REAL)
        );
        // A trusted request with a garbage last entry falls back to the peer.
        for garbage in ["203.0.113.9, junk", "203.0.113.9,", "unknown"] {
            assert_eq!(
                keyed_on(&auth, PROXY, &good(garbage)),
                ip(PROXY),
                "{garbage}"
            );
        }
        // Without the exact token the header is ignored.
        let forged = headers(&[b"forged-forged-forged-forged-forged"], Some(REAL));
        assert_eq!(keyed_on(&auth, PROXY, &forged), ip(PROXY));
        assert_eq!(keyed_on(&auth, PROXY, &headers(&[], Some(REAL))), ip(PROXY));

        // IP-only mode works as before.
        let auth = super::tests::auth(ProxyTrust {
            ip: Some(ip(PROXY)),
            token: None,
        });
        let h = headers(&[], Some("1.2.3.4, 203.0.113.9"));
        assert_eq!(keyed_on(&auth, PROXY, &h), ip(REAL));
        assert_eq!(keyed_on(&auth, "198.51.100.5", &h), ip("198.51.100.5"));
    }

    #[test]
    fn bad_tokens_are_reported_once() {
        let auth = auth(ProxyTrust {
            ip: None,
            token: token(),
        });
        assert!(!auth.warned_token.load(Ordering::Relaxed));
        keyed_on(&auth, PROXY, &headers(&[TOKEN.as_bytes()], None));
        keyed_on(&auth, PROXY, &headers(&[], None));
        assert!(!auth.warned_token.load(Ordering::Relaxed));
        for _ in 0..3 {
            keyed_on(&auth, PROXY, &headers(&[b"wrong"], None));
            assert!(auth.warned_token.load(Ordering::Relaxed));
        }
        // A token header with none configured is reported too.
        let auth = super::tests::auth(ProxyTrust::default());
        keyed_on(&auth, PROXY, &headers(&[TOKEN.as_bytes()], None));
        assert!(auth.warned_token.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn the_token_never_reaches_inner_layers_or_handlers() {
        let proxy = ProxyTrust {
            ip: None,
            token: token(),
        };
        let origin = PublicOrigin::parse(TEST_URL, false).unwrap();
        let state = Arc::new(AppState::new(true, unconnected_pool(), origin, proxy).unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = seen.clone();
        // The inner layer stands where request logging would go.
        let app = Router::new()
            .route(
                "/probe",
                get(|h: HeaderMap| async move { h.contains_key(TOKEN_HEADER).to_string() }),
            )
            .layer(middleware::map_request(move |req: Request| {
                let recorder = recorder.clone();
                async move {
                    recorder
                        .lock()
                        .unwrap()
                        .push(req.headers().contains_key(TOKEN_HEADER));
                    req
                }
            }))
            .layer(middleware::from_fn_with_state(state, edge));
        for tokens in [&[TOKEN][..], &[TOKEN, TOKEN], &["wrong"]] {
            let mut req = HttpRequest::get("/probe");
            for t in tokens {
                req = req.header(TOKEN_HEADER, *t);
            }
            let res = app
                .clone()
                .oneshot(req.body(Body::empty()).unwrap())
                .await
                .unwrap();
            let body = axum::body::to_bytes(res.into_body(), usize::MAX)
                .await
                .unwrap();
            assert_eq!(&body[..], b"false", "{tokens:?}");
        }
        assert_eq!(*seen.lock().unwrap(), [false, false, false]);
    }
}
