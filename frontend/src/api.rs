//! Typed wrappers around the backend HTTP API.

use crate::webauthn;
use gloo_net::http::{Request, RequestBuilder, Response};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use shared::{ApiError, AuthMe, AuthUsername, KnownRecord, ScoreDetail, ScoreEntry, SubmitScore};
use std::fmt;
use std::future::Future;
use std::time::Duration;
use uuid::Uuid;
use web_sys::RequestCredentials;

/// Why a request failed, sorted by what the caller should do about it.
#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    /// 401: not signed in, the session expired, or a sign-in was refused.
    SignedOut(String),
    /// 429: try again after the server's Retry-After, if it sent one.
    RateLimited(Option<Duration>),
    /// Anything else, as a message to show.
    Other(String),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SignedOut(message) | Self::Other(message) => f.write_str(message),
            Self::RateLimited(Some(wait)) => write!(
                f,
                "Too many attempts. Try again in {} seconds.",
                wait.as_secs().max(1)
            ),
            Self::RateLimited(None) => f.write_str("Too many attempts. Try again later."),
        }
    }
}

/// The session cookie goes with every request that may need it.
/// Same-origin is fetch's default; it's stated so it stays that way.
fn with_session(request: RequestBuilder) -> RequestBuilder {
    request.credentials(RequestCredentials::SameOrigin)
}

fn network(e: gloo_net::Error) -> Failure {
    Failure::Other(e.to_string())
}

/// A response's Retry-After as a wait, if it has a readable one.
fn wait_from(resp: &Response) -> Option<Duration> {
    let value = resp.headers().get("retry-after")?;
    retry_after(&value, js_sys::Date::now(), |date| {
        let at = js_sys::Date::parse(date);
        at.is_finite().then_some(at)
    })
}

/// Classify a failed response by its status, with the server's message.
async fn failure(resp: Response) -> Failure {
    let status = resp.status();
    if status == 429 {
        return Failure::RateLimited(wait_from(&resp));
    }
    let message = match resp.json::<ApiError>().await {
        Ok(err) => err.error,
        Err(_) => format!("HTTP {status}"),
    };
    if status == 401 {
        Failure::SignedOut(message)
    } else {
        Failure::Other(message)
    }
}

async fn decode<T: DeserializeOwned>(resp: Response) -> Result<T, Failure> {
    if resp.ok() {
        resp.json::<T>()
            .await
            .map_err(|e| Failure::Other(e.to_string()))
    } else {
        Err(failure(resp).await)
    }
}

async fn post<B: Serialize + ?Sized, T: DeserializeOwned>(
    url: &str,
    body: &B,
) -> Result<T, Failure> {
    let resp = with_session(Request::post(url))
        .json(body)
        .map_err(network)?
        .send()
        .await
        .map_err(network)?;
    decode(resp).await
}

/// Save a score for the signed-in account.
pub async fn submit_score(body: SubmitScore) -> Result<ScoreEntry, Failure> {
    post("/api/scores", &body).await
}

/// Options for the browser's passkey call, and the ceremony to finish.
#[derive(Deserialize)]
struct Started {
    ceremony: String,
    options: serde_json::Value,
}

#[derive(Serialize)]
struct Finish {
    ceremony: String,
    credential: serde_json::Value,
}

