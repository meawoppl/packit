use std::env;

/// Server configuration resolved from the environment.
///
/// Each value falls back to a sensible default and is logged at startup, so the
/// effective configuration is always visible in the logs.
#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    /// Origin used for absolute URLs in link previews, without a trailing
    /// slash. Configured rather than taken from request headers.
    pub public_url: String,
}

impl Config {
    /// Read configuration from the environment, applying defaults and logging
    /// each resolved value.
    pub fn from_env() -> Self {
        let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = env::var("PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3000);
        let public_url = env::var("PUBLIC_URL")
            .unwrap_or_else(|_| "https://potatos.txcl.io".to_string())
            .trim_end_matches('/')
            .to_string();

        tracing::info!("Config: HOST={host}");
        tracing::info!("Config: PORT={port}");
        tracing::info!("Config: PUBLIC_URL={public_url}");

        Self {
            host,
            port,
            public_url,
        }
    }

    /// The `host:port` address to bind the server to.
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}
