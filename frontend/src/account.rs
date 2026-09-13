//! The signed-in account: loaded from `/api/auth/me` at startup, changed
//! from the account control in the header, and shared with pages through
//! [`Account`]. A page that needs an account, like the play screen's Share
//! and Submit, asks for a sign-in and hears back whether it happened.
//!
//! Responses can't be trusted to arrive in order, and a response that sets
//! or clears the session cookie takes effect in the browser whether or not
//! the page still wants it. So requests that change the session run one at
//! a time ([`Settle`]), each followed by a `/me` check under the same lock,
//! and the displayed account is always the one the server says the cookie
//! holds.

#[cfg(all(test, target_arch = "wasm32"))]
pub(crate) mod browser_tests;

use crate::api::{self, Failure, Prepared};
use web_sys::HtmlInputElement;
use yew::prelude::*;

/// What pages see of the account.
#[derive(Clone, PartialEq)]
pub struct Account {
    /// The signed-in username; `None` when signed out or not yet known.
    pub username: Option<String>,
    /// Open the sign-in dialog.
    pub ask: Callback<SignInAsk>,
}

/// A request to sign in, from a page that needs an account.
pub struct SignInAsk {
    /// Why an account is needed, shown in the dialog.
    pub reason: &'static str,
    /// Answered once: `true` after signing in, `false` if the dialog is
    /// dismissed or another ask replaces this one.
    pub done: Callback<bool>,
}

/// What the header's account control shows and can do.
#[derive(Clone, PartialEq)]
struct Menu {
    opener: Option<web_sys::Element>,
    username: Option<String>,
    /// `Some` while the sign-in dialog is open, with the reason it was
    /// opened for, if a page asked.
    dialog: Option<Option<&'static str>>,
    busy: bool,
    /// What a session change in progress is doing.
    status: Option<&'static str>,
    /// The last sign-in failure, shown in the dialog.
    error: Option<String>,
    /// The outcome of adding a passkey or signing out.
    notice: Option<String>,
    act: Callback<Action>,
}

/// What the account control asks the provider to do.
#[derive(Clone, PartialEq)]
pub enum Action {
    Open,
    Close,
    SignIn,
    Register(String),
    AddPasskey,
    SignOut,
}

/// A request that sets or clears the session cookie, or the `/me` check
/// that follows one. One runs at a time; until it and its follow-ups have
/// settled, no other auth operation starts.
#[derive(Clone, Copy, PartialEq)]
enum Settle {
    /// A sign-in or registration finish. `current` until the dialog it was
    /// started from closes or a newer ask replaces it.
    Finish { current: bool },
    /// Signing out: on request, or undoing a sign-in cancelled while it
    /// finished.
    SignOut,
    /// Asking the server which account the cookie now holds. `answer`: a
    /// signed-in result answers the waiting ask.
    Check { answer: bool },
}

impl Settle {
    fn status(self) -> &'static str {
        match self {
            Self::Finish { .. } => "Finishing sign-in…",
            Self::SignOut => "Signing out…",
            Self::Check { .. } => "Checking your session…",
        }
    }
}

pub enum Msg {
    Ask(SignInAsk),
    Act(Action),
    /// The startup `/me`, with the operation number it was sent under.
    Me(u32, Result<Option<String>, Failure>),
    /// A ceremony prepared up to its finish, under this operation number.
    Prepared(u32, Result<Prepared, Failure>),
    Added(u32, Result<(), Failure>),
    Finished(Result<String, Failure>),
    SignedOut(Result<(), Failure>),
    Checked(Result<Option<String>, Failure>),
}

#[derive(Properties, PartialEq)]
pub struct AccountProviderProps {
    pub children: Html,
}

/// Holds the account for everything inside it.
pub struct AccountProvider {
    opener: Option<web_sys::Element>,
    username: Option<String>,
    dialog: Option<Option<&'static str>>,
    /// The page ask the open dialog will answer.
    waiting: Option<Callback<bool>>,
    /// A ceremony preparing, or a passkey being added: nothing that
    /// changes the session yet.
    preparing: bool,
    settle: Option<Settle>,
    error: Option<String>,
    notice: Option<String>,
    ask: Callback<SignInAsk>,
    act: Callback<Action>,
    /// Numbers operations that don't change the session (the startup
    /// `/me`, preparing a ceremony, adding a passkey). A response counts
    /// only while its number is the latest, so a ceremony cancelled before
    /// its finish never sends one.
    op: u32,
}