/// The signed-in username, or `None` when signed out.
pub async fn me() -> Result<Option<String>, Failure> {
    let resp = with_session(Request::get("/api/auth/me"))
        .send()
        .await
        .map_err(network)?;
    match decode::<AuthMe>(resp).await {
        Ok(me) => Ok(Some(me.username)),
        Err(Failure::SignedOut(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// A passkey ceremony up to its finish. Preparing it leaves the session
/// alone; [`finish`] is the request that sets the session cookie.
pub struct Prepared {
    finish: &'static str,
    ceremony: String,
    credential: serde_json::Value,
}

/// Start an account with a new passkey, up to its finish.
pub async fn prepare_registration(username: &str) -> Result<Prepared, Failure> {
    let body = AuthUsername {
        username: username.into(),
    };
    let started: Started = post("/api/auth/register/start", &body).await?;
    Ok(Prepared {
        finish: "/api/auth/register/finish",
        credential: webauthn::create(&started.options).await?,
        ceremony: started.ceremony,
    })
}

/// Sign in to `username` with one of its passkeys, up to the finish.
pub async fn prepare_sign_in(username: &str) -> Result<Prepared, Failure> {
    let body = AuthUsername {
        username: username.into(),
    };
    let started: Started = post("/api/auth/login/start", &body).await?;
    Ok(Prepared {
        finish: "/api/auth/login/finish",
        credential: webauthn::get(&started.options).await?,
        ceremony: started.ceremony,
    })
}

/// Finish a prepared ceremony and return the account's username. For a
/// sign-in or registration this sets the session cookie.
pub async fn finish(prepared: Prepared) -> Result<String, Failure> {
    let body = Finish {
        ceremony: prepared.ceremony,
        credential: prepared.credential,
    };
    let me: AuthMe = post(prepared.finish, &body).await?;
    Ok(me.username)
}

/// Add another passkey to the signed-in account. The server wants a recent
/// sign-in for this. It doesn't change the session.
pub async fn add_passkey() -> Result<(), Failure> {
    let resp = with_session(Request::post("/api/auth/passkeys/start"))
        .send()
        .await
        .map_err(network)?;
    let started: Started = decode(resp).await?;
    finish(Prepared {
        finish: "/api/auth/passkeys/finish",
        credential: webauthn::create(&started.options).await?,
        ceremony: started.ceremony,
    })
    .await?;
    Ok(())
}

pub async fn sign_out() -> Result<(), Failure> {
    let resp = with_session(Request::post("/api/auth/logout"))
        .send()
        .await
        .map_err(network)?;
    if resp.ok() {
        Ok(())
    } else {
        Err(failure(resp).await)
    }
}

pub async fn list_scores(n: Option<u32>, limit: Option<u32>) -> Result<Vec<ScoreEntry>, String> {
    let mut query = Vec::new();
    if let Some(n) = n {
        query.push(format!("n={n}"));
    }
    if let Some(limit) = limit {
        query.push(format!("limit={limit}"));
    }
    let url = format!("/api/scores?{}", query.join("&"));
    let resp = Request::get(&url).send().await.map_err(|e| e.to_string())?;
    decode(resp).await.map_err(|e| e.to_string())
}

pub async fn get_score(id: Uuid) -> Result<ScoreDetail, String> {
    let resp = Request::get(&format!("/api/scores/{id}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode(resp).await.map_err(|e| e.to_string())
}

pub async fn known_records() -> Result<Vec<KnownRecord>, String> {
    let resp = Request::get("/api/records")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode(resp).await.map_err(|e| e.to_string())
}

/// Bounded retries: at most `attempts` requests, each cut off after
/// `timeout`, all within one `budget` that also covers the waits between.
pub struct Retry {
    pub attempts: u32,
    pub budget: Duration,
    pub timeout: Duration,
    /// First wait between attempts, doubling each time unless the server
    /// sends Retry-After.
    pub backoff: Duration,
}

#[cfg(not(all(test, target_arch = "wasm32")))]
pub const SHARE_RETRY: Retry = Retry {
    attempts: 4,
    budget: Duration::from_secs(20),
    timeout: Duration::from_secs(8),
    backoff: Duration::from_millis(400),
};

/// Browser tests run the same policy with shorter timings.
#[cfg(all(test, target_arch = "wasm32"))]
pub const SHARE_RETRY: Retry = Retry {
    attempts: 4,
    budget: Duration::from_secs(6),
    timeout: Duration::from_millis(600),
    backoff: Duration::from_millis(400),
};

/// How one attempt ended.
#[derive(Debug, PartialEq)]
pub enum Attempt<T> {
    Done(T),
    /// Worth another try: a network error, a timeout, or a 408, 425, 429 or
    /// 5xx status. `after` is the server's Retry-After, if it sent one.
    Transient {
        after: Option<Duration>,
    },
    /// Retrying won't help: the server rejected the request.
    Permanent,
    /// A 401: only signing in again will help.
    SignedOut,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RetryError {
    /// Attempts or time ran out, or the server refused.
    Failed,
    /// The server needs a sign-in first.
    SignedOut,
    /// The caller went away, so nothing more should happen.
    Cancelled,
}

/// Run `attempt` under `policy`. Each attempt is told how long it may take;
/// `elapsed` reports the time since the start and `sleep` waits. A
/// Retry-After is honored exactly: if it would outlast the budget, this
/// gives up rather than retrying early. `cancelled` is checked before every
/// attempt and after every wait.
pub async fn retry<T, A, AF, S, SF>(
    policy: &Retry,
    elapsed: impl Fn() -> Duration,
    cancelled: impl Fn() -> bool,
    mut attempt: A,
    sleep: S,
) -> Result<T, RetryError>
where
    A: FnMut(Duration) -> AF,
    AF: Future<Output = Attempt<T>>,
    S: Fn(Duration) -> SF,
    SF: Future<Output = ()>,
{
    for k in 0..policy.attempts {
        if cancelled() {
            return Err(RetryError::Cancelled);
        }
        let left = policy.budget.saturating_sub(elapsed());
        if left.is_zero() {
            break;
        }
        let after = match attempt(left.min(policy.timeout)).await {
            Attempt::Done(value) => return Ok(value),
            Attempt::SignedOut => return Err(RetryError::SignedOut),
            Attempt::Permanent => break,
            Attempt::Transient { after } => after,
        };
        if k + 1 == policy.attempts {
            break;
        }
        let wait = after.unwrap_or(policy.backoff * 2u32.pow(k));
        if wait >= policy.budget.saturating_sub(elapsed()) {
            break;
        }
        sleep(wait).await;
        if cancelled() {
            return Err(RetryError::Cancelled);
        }
    }
    Err(RetryError::Failed)
}

/// Statuses worth retrying: request timeouts, rate limits and server errors.
pub fn transient_status(status: u16) -> bool {
    matches!(status, 408 | 425 | 429 | 500..=599)
}

/// A Retry-After header as a wait: either delta-seconds, or an HTTP-date,
/// read with `parse_date` (Unix milliseconds) against `now_ms`.
pub fn retry_after(
    value: &str,
    now_ms: f64,
    parse_date: impl Fn(&str) -> Option<f64>,
) -> Option<Duration> {
    let value = value.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let at = parse_date(value)?;
    // A date too far out saturates; it outlasts any budget either way.
    Some(Duration::try_from_secs_f64(((at - now_ms) / 1000.0).max(0.0)).unwrap_or(Duration::MAX))
}

/// Create a short link for `body` under [`SHARE_RETRY`]. Every attempt
/// sends the same snapshot; `cancelled` stops further attempts.
pub async fn create_share(
    body: shared::CreateShare,
    cancelled: impl Fn() -> bool,
) -> Result<shared::ShortShare, RetryError> {
    // The budget runs on the monotonic clock; wall time only reads HTTP-dates.
    let performance = web_sys::window().and_then(|w| w.performance());
    let now = move || {
        performance
            .as_ref()
            .map_or_else(js_sys::Date::now, |p| p.now())
    };
    let start = now();
    let elapsed = move || Duration::from_secs_f64(((now() - start) / 1000.0).max(0.0));
    retry(
        &SHARE_RETRY,
        elapsed,
        cancelled,
        |timeout| share_attempt(&body, timeout),
        gloo_timers::future::sleep,
    )
    .await
}

/// `fut`'s output, or `None` if `timeout` passes first.
async fn within<F: Future>(timeout: Duration, fut: F) -> Option<F::Output> {
    let mut fut = std::pin::pin!(fut);
    let mut timer = std::pin::pin!(gloo_timers::future::sleep(timeout));
    std::future::poll_fn(|cx| {
        if let std::task::Poll::Ready(value) = fut.as_mut().poll(cx) {
            return std::task::Poll::Ready(Some(value));
        }
        timer.as_mut().poll(cx).map(|()| None)
    })
    .await
}

async fn share_attempt(
    body: &shared::CreateShare,
    timeout: Duration,
) -> Attempt<shared::ShortShare> {
    // The race below enforces the deadline; aborting also makes the browser
    // drop the request, when AbortController is available.
    let controller = web_sys::AbortController::new().ok();
    let signal = controller.as_ref().map(|c| c.signal());
    let Ok(request) = with_session(Request::post("/api/shares"))
        .abort_signal(signal.as_ref())
        .json(body)
    else {
        return Attempt::Permanent;
    };
    let outcome = within(timeout, async {
        let Ok(resp) = request.send().await else {
            return Attempt::Transient { after: None };
        };
        // A failure is decided by its status and headers alone; its body is
        // never read, so a slow or broken one can't change the outcome.
        if !resp.ok() {
            if resp.status() == 401 {
                return Attempt::SignedOut;
            }
            if !transient_status(resp.status()) {
                return Attempt::Permanent;
            }
            return Attempt::Transient {
                after: wait_from(&resp),
            };
        }
        // A success's body that never arrives is a transport failure; one
        // that isn't a short link won't improve on retry.
        let Ok(text) = resp.text().await else {
            return Attempt::Transient { after: None };
        };
        serde_json::from_str(&text).map_or(Attempt::Permanent, Attempt::Done)
    })
    .await;
    outcome.unwrap_or_else(|| {
        if let Some(controller) = controller {
            controller.abort();
        }
        Attempt::Transient { after: None }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};

    fn block_on<F: Future>(f: F) -> F::Output {
        let mut f = pin!(f);
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            if let Poll::Ready(value) = f.as_mut().poll(&mut cx) {
                return value;
            }
        }
    }

    const FAST: Duration = Duration::from_millis(50);

    fn transient() -> Attempt<u32> {
        Attempt::Transient { after: None }
    }

    /// Run `retry` under [`SHARE_RETRY`] against a scripted server on a fake
    /// clock. Each entry is how long the attempt runs and how it ends; an
    /// attempt never runs past the time it's given, as the abort ensures.
    /// Returns the result, each attempt's allowed time, and each wait.
    fn run(
        script: Vec<(Duration, Attempt<u32>)>,
        cancel_after: Option<usize>,
    ) -> (Result<u32, RetryError>, Vec<Duration>, Vec<Duration>) {
        let clock = Cell::new(Duration::ZERO);
        let timeouts = RefCell::new(Vec::new());
        let waits = RefCell::new(Vec::new());
        let script = RefCell::new(script.into_iter());
        let result = block_on(retry(
            &SHARE_RETRY,
            || clock.get(),
            || cancel_after.is_some_and(|n| timeouts.borrow().len() >= n),
            |timeout| {
                timeouts.borrow_mut().push(timeout);
                let (runs, outcome) = script.borrow_mut().next().expect("attempt scripted");
                clock.set(clock.get() + runs.min(timeout));
                std::future::ready(outcome)
            },
            |wait| {
                waits.borrow_mut().push(wait);
                clock.set(clock.get() + wait);
                std::future::ready(())
            },
        ));
        (result, timeouts.into_inner(), waits.into_inner())
    }

    #[test]
    fn a_network_error_then_success_retries_once() {
        let (result, timeouts, waits) =
            run(vec![(FAST, transient()), (FAST, Attempt::Done(7))], None);
        assert_eq!(result, Ok(7));
        assert_eq!(timeouts.len(), 2);
        assert_eq!(waits, [Duration::from_millis(400)]);
    }

    #[test]
    fn repeated_server_errors_exhaust_the_attempts_with_backoff() {
        let (result, timeouts, waits) = run((0..4).map(|_| (FAST, transient())).collect(), None);
        assert_eq!(result, Err(RetryError::Failed));
        assert_eq!(timeouts.len(), 4);
        assert_eq!(waits, [400, 800, 1600].map(Duration::from_millis));
    }

    #[test]
    fn a_permanent_failure_is_not_retried() {
        let (result, timeouts, waits) = run(vec![(FAST, Attempt::Permanent)], None);
        assert_eq!(result, Err(RetryError::Failed));
        assert_eq!(timeouts.len(), 1);
        assert!(waits.is_empty());
    }

    #[test]
    fn a_sign_in_request_is_not_retried() {
        let (result, timeouts, waits) = run(vec![(FAST, Attempt::SignedOut)], None);
        assert_eq!(result, Err(RetryError::SignedOut));
        assert_eq!(timeouts.len(), 1);
        assert!(waits.is_empty());
    }

    #[test]
    fn failures_read_as_messages() {
        let wait = Failure::RateLimited(Some(Duration::from_secs(7)));
        assert_eq!(
            wait.to_string(),
            "Too many attempts. Try again in 7 seconds."
        );
        assert_eq!(
            Failure::RateLimited(None).to_string(),
            "Too many attempts. Try again later."
        );
        assert_eq!(
            Failure::SignedOut("Sign-in failed".into()).to_string(),
            "Sign-in failed"
        );
    }

    #[test]
    fn retry_after_is_honored_exactly() {
        let wait = Duration::from_secs(3);
        let (result, _, waits) = run(
            vec![
                (FAST, Attempt::Transient { after: Some(wait) }),
                (FAST, Attempt::Done(1)),
            ],
            None,
        );
        assert_eq!(result, Ok(1));
        assert_eq!(waits, [wait]);
    }

    #[test]
    fn a_retry_after_past_the_budget_gives_up_instead_of_retrying_early() {
        let later = Attempt::Transient {
            after: Some(Duration::from_secs(60)),
        };
        let (result, timeouts, waits) = run(vec![(FAST, later)], None);
        assert_eq!(result, Err(RetryError::Failed));
        assert_eq!(timeouts.len(), 1);
        assert!(waits.is_empty());
    }

    #[test]
    fn hung_requests_are_cut_off_within_the_budget() {
        let hang = Duration::from_secs(3600);
        let (result, timeouts, waits) = run((0..4).map(|_| (hang, transient())).collect(), None);
        assert_eq!(result, Err(RetryError::Failed));
        // Each attempt gets the timeout, or whatever is left of the budget.
        assert_eq!(
            timeouts,
            [8000, 8000, 2800].map(Duration::from_millis),
            "{waits:?}"
        );
        let total: Duration = timeouts.iter().chain(&waits).sum();
        assert!(total <= SHARE_RETRY.budget, "{total:?}");
    }

    #[test]
    fn an_enormous_retry_after_gives_up_without_overflowing() {
        let forever = Attempt::Transient {
            after: Some(Duration::from_secs(u64::MAX)),
        };
        let (result, timeouts, waits) = run(vec![(FAST, forever)], None);
        assert_eq!(result, Err(RetryError::Failed));
        assert_eq!(timeouts.len(), 1);
        assert!(waits.is_empty());
        let far = |_: &str| Some(f64::MAX);
        assert_eq!(retry_after("someday", 0.0, far), Some(Duration::MAX));
    }

    #[test]
    fn cancelling_during_a_wait_sends_nothing_more() {
        let (result, timeouts, waits) =
            run(vec![(FAST, transient()), (FAST, Attempt::Done(1))], Some(1));
        assert_eq!(result, Err(RetryError::Cancelled));
        assert_eq!(timeouts.len(), 1, "no request after cancelling");
        assert_eq!(waits.len(), 1);
    }

    #[test]
    fn retry_after_reads_seconds_and_http_dates() {
        let now = 1_000_000.0;
        let date = |s: &str| (s == "Wed, 21 Oct 2015 07:28:00 GMT").then_some(now + 5_000.0);
        assert_eq!(
            retry_after("120", now, date),
            Some(Duration::from_secs(120))
        );
        assert_eq!(retry_after(" 7 ", now, date), Some(Duration::from_secs(7)));
        assert_eq!(
            retry_after("Wed, 21 Oct 2015 07:28:00 GMT", now, date),
            Some(Duration::from_secs(5))
        );
        assert_eq!(retry_after("soon", now, date), None);
        let past = |_: &str| Some(now - 9_000.0);
        assert_eq!(
            retry_after("Tue, 20 Oct 2015", now, past),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn only_timeouts_rate_limits_and_server_errors_are_transient() {
        for status in [408, 425, 429, 500, 502, 503, 504] {
            assert!(transient_status(status), "{status}");
        }
        for status in [400, 401, 403, 404, 413, 422] {
            assert!(!transient_status(status), "{status}");
        }
    }
}
