use anyhow::Context;
use std::env;
use std::net::IpAddr;
use webauthn_rs::prelude::Url;

/// Server configuration resolved from the environment.
///
/// Each value falls back to a sensible default and is logged at startup, so the
/// effective configuration is always visible in the logs.
#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    /// The site origin from `PUBLIC_URL`. Configured rather than taken from
    /// request headers.
    pub public: PublicOrigin,
    /// The one peer allowed to report client addresses via X-Forwarded-For.
    pub trusted_proxy: Option<IpAddr>,
}

impl Config {
    /// Read configuration from the environment, applying defaults and logging
    /// each resolved value. Fails on an unusable `PUBLIC_URL` or `TRUSTED_PROXY`.
    pub fn from_env(dev_mode: bool) -> anyhow::Result<Self> {
        let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = env::var("PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3000);
        let raw_url = env::var("PUBLIC_URL").unwrap_or_else(|_| "https://potatos.txcl.io".into());
        let public = PublicOrigin::parse(&raw_url, dev_mode).map_err(anyhow::Error::msg)?;
        let trusted_proxy = match env::var("TRUSTED_PROXY") {
            Ok(v) if !v.trim().is_empty() => Some(
                v.trim()
                    .parse::<IpAddr>()
                    .with_context(|| format!("TRUSTED_PROXY {v:?} is not an IP address"))?,
            ),
            _ => None,
        };

        tracing::info!("Config: HOST={host}");
        tracing::info!("Config: PORT={port}");
        tracing::info!(
            "Config: PUBLIC_URL={} (RP ID {})",
            public.origin,
            public.host
        );
        match trusted_proxy {
            Some(ip) => tracing::info!("Config: TRUSTED_PROXY={ip}"),
            None => tracing::info!("Config: TRUSTED_PROXY unset; X-Forwarded-For is ignored"),
        }

        Ok(Self {
            host,
            port,
            public,
            trusted_proxy,
        })
    }

    /// The `host:port` address to bind the server to.
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// The canonical site origin. It is the base of absolute links, the WebAuthn
/// relying-party origin, and the only accepted `Origin` for auth requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicOrigin {
    /// `scheme://host[:port]`, without a trailing slash.
    pub origin: String,
    /// The hostname without a port: the WebAuthn RP ID.
    pub host: String,
    /// Whether the origin is https, which cookies need for `Secure`.
    pub https: bool,
}

impl PublicOrigin {
    /// Parse `PUBLIC_URL`. Only a bare origin is accepted: no credentials,
    /// path, query or fragment, and the host must be a domain name. http is
    /// allowed only for `localhost` in dev mode.
    pub fn parse(raw: &str, dev_mode: bool) -> Result<Self, String> {
        let raw = raw.trim();
        let url = Url::parse(raw).map_err(|e| format!("PUBLIC_URL {raw:?} is not a URL: {e}"))?;
        let https = match url.scheme() {
            "https" => true,
            "http" => false,
            other => return Err(format!("PUBLIC_URL scheme must be https, not {other}")),
        };
        if !url.username().is_empty() || url.password().is_some() {
            return Err("PUBLIC_URL must not contain credentials".into());
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err("PUBLIC_URL must not have a query or fragment".into());
        }
        if url.path() != "/" {
            return Err("PUBLIC_URL must be an origin without a path".into());
        }
        let host = url
            .domain()
            .ok_or("PUBLIC_URL host must be a domain name, not an IP address")?;
        if host.ends_with('.') {
            return Err("PUBLIC_URL host must not end with a dot".into());
        }
        let allowed = https || (dev_mode && host == "localhost");
        if !allowed {
            return Err(
                "PUBLIC_URL must use https; http is allowed only for localhost with --dev-mode"
                    .into(),
            );
        }
        Ok(Self {
            origin: url.origin().ascii_serialization(),
            host: host.to_string(),
            https,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use webauthn_rs::prelude::WebauthnBuilder;

    fn ok(raw: &str, dev_mode: bool) -> PublicOrigin {
        PublicOrigin::parse(raw, dev_mode).unwrap_or_else(|e| panic!("{raw}: {e}"))
    }

    #[test]
    fn accepts_bare_origins_and_canonicalizes_them() {
        for (raw, origin, host) in [
            (
                "https://potatos.txcl.io",
                "https://potatos.txcl.io",
                "potatos.txcl.io",
            ),
            (
                "https://potatos.txcl.io/",
                "https://potatos.txcl.io",
                "potatos.txcl.io",
            ),
            (
                " https://Packit.Example ",
                "https://packit.example",
                "packit.example",
            ),
            (
                "https://packit.example:443",
                "https://packit.example",
                "packit.example",
            ),
            (
                "https://packit.example:8443",
                "https://packit.example:8443",
                "packit.example",
            ),
        ] {
            let parsed = ok(raw, false);
            assert_eq!(parsed.origin, origin, "{raw}");
            assert_eq!(parsed.host, host, "{raw}");
            assert!(parsed.https);
            // The dev flag never changes an https parse.
            assert_eq!(ok(raw, true), parsed);
        }
    }

    #[test]
    fn http_only_for_localhost_in_dev_mode() {
        let dev = ok("http://localhost:3000", true);
        assert_eq!(dev.origin, "http://localhost:3000");
        assert_eq!(dev.host, "localhost");
        assert!(!dev.https);
        for (raw, dev_mode) in [
            ("http://localhost:3000", false),
            ("http://packit.example", true),
            ("http://packit.example", false),
            ("http://localhost.packit.example", true),
        ] {
            assert!(PublicOrigin::parse(raw, dev_mode).is_err(), "{raw}");
        }
    }

    #[test]
    fn rejects_everything_but_an_origin() {
        for raw in [
            "",
            "potatos.txcl.io",
            "ftp://packit.example",
            "file:///srv/packit",
            "https://user@packit.example",
            "https://user:pw@packit.example",
            "https://:pw@packit.example",
            "https://packit.example/app",
            "https://packit.example//",
            "https://packit.example/?",
            "https://packit.example/?a=b",
            "https://packit.example/#",
            "https://packit.example/#top",
            "https://packit.example.",
            "https://127.0.0.1",
            "https://[::1]:3000",
            "http://127.0.0.1:3000",
        ] {
            for dev_mode in [false, true] {
                assert!(PublicOrigin::parse(raw, dev_mode).is_err(), "{raw}");
            }
        }
    }

    #[test]
    fn webauthn_accepts_parsed_origins_but_not_ip_hosts() {
        for (raw, dev_mode) in [
            ("https://potatos.txcl.io", false),
            ("https://packit.example:8443", false),
            ("http://localhost:3000", true),
        ] {
            let parsed = ok(raw, dev_mode);
            let url = Url::parse(&parsed.origin).unwrap();
            assert!(WebauthnBuilder::new(&parsed.host, &url).is_ok(), "{raw}");
        }
        // Why 127.0.0.1 is not accepted even in dev mode: webauthn-rs needs a
        // domain as the RP ID.
        let ip = Url::parse("http://127.0.0.1:3000").unwrap();
        assert!(WebauthnBuilder::new("127.0.0.1", &ip).is_err());
    }
}
