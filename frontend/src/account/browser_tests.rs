//! In-browser tests of the account control and the WebAuthn bridge, against
//! a stubbed API and a fake `navigator.credentials`. Real passkey crypto is
//! covered by the backend's tests. The stubs are shared with the play
//! screen's tests.

use super::*;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use js_sys::{Object, Promise, Reflect, Uint8Array};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::*;
use web_sys::{Element, HtmlElement};

wasm_bindgen_test_configure!(run_in_browser);

/// One scripted API reply.
#[derive(Clone)]
pub(crate) struct Reply {
    pub status: u16,
    pub body: String,
    pub retry_after: Option<&'static str>,
    /// Milliseconds before it answers, to deliver responses out of order.
    pub delay: u64,
}

impl Reply {
    pub(crate) fn after(self, delay: u64) -> Self {
        Self { delay, ..self }
    }
}

pub(crate) fn reply(status: u16, body: Value) -> Reply {
    Reply {
        status,
        body: body.to_string(),
        retry_after: None,
        delay: 0,
    }
}

/// A reply with no body, like logout's 204.
pub(crate) fn empty(status: u16) -> Reply {
    Reply {
        status,
        body: String::new(),
        retry_after: None,
        delay: 0,
    }
}

type Fetch = Closure<dyn FnMut(JsValue, JsValue) -> Promise>;

/// Stubs `fetch` for the scripted paths; other requests go to the `fetch`
/// it replaced, so it stacks on other stubs. Each path answers from its
/// own script in order, repeating the last reply.
pub(crate) struct Api {
    original: JsValue,
    _fetch: Fetch,
    /// Every stubbed request: its path and body.
    requests: Rc<RefCell<Vec<(String, String)>>>,
}