impl AccountProvider {
    fn busy(&self) -> bool {
        self.preparing || self.settle.is_some()
    }

    /// Start an operation that doesn't change the session yet.
    fn begin(&mut self) -> u32 {
        self.op += 1;
        self.preparing = true;
        self.op
    }

    /// The dialog closed or a newer ask replaced its ask: a ceremony still
    /// preparing is dropped, and one settling no longer answers anything.
    fn detach(&mut self) {
        self.op += 1;
        self.preparing = false;
        match &mut self.settle {
            Some(Settle::Finish { current }) => *current = false,
            Some(Settle::Check { answer }) => *answer = false,
            Some(Settle::SignOut) | None => {}
        }
    }

    fn answer(&mut self, signed_in: bool) {
        if let Some(done) = self.waiting.take() {
            done.emit(signed_in);
        }
    }

    /// Sign out, as the lock's next step.
    fn sign_out(&mut self, ctx: &Context<Self>) {
        self.settle = Some(Settle::SignOut);
        ctx.link()
            .send_future(async { Msg::SignedOut(api::sign_out().await) });
    }

    /// Ask the server which account the cookie holds, as the lock's last
    /// step. `answer`: a signed-in result answers the waiting ask.
    fn check(&mut self, ctx: &Context<Self>, answer: bool) {
        self.settle = Some(Settle::Check { answer });
        ctx.link()
            .send_future(async { Msg::Checked(api::me().await) });
    }
}

impl Component for AccountProvider {
    type Message = Msg;
    type Properties = AccountProviderProps;