impl Api {
    pub(crate) fn install(routes: &[(&'static str, Vec<Reply>)]) -> Self {
        let routes: HashMap<&'static str, Vec<Reply>> = routes.iter().cloned().collect();
        let window = web_sys::window().unwrap();
        let original = Reflect::get(&window, &"fetch".into()).unwrap();
        let fetch = original.clone().dyn_into::<js_sys::Function>().unwrap();
        let requests = Rc::new(RefCell::new(Vec::<(String, String)>::new()));
        let captured = requests.clone();
        let replacement = Closure::wrap(Box::new(move |request: JsValue, init: JsValue| {
            let req = request.clone().dyn_into::<web_sys::Request>().unwrap();
            let path = web_sys::Url::new(&req.url()).unwrap().pathname();
            let Some(script) = routes.get(path.as_str()).cloned() else {
                return fetch
                    .call2(&web_sys::window().unwrap(), &request, &init)
                    .unwrap()
                    .unchecked_into();
            };
            let captured = captured.clone();
            wasm_bindgen_futures::future_to_promise(async move {
                let body = wasm_bindgen_futures::JsFuture::from(req.text()?)
                    .await?
                    .as_string()
                    .unwrap_or_default();
                let reply = {
                    let mut captured = captured.borrow_mut();
                    let sent = captured.iter().filter(|(p, _)| *p == path).count();
                    captured.push((path, body));
                    script[sent.min(script.len() - 1)].clone()
                };
                sleep(reply.delay).await;
                let options = web_sys::ResponseInit::new();
                options.set_status(reply.status);
                let body = (!reply.body.is_empty()).then_some(reply.body.as_str());
                let response = web_sys::Response::new_with_opt_str_and_init(body, &options)?;
                if let Some(after) = reply.retry_after {
                    response.headers().set("retry-after", after)?;
                }
                Ok(response.into())
            })
        }) as Box<dyn FnMut(JsValue, JsValue) -> Promise>);
        Reflect::set(&window, &"fetch".into(), replacement.as_ref()).unwrap();
        Self {
            original,
            _fetch: replacement,
            requests,
        }
    }

    /// The bodies sent to `path`, in order.
    pub(crate) fn sent(&self, path: &str) -> Vec<String> {
        self.requests
            .borrow()
            .iter()
            .filter(|(p, _)| p == path)
            .map(|(_, body)| body.clone())
            .collect()
    }
}

impl Drop for Api {
    fn drop(&mut self) {
        Reflect::set(&web_sys::window().unwrap(), &"fetch".into(), &self.original).unwrap();
    }
}

pub(crate) fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn buffer(bytes: &[u8]) -> JsValue {
    Uint8Array::from(bytes).buffer().into()
}

fn set(object: &JsValue, key: &str, value: impl Into<JsValue>) {
    Reflect::set(object, &key.into(), &value.into()).unwrap();
}

/// The credential a fake authenticator returns, in the shape of the
/// browser's `PublicKeyCredential`.
fn fake_credential(creating: bool) -> JsValue {
    let response: JsValue = Object::new().into();
    set(&response, "clientDataJSON", buffer(b"client-data"));
    if creating {
        set(&response, "attestationObject", buffer(b"attestation"));
    } else {
        set(&response, "authenticatorData", buffer(b"auth-data"));
        set(&response, "signature", buffer(b"signature"));
        set(&response, "userHandle", JsValue::NULL);
    }
    let credential: JsValue = Object::new().into();
    set(&credential, "id", "AQID");
    set(&credential, "rawId", buffer(&[1, 2, 3]));
    set(&credential, "type", "public-key");
    set(&credential, "response", response);
    credential
}

/// What the page sends to register/finish for [`fake_credential`].
pub(crate) fn created_credential() -> Value {
    json!({
        "id": "AQID",
        "rawId": "AQID",
        "type": "public-key",
        "response": {
            "attestationObject": b64(b"attestation"),
            "clientDataJSON": b64(b"client-data"),
        },
    })
}

/// What the page sends to login/finish for [`fake_credential`].
pub(crate) fn asserted_credential() -> Value {
    json!({
        "id": "AQID",
        "rawId": "AQID",
        "type": "public-key",
        "response": {
            "authenticatorData": b64(b"auth-data"),
            "clientDataJSON": b64(b"client-data"),
            "signature": b64(b"signature"),
            "userHandle": null,
        },
    })
}

type Call = Closure<dyn FnMut(JsValue) -> Promise>;

/// A fake `navigator.credentials`. `create` and `get` record the options
/// they get, then resolve to [`fake_credential`], or reject with an error
/// named like a DOMException while `fail` holds a name.
pub(crate) struct Passkeys {
    _create: Call,
    _get: Call,
    /// Each call's method and options.
    pub calls: Rc<RefCell<Vec<(&'static str, JsValue)>>>,
    pub fail: Rc<RefCell<Option<&'static str>>>,
}

impl Passkeys {
    pub(crate) fn install() -> Self {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let fail = Rc::new(RefCell::new(None));
        let method = |name: &'static str| {
            let (calls, fail) = (calls.clone(), fail.clone());
            Closure::wrap(Box::new(move |options: JsValue| {
                calls.borrow_mut().push((name, options));
                match *fail.borrow() {
                    Some(error) => {
                        let e: JsValue = Object::new().into();
                        set(&e, "name", error);
                        set(&e, "message", "refused by the test");
                        Promise::reject(&e)
                    }
                    None => Promise::resolve(&fake_credential(name == "create")),
                }
            }) as Box<dyn FnMut(JsValue) -> Promise>)
        };
        let (create, get) = (method("create"), method("get"));
        let container: JsValue = Object::new().into();
        set(&container, "create", create.as_ref().clone());
        set(&container, "get", get.as_ref().clone());
        let descriptor: JsValue = Object::new().into();
        set(&descriptor, "configurable", true);
        set(&descriptor, "value", container);
        Object::define_property(
            web_sys::window().unwrap().navigator().as_ref(),
            &"credentials".into(),
            descriptor.unchecked_ref(),
        );
        Self {
            _create: create,
            _get: get,
            calls,
            fail,
        }
    }
}

impl Drop for Passkeys {
    fn drop(&mut self) {
        let _ = Reflect::delete_property(
            web_sys::window().unwrap().navigator().as_ref(),
            &"credentials".into(),
        );
    }
}

/// The bytes at `path` (dot-separated keys, digits index arrays) in the
/// options a fake authenticator received.
pub(crate) fn bytes_at(options: &JsValue, path: &str) -> Vec<u8> {
    let value = path.split('.').fold(options.clone(), |v, key| {
        Reflect::get(&v, &key.into()).unwrap()
    });
    assert!(
        value.is_instance_of::<js_sys::ArrayBuffer>(),
        "{path} is an ArrayBuffer"
    );
    Uint8Array::new(&value).to_vec()
}

pub(crate) async fn sleep(ms: u64) {
    gloo_timers::future::sleep(Duration::from_millis(ms)).await;
}

/// Poll `ready` for up to three seconds.
pub(crate) async fn wait_until(what: &str, ready: impl Fn() -> bool) {
    for _ in 0..150 {
        if ready() {
            return;
        }
        sleep(20).await;
    }
    panic!("timed out waiting for {what}");
}

pub(crate) fn find(root: &Element, selector: &str) -> Option<HtmlElement> {
    root.query_selector(selector)
        .unwrap()
        .map(|e| e.dyn_into().unwrap())
}

pub(crate) fn click(root: &Element, selector: &str) {
    find(root, selector)
        .unwrap_or_else(|| panic!("{selector} rendered"))
        .click();
}

pub(crate) fn text_of(root: &Element, selector: &str) -> Option<String> {
    find(root, selector).map(|e| e.text_content().unwrap_or_default())
}

pub(crate) fn type_username(root: &Element, name: &str) {
    let input: HtmlInputElement = find(root, "#account-username")
        .expect("the sign-in dialog is open")
        .unchecked_into();
    input.set_value(name);
    input
        .dispatch_event(&web_sys::Event::new("input").unwrap())
        .unwrap();
}

/// Sign-in options, as webauthn-rs sends them.
pub(crate) fn login_started() -> Reply {
    reply(
        200,
        json!({
            "ceremony": "login-1",
            "options": { "publicKey": {
                "challenge": b64(&[7; 32]),
                "timeout": 60000,
                "rpId": "localhost",
                "allowCredentials": [{ "type": "public-key", "id": b64(&[9, 9]) }],
                "userVerification": "required",
            } },
        }),
    )
}

fn creation_started(ceremony: &str) -> Reply {
    reply(
        200,
        json!({
            "ceremony": ceremony,
            "options": { "publicKey": {
                "rp": { "name": "Potatos", "id": "localhost" },
                "user": { "id": b64(&[5; 16]), "name": "bob", "displayName": "bob" },
                "challenge": b64(&[8; 32]),
                "pubKeyCredParams": [{ "type": "public-key", "alg": -7 }],
                "timeout": 60000,
                "excludeCredentials": [{ "type": "public-key", "id": b64(&[4, 4]) }],
                "authenticatorSelection": {
                    "residentKey": "preferred",
                    "requireResidentKey": false,
                    "userVerification": "required",
                },
                "attestation": "none",
            } },
        }),
    )
}

pub(crate) fn not_signed_in() -> Reply {
    reply(401, json!({ "error": "Not signed in" }))
}

#[function_component(Probe)]
fn probe() -> Html {
    let account = use_context::<Account>().unwrap();
    html! { <span class="probe">{ account.username.unwrap_or_default() }</span> }
}

thread_local! {
    /// Answers to the asks [`Asker`] made, in order.
    static ANSWERS: RefCell<Vec<bool>> = const { RefCell::new(Vec::new()) };
}

const ASK_REASON: &str = "A test needs an account.";

/// A page that asks for a sign-in when its button is pressed.
#[function_component(Asker)]
fn asker() -> Html {
    let account = use_context::<Account>().unwrap();
    let onclick = Callback::from(move |_: MouseEvent| {
        account.ask.emit(SignInAsk {
            reason: ASK_REASON,
            done: Callback::from(|signed_in| ANSWERS.with(|a| a.borrow_mut().push(signed_in))),
        })
    });
    html! { <button class="asker" {onclick}>{ "Ask" }</button> }
}

#[function_component(Host)]
fn host() -> Html {
    html! { <AccountProvider><AccountMenu /><Probe /><Asker /></AccountProvider> }
}

async fn mount() -> (yew::AppHandle<Host>, Element) {
    let document = web_sys::window().unwrap().document().unwrap();
    let root = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&root).unwrap();
    let handle = yew::Renderer::<Host>::with_root(root.clone()).render();
    sleep(50).await;
    (handle, root)
}

fn json_of(body: &str) -> Value {
    serde_json::from_str(body).unwrap()
}

/// Submit the open sign-in form, as Enter in the username field does.
fn submit_form(root: &Element) {
    let init = web_sys::EventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);
    let submit = web_sys::Event::new_with_event_init_dict("submit", &init).unwrap();
    root.query_selector(".account-dialog")
        .unwrap()
        .expect("the sign-in dialog is open")
        .dispatch_event(&submit)
        .unwrap();
}