    fn create(ctx: &Context<Self>) -> Self {
        ctx.link()
            .send_future(async { Msg::Me(0, api::me().await) });
        Self {
            opener: None,
            username: None,
            dialog: None,
            waiting: None,
            preparing: false,
            settle: None,
            error: None,
            notice: None,
            ask: ctx.link().callback(Msg::Ask),
            act: ctx.link().callback(Msg::Act),
            op: 0,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Me(op, _) | Msg::Prepared(op, _) | Msg::Added(op, _) if op != self.op => {
                return false;
            }
            Msg::Me(_, Ok(Some(username))) => {
                self.username = Some(username);
                self.dialog = None;
            }
            Msg::Me(..) => return false,
            Msg::Ask(ask) => {
                if self.dialog.is_none() {
                    self.opener = web_sys::window()
                        .and_then(|w| w.document())
                        .and_then(|d| d.active_element());
                }
                // A page asks when it has no session, or the server said it
                // has none, so the account is signed out either way.
                self.detach();
                self.username = None;
                self.answer(false);
                self.waiting = Some(ask.done);
                self.dialog = Some(Some(ask.reason));
                self.error = None;
            }
            Msg::Act(Action::Open) => {
                if self.dialog.is_none() {
                    self.opener = web_sys::window()
                        .and_then(|w| w.document())
                        .and_then(|d| d.active_element());
                }
                self.dialog.get_or_insert(None);
                self.error = None;
            }
            Msg::Act(Action::Close) => {
                self.detach();
                self.dialog = None;
                self.error = None;
                self.answer(false);
            }
            // One operation at a time, however the form is submitted.
            Msg::Act(_) if self.busy() => return false,
            Msg::Act(Action::SignIn) => {
                let op = self.begin();
                self.error = None;
                ctx.link()
                    .send_future(async move { Msg::Prepared(op, api::prepare_sign_in().await) });
            }
            Msg::Act(Action::Register(name)) => {
                let op = self.begin();
                self.error = None;
                ctx.link().send_future(async move {
                    Msg::Prepared(op, api::prepare_registration(&name).await)
                });
            }
            Msg::Act(Action::AddPasskey) => {
                let op = self.begin();
                self.notice = None;
                ctx.link()
                    .send_future(async move { Msg::Added(op, api::add_passkey().await) });
            }
            Msg::Act(Action::SignOut) => {
                self.op += 1;
                self.notice = None;
                self.sign_out(ctx);
            }
            Msg::Prepared(_, Ok(prepared)) => {
                self.preparing = false;
                self.settle = Some(Settle::Finish { current: true });
                ctx.link()
                    .send_future(async { Msg::Finished(api::finish(prepared).await) });
            }
            Msg::Prepared(_, Err(e)) => {
                self.preparing = false;
                self.error = Some(e.to_string());
            }
            Msg::Added(_, result) => {
                self.preparing = false;
                self.notice = Some(match result {
                    Ok(()) => "Passkey added. You can sign in with either.".into(),
                    Err(e) => e.to_string(),
                });
            }
            Msg::Finished(result) => {
                let current = self.settle == Some(Settle::Finish { current: true });
                match result {
                    // Cancelled while it finished, so sign that session out
                    // again before anything else can start.
                    Ok(_) if !current => self.sign_out(ctx),
                    Ok(_) => self.check(ctx, true),
                    Err(e) => {
                        if current {
                            self.error = Some(e.to_string());
                        }
                        self.check(ctx, false);
                    }
                }
            }
            Msg::SignedOut(result) => {
                if let Err(e) = result {
                    self.notice = Some(e.to_string());
                }
                self.check(ctx, false);
            }
            Msg::Checked(result) => {
                let answer = self.settle == Some(Settle::Check { answer: true });
                self.settle = None;
                match result {
                    // The cookie holds this account, so it is the one shown.
                    // Only a sign-in still wanted answers the waiting ask;
                    // a dismissed one stays dismissed.
                    Ok(Some(username)) => {
                        self.username = Some(username);
                        self.dialog = None;
                        self.error = None;
                        self.answer(answer);
                    }
                    Ok(None) => {
                        self.username = None;
                        if answer {
                            self.error = Some("The sign-in didn't take effect. Try again.".into());
                        }
                    }
                    Err(e) => {
                        self.username = None;
                        self.error = Some(format!("Couldn't check your sign-in: {e}"));
                    }
                }
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let account = Account {
            username: self.username.clone(),
            ask: self.ask.clone(),
        };
        let menu = Menu {
            opener: self.opener.clone(),
            username: self.username.clone(),
            dialog: self.dialog,
            busy: self.busy(),
            status: self.settle.map(Settle::status),
            error: self.error.clone(),
            notice: self.notice.clone(),
            act: self.act.clone(),
        };
        // Keep both sibling slots present: changing this child list would
        // remount the page and discard its frozen pending Share or Submit.
        html! {
            <ContextProvider<Account> context={account}>
                <ContextProvider<Menu> context={menu}>
                    <div class="account-page" inert={self.dialog.is_some().then_some("")}>
                        { ctx.props().children.clone() }
                    </div>
                    <div class="account-modal-slot">
                        { self.dialog.map(|_| html! { <AccountDialog /> }) }
                    </div>
                </ContextProvider<Menu>>
            </ContextProvider<Account>>
        }
    }
}

/// The header keeps only an opener. Account actions live in the same modal.
#[function_component(AccountMenu)]
pub fn account_menu() -> Html {
    let menu = use_context::<Menu>().expect("AccountMenu is inside an AccountProvider");
    let onclick = Callback::from(move |_| menu.act.emit(Action::Open));
    html! {
        <button class={classes!("account-button", if menu.username.is_some() { "account-name" } else { "account-open" })}
            {onclick} aria-haspopup="dialog">
            { menu.username.as_deref().unwrap_or("Sign in") }
        </button>
    }
}

/// Native modal semantics make the background inert, including to assistive
/// technology. The explicit Tab loop also follows controls becoming disabled
/// while a ceremony finishes. Unmount restores scroll and the opener's focus.
#[function_component(AccountDialog)]
fn account_dialog() -> Html {
    use wasm_bindgen::JsCast;
    use web_sys::{HtmlDialogElement, HtmlElement};
    let menu = use_context::<Menu>().expect("dialog inside provider");
    let dialog = use_node_ref();
    let input = use_node_ref();
    let new_account = use_state(|| false);
    let name = use_state(String::new);
    let available = use_state(|| None::<(String, Result<bool, String>)>);
    {
        let dialog = dialog.clone();
        let opener = menu.opener.clone();
        use_effect_with((), move |_| {
            let document = web_sys::window().unwrap().document().unwrap();
            let body = document.body().unwrap();
            let style = body.style();
            let overflow = style.get_property_value("overflow").unwrap_or_default();
            let priority = style.get_property_priority("overflow");
            let _ = style.set_property("overflow", "hidden");
            let element = dialog.cast::<HtmlDialogElement>().unwrap();
            let _ = element.show_modal();
            move || {
                element.close();
                if overflow.is_empty() {
                    let _ = style.remove_property("overflow");
                } else {
                    let _ = style.set_property_with_priority("overflow", &overflow, &priority);
                }
                // Effects can unmount before the sibling loses `inert`.
                wasm_bindgen_futures::spawn_local(async move {
                    gloo_timers::future::sleep(std::time::Duration::ZERO).await;
                    let target = opener
                        .filter(|e| e.is_connected())
                        .and_then(|e| e.dyn_into::<HtmlElement>().ok())
                        .or_else(|| {
                            document
                                .query_selector(".account-name, .account-open")
                                .ok()
                                .flatten()
                                .and_then(|e| e.dyn_into::<HtmlElement>().ok())
                        });
                    if document
                        .query_selector("dialog[open]")
                        .ok()
                        .flatten()
                        .is_none()
                    {
                        if let Some(target) = target.filter(|t| t.is_connected()) {
                            let _ = target.focus();
                        }
                    }
                });
            }
        });
    }
    {
        let input = input.clone();
        use_effect_with(*new_account, move |new| {
            if *new {
                if let Some(input) = input.cast::<HtmlInputElement>() {
                    let _ = input.focus();
                }
            }
        });
    }
    let typed = name.trim().to_ascii_lowercase();
    let valid_name = (3..=24).contains(&typed.len())
        && typed.as_bytes()[0].is_ascii_alphanumeric()
        && typed
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
    {
        let available = available.clone();
        use_effect_with(
            (typed.clone(), valid_name, *new_account),
            move |(name, valid, new)| {
                let alive = std::rc::Rc::new(std::cell::Cell::new(true));
                let still_here = alive.clone();
                if *valid && *new {
                    let name = name.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        gloo_timers::future::sleep(std::time::Duration::from_millis(250)).await;
                        if !still_here.get() {
                            return;
                        }
                        let result = api::username_available(&name)
                            .await
                            .map_err(|e| e.to_string());
                        if still_here.get() {
                            available.set(Some((name, result)));
                        }
                    });
                }
                move || alive.set(false)
            },
        );
    }
    let act = |action: Action| {
        let act = menu.act.clone();
        Callback::from(move |_: MouseEvent| act.emit(action.clone()))
    };
    let oninput = {
        let name = name.clone();
        Callback::from(move |e: InputEvent| {
            name.set(e.target_unchecked_into::<HtmlInputElement>().value())
        })
    };
    let onkeydown = {
        let (dialog, act) = (dialog.clone(), menu.act.clone());
        Callback::from(move |e: KeyboardEvent| {
            if e.key() == "Escape" {
                e.prevent_default();
                e.stop_propagation();
                act.emit(Action::Close);
            }
            if e.key() == "Tab" {
                let Some(dialog) = dialog.cast::<HtmlDialogElement>() else {
                    return;
                };
                let nodes = dialog
                    .query_selector_all(
                        "button:not([disabled]), input:not([disabled]), a[href], [tabindex='0']",
                    )
                    .unwrap();
                let controls: Vec<HtmlElement> = (0..nodes.length())
                    .filter_map(|i| nodes.item(i)?.dyn_into().ok())
                    .collect();
                if controls.is_empty() {
                    e.prevent_default();
                    return;
                }
                let active = web_sys::window()
                    .unwrap()
                    .document()
                    .unwrap()
                    .active_element();
                let at = controls
                    .iter()
                    .position(|c| Some(c.as_ref()) == active.as_ref());
                let index = if e.shift_key() {
                    at.filter(|i| *i > 0).map_or(controls.len() - 1, |i| i - 1)
                } else {
                    at.map_or(0, |i| (i + 1) % controls.len())
                };
                e.prevent_default();
                let _ = controls[index].focus();
            }
        })
    };
    let oncancel = {
        let act = menu.act.clone();
        Callback::from(move |e: Event| {
            e.prevent_default();
            act.emit(Action::Close);
        })
    };
    let onclick = {
        let (act, dialog) = (menu.act.clone(), dialog.clone());
        Callback::from(move |e: MouseEvent| {
            if e.target() == dialog.cast::<HtmlDialogElement>().map(Into::into) {
                act.emit(Action::Close);
            }
        })
    };
    let onsubmit = {
        let (act, typed, busy, new) = (menu.act.clone(), typed.clone(), menu.busy, *new_account);
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            if !busy {
                if new && valid_name {
                    act.emit(Action::Register(typed.clone()));
                } else if !new {
                    act.emit(Action::SignIn);
                }
            }
        })
    };
    let next = {
        let new = new_account.clone();
        Callback::from(move |_| new.set(true))
    };
    let back = {
        let new = new_account.clone();
        Callback::from(move |_| new.set(false))
    };
    let hint = if typed.is_empty() {
        "Choose a unique username.".to_owned()
    } else if !valid_name {
        "Use 3–24 letters, digits, - or _, starting with a letter or digit.".into()
    } else {
        match available
            .as_ref()
            .filter(|(n, _)| *n == typed)
            .map(|(_, r)| r)
        {
            Some(Ok(true)) => "This username is available.".into(),
            Some(Ok(false)) => "That username is unavailable.".into(),
            Some(Err(_)) => {
                "Couldn't check availability. Creating the passkey will check again.".into()
            }
            None => "Checking availability…".into(),
        }
    };
    html! {
        <dialog ref={dialog} class="account-modal" aria-modal="true" aria-labelledby="account-title"
            {onkeydown} {oncancel} {onclick}>
            <form class="account-dialog" tabindex="-1" {onsubmit}>
                <h2 id="account-title">{ if menu.username.is_some() { "Your account" } else { "Sign in" } }</h2>
                { menu.dialog.flatten().map(|reason| html! { <p class="account-reason">{ reason }</p> }) }
                { if let Some(username) = &menu.username { html! {
                    <div class="account-actions">
                        <p>{ username }</p>
                        <button type="button" class="account-button account-add" disabled={menu.busy} onclick={act(Action::AddPasskey)}>{ "Add a passkey" }</button>
                        <button type="button" class="account-button account-sign-out" disabled={menu.busy} onclick={act(Action::SignOut)}>{ "Sign out" }</button>
                    </div>
                }} else if *new_account { html! {
                    <>
                        <label for="account-username">{ "New username" }</label>
                        <input id="account-username" ref={input} type="text" autocomplete="username webauthn"
                            autocapitalize="none" spellcheck="false" maxlength="24" disabled={menu.busy}
                            value={(*name).clone()} {oninput} aria-describedby="account-availability" />
                        <p id="account-availability" class="account-availability" role="status">{ hint }</p>
                        <div class="account-actions">
                            <button type="submit" class="account-button account-primary account-create" disabled={menu.busy || !valid_name}>{ "Create passkey" }</button>
                            <button type="button" class="account-button account-back" disabled={menu.busy} onclick={back}>{ "Back" }</button>
                        </div>
                    </>
                }} else { html! {
                    <div class="account-choices">
                        <button type="submit" class="account-button account-primary account-sign-in" disabled={menu.busy} autofocus=true>
                            <strong>{ "Existing" }</strong><span>{ "Choose your saved passkey" }</span>
                        </button>
                        <button type="button" class="account-button account-new" disabled={menu.busy} onclick={next}>
                            <strong>{ "New" }</strong><span>{ "Choose a username and save a passkey" }</span>
                        </button>
                    </div>
                }} }
                { menu.status.map(|s| html! { <p class="account-status" role="status">{ s }</p> }) }
                { menu.error.as_ref().map(|s| html! { <p class="account-error" role="alert">{ s }</p> }) }
                { menu.notice.as_ref().map(|s| html! { <p class="account-notice" role="status">{ s }</p> }) }
                <p class="account-help">{ "Use a passkey saved on this device or your phone. There is no account recovery: keep the passkey in a synced password manager or add a second one, because an account whose passkeys are all lost can't be recovered." }</p>
                <button type="button" class="account-button account-cancel" onclick={act(Action::Close)}>{ if menu.username.is_some() { "Close" } else { "Cancel" } }</button>
            </form>
        </dialog>
    }
}