/// Sign in through the account control, with the dialog closed.
async fn sign_in_as(root: &Element, name: &str) {
    click(root, ".account-open");
    sleep(30).await;
    type_username(root, name);
    sleep(30).await;
    click(root, ".account-sign-in");
    wait_until("the sign-in", || {
        text_of(root, ".account-name").as_deref() == Some(name)
    })
    .await;
}

#[wasm_bindgen_test]
async fn a_slow_me_does_not_overwrite_a_later_sign_in() {
    let _api = Api::install(&[
        (
            "/api/auth/me",
            vec![
                reply(200, json!({ "username": "old" })).after(800),
                reply(200, json!({ "username": "ada" })),
            ],
        ),
        ("/api/auth/login/start", vec![login_started()]),
        (
            "/api/auth/login/finish",
            vec![reply(200, json!({ "username": "ada" }))],
        ),
    ]);
    let _keys = Passkeys::install();
    let (handle, root) = mount().await;
    sign_in_as(&root, "ada").await;
    // The startup /me answers now, for the session before this sign-in.
    sleep(1000).await;
    assert_eq!(text_of(&root, ".account-name").as_deref(), Some("ada"));
    assert_eq!(text_of(&root, ".probe").as_deref(), Some("ada"));
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn a_slow_me_does_not_sign_back_in_after_a_sign_out() {
    let api = Api::install(&[
        (
            "/api/auth/me",
            vec![
                reply(200, json!({ "username": "carol" })).after(1500),
                reply(200, json!({ "username": "ada" })),
                not_signed_in(),
            ],
        ),
        ("/api/auth/login/start", vec![login_started()]),
        (
            "/api/auth/login/finish",
            vec![reply(200, json!({ "username": "ada" }))],
        ),
        ("/api/auth/logout", vec![empty(204)]),
    ]);
    let _keys = Passkeys::install();
    let (handle, root) = mount().await;
    sign_in_as(&root, "ada").await;
    click(&root, ".account-sign-out");
    wait_until("signed out", || find(&root, ".account-open").is_some()).await;
    sleep(1800).await;
    // At startup, then a check after the sign-in and after the sign-out.
    assert_eq!(api.sent("/api/auth/me").len(), 3, "the slow /me answered");
    assert!(find(&root, ".account-name").is_none(), "still signed out");
    assert_eq!(text_of(&root, ".probe").as_deref(), Some(""));
    handle.destroy();
    root.remove();
}

/// A sign-in still finishing when its dialog is cancelled answers neither
/// the cancelled ask nor a newer one. Nothing else starts until it lands,
/// and then its session is signed out again.
#[wasm_bindgen_test]
async fn a_stale_sign_in_never_answers_a_newer_ask() {
    ANSWERS.with(|a| a.borrow_mut().clear());
    let api = Api::install(&[
        (
            "/api/auth/me",
            vec![
                not_signed_in(),
                not_signed_in(),
                reply(200, json!({ "username": "ada" })),
            ],
        ),
        ("/api/auth/login/start", vec![login_started()]),
        (
            "/api/auth/login/finish",
            vec![
                reply(200, json!({ "username": "ada" })).after(800),
                reply(200, json!({ "username": "ada" })),
            ],
        ),
        ("/api/auth/logout", vec![empty(204)]),
    ]);
    let _keys = Passkeys::install();
    let (handle, root) = mount().await;
    click(&root, ".asker");
    sleep(30).await;
    type_username(&root, "ada");
    sleep(30).await;
    click(&root, ".account-sign-in");
    wait_until("the first finish", || {
        api.sent("/api/auth/login/finish").len() == 1
    })
    .await;
    click(&root, ".account-cancel");
    sleep(30).await;
    click(&root, ".asker");
    sleep(30).await;
    assert_eq!(
        text_of(&root, ".account-status").as_deref(),
        Some("Finishing sign-in…")
    );
    submit_form(&root);
    assert!(find(&root, ".account-sign-in")
        .unwrap()
        .has_attribute("disabled"));
    sleep(1100).await;
    assert_eq!(
        api.sent("/api/auth/login/start").len(),
        1,
        "nothing started"
    );
    assert_eq!(
        api.sent("/api/auth/logout").len(),
        1,
        "the cancelled sign-in's session was signed out"
    );
    assert_eq!(ANSWERS.with(|a| a.borrow().clone()), [false]);
    assert!(find(&root, ".account-name").is_none());
    assert!(find(&root, ".account-status").is_none());
    assert_eq!(
        text_of(&root, ".account-reason").as_deref(),
        Some(ASK_REASON)
    );
    // A sign-in for the newer ask answers it, once.
    click(&root, ".account-sign-in");
    wait_until("the sign-in", || {
        text_of(&root, ".account-name").as_deref() == Some("ada")
    })
    .await;
    assert_eq!(ANSWERS.with(|a| a.borrow().clone()), [false, true]);
    handle.destroy();
    root.remove();
}

/// A ceremony cancelled before its finish never sends it, so the session
/// is never touched.
#[wasm_bindgen_test]
async fn a_sign_in_cancelled_before_its_finish_never_sends_it() {
    let api = Api::install(&[
        ("/api/auth/me", vec![not_signed_in()]),
        ("/api/auth/login/start", vec![login_started().after(600)]),
        (
            "/api/auth/login/finish",
            vec![reply(200, json!({ "username": "ada" }))],
        ),
    ]);
    let keys = Passkeys::install();
    let (handle, root) = mount().await;
    click(&root, ".account-open");
    sleep(30).await;
    type_username(&root, "ada");
    sleep(30).await;
    click(&root, ".account-sign-in");
    sleep(100).await;
    click(&root, ".account-cancel");
    sleep(900).await;
    assert_eq!(keys.calls.borrow().len(), 1, "the passkey step ran");
    assert!(api.sent("/api/auth/login/finish").is_empty());
    assert!(find(&root, ".account-name").is_none());
    // Nothing is left holding the lock.
    click(&root, ".account-open");
    sleep(30).await;
    assert!(!find(&root, ".account-sign-in")
        .unwrap()
        .has_attribute("disabled"));
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn enter_while_signing_in_starts_no_second_ceremony() {
    let api = Api::install(&[
        (
            "/api/auth/me",
            vec![not_signed_in(), reply(200, json!({ "username": "ada" }))],
        ),
        ("/api/auth/login/start", vec![login_started()]),
        (
            "/api/auth/login/finish",
            vec![reply(200, json!({ "username": "ada" })).after(600)],
        ),
    ]);
    let keys = Passkeys::install();
    let (handle, root) = mount().await;
    click(&root, ".account-open");
    sleep(30).await;
    type_username(&root, "ada");
    sleep(30).await;
    submit_form(&root);
    sleep(100).await;
    assert!(
        find(&root, ".account-sign-in")
            .unwrap()
            .has_attribute("disabled"),
        "the ceremony is under way"
    );
    submit_form(&root);
    submit_form(&root);
    wait_until("the sign-in", || {
        text_of(&root, ".account-name").as_deref() == Some("ada")
    })
    .await;
    assert_eq!(api.sent("/api/auth/login/start").len(), 1);
    assert_eq!(keys.calls.borrow().len(), 1);
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn signing_in_and_out_through_the_account_control() {
    let api = Api::install(&[
        (
            "/api/auth/me",
            vec![
                not_signed_in(),
                reply(200, json!({ "username": "ada" })),
                not_signed_in(),
            ],
        ),
        ("/api/auth/login/start", vec![login_started()]),
        (
            "/api/auth/login/finish",
            vec![reply(200, json!({ "username": "ada" }))],
        ),
        ("/api/auth/logout", vec![empty(204)]),
    ]);
    let keys = Passkeys::install();
    let (handle, root) = mount().await;
    assert!(find(&root, ".account-dialog").is_none());
    click(&root, ".account-open");
    sleep(30).await;
    assert!(find(&root, ".account-reason").is_none(), "no page asked");
    let help = text_of(&root, ".account-help").unwrap();
    assert!(help.contains("can't be recovered"), "{help}");
    type_username(&root, " Ada ");
    sleep(30).await;
    click(&root, ".account-sign-in");
    wait_until("the username", || {
        text_of(&root, ".account-name").as_deref() == Some("ada")
    })
    .await;
    assert_eq!(text_of(&root, ".probe").as_deref(), Some("ada"));
    assert!(find(&root, ".account-dialog").is_none());
    assert_eq!(
        api.sent("/api/auth/login/start")
            .iter()
            .map(|b| json_of(b))
            .collect::<Vec<_>>(),
        [json!({ "username": "Ada" })]
    );
    let calls = keys.calls.borrow().clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "get");
    assert_eq!(bytes_at(&calls[0].1, "publicKey.challenge"), [7; 32]);
    assert_eq!(
        bytes_at(&calls[0].1, "publicKey.allowCredentials.0.id"),
        [9, 9]
    );
    assert_eq!(
        json_of(&api.sent("/api/auth/login/finish")[0]),
        json!({ "ceremony": "login-1", "credential": asserted_credential() })
    );

    click(&root, ".account-sign-out");
    wait_until("signed out", || find(&root, ".account-open").is_some()).await;
    assert_eq!(api.sent("/api/auth/logout").len(), 1);
    assert_eq!(text_of(&root, ".probe").as_deref(), Some(""));
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn creating_an_account_converts_the_creation_options() {
    let api = Api::install(&[
        (
            "/api/auth/me",
            vec![not_signed_in(), reply(200, json!({ "username": "bob" }))],
        ),
        ("/api/auth/register/start", vec![creation_started("new-1")]),
        (
            "/api/auth/register/finish",
            vec![reply(200, json!({ "username": "bob" }))],
        ),
    ]);
    let keys = Passkeys::install();
    let (handle, root) = mount().await;
    click(&root, ".account-open");
    sleep(30).await;
    type_username(&root, "bob");
    sleep(30).await;
    click(&root, ".account-create");
    wait_until("the username", || {
        text_of(&root, ".account-name").as_deref() == Some("bob")
    })
    .await;
    let calls = keys.calls.borrow();
    assert_eq!(calls[0].0, "create");
    assert_eq!(bytes_at(&calls[0].1, "publicKey.challenge"), [8; 32]);
    assert_eq!(bytes_at(&calls[0].1, "publicKey.user.id"), [5; 16]);
    assert_eq!(
        bytes_at(&calls[0].1, "publicKey.excludeCredentials.0.id"),
        [4, 4]
    );
    assert_eq!(
        json_of(&api.sent("/api/auth/register/finish")[0]),
        json!({ "ceremony": "new-1", "credential": created_credential() })
    );
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn a_session_loads_on_startup_and_can_add_a_passkey() {
    let api = Api::install(&[
        (
            "/api/auth/me",
            vec![reply(200, json!({ "username": "carol" }))],
        ),
        (
            "/api/auth/passkeys/start",
            vec![
                creation_started("add-1"),
                reply(403, json!({ "error": "Sign in again to add a passkey" })),
            ],
        ),
        (
            "/api/auth/passkeys/finish",
            vec![reply(200, json!({ "username": "carol" }))],
        ),
    ]);
    let _keys = Passkeys::install();
    let (handle, root) = mount().await;
    wait_until("the stored session", || {
        text_of(&root, ".account-name").as_deref() == Some("carol")
    })
    .await;
    click(&root, ".account-add");
    wait_until("the passkey", || find(&root, ".account-notice").is_some()).await;
    assert_eq!(
        text_of(&root, ".account-notice").unwrap(),
        "Passkey added. You can sign in with either."
    );
    assert_eq!(
        json_of(&api.sent("/api/auth/passkeys/finish")[0]),
        json!({ "ceremony": "add-1", "credential": created_credential() })
    );
    click(&root, ".account-add");
    wait_until("the refusal", || {
        text_of(&root, ".account-notice").as_deref() == Some("Sign in again to add a passkey")
    })
    .await;
    assert_eq!(api.sent("/api/auth/passkeys/finish").len(), 1);
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn sign_in_failures_read_clearly() {
    let limited = Reply {
        retry_after: Some("7"),
        ..reply(
            429,
            json!({ "error": "Too many attempts; try again later" }),
        )
    };
    let api = Api::install(&[
        ("/api/auth/me", vec![not_signed_in()]),
        ("/api/auth/login/start", vec![limited, login_started()]),
        (
            "/api/auth/register/start",
            vec![reply(
                409,
                json!({ "error": "That username isn't available" }),
            )],
        ),
    ]);
    let keys = Passkeys::install();
    let (handle, root) = mount().await;
    click(&root, ".account-open");
    sleep(30).await;
    type_username(&root, "dave");
    sleep(30).await;
    let error = |expected: &'static str| {
        let root = root.clone();
        move || text_of(&root, ".account-error").as_deref() == Some(expected)
    };
    click(&root, ".account-sign-in");
    wait_until(
        "the rate limit",
        error("Too many attempts. Try again in 7 seconds."),
    )
    .await;
    *keys.fail.borrow_mut() = Some("NotAllowedError");
    click(&root, ".account-sign-in");
    wait_until(
        "the cancelled passkey",
        error("The passkey request was cancelled or timed out."),
    )
    .await;
    click(&root, ".account-create");
    wait_until("the taken name", error("That username isn't available")).await;
    assert!(api.sent("/api/auth/login/finish").is_empty());
    assert!(find(&root, ".account-name").is_none());

    // Escape dismisses the dialog.
    let escape = web_sys::KeyboardEventInit::new();
    escape.set_key("Escape");
    escape.set_bubbles(true);
    find(&root, "#account-username")
        .unwrap()
        .dispatch_event(
            &web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &escape).unwrap(),
        )
        .unwrap();
    sleep(30).await;
    assert!(find(&root, ".account-dialog").is_none());
    handle.destroy();
    root.remove();
}
