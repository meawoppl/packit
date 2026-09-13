//! The signed-in account: loaded from `/api/auth/me` at startup, changed
//! from the account control in the header, and shared with pages through
//! [`Account`]. A page that needs an account, like the play screen's Share
//! and Submit, asks for a sign-in and hears back whether it happened.

#[cfg(all(test, target_arch = "wasm32"))]
pub(crate) mod browser_tests;

use crate::api::{self, Failure};
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
    username: Option<String>,
    /// `Some` while the sign-in dialog is open, with the reason it was
    /// opened for, if a page asked.
    dialog: Option<Option<&'static str>>,
    busy: bool,
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
    SignIn(String),
    Register(String),
    AddPasskey,
    SignOut,
}

pub enum Msg {
    Me(Result<Option<String>, Failure>),
    Ask(SignInAsk),
    Act(Action),
    SignedIn(Result<String, Failure>),
    Added(Result<(), Failure>),
    SignedOut(Result<(), Failure>),
}

#[derive(Properties, PartialEq)]
pub struct AccountProviderProps {
    pub children: Html,
}

/// Holds the account for everything inside it.
pub struct AccountProvider {
    username: Option<String>,
    dialog: Option<Option<&'static str>>,
    /// The page ask the open dialog will answer.
    waiting: Option<Callback<bool>>,
    busy: bool,
    error: Option<String>,
    notice: Option<String>,
    ask: Callback<SignInAsk>,
    act: Callback<Action>,
}

impl AccountProvider {
    fn answer(&mut self, signed_in: bool) {
        if let Some(done) = self.waiting.take() {
            done.emit(signed_in);
        }
    }

    fn signed_in(&mut self, username: String) {
        self.username = Some(username);
        self.dialog = None;
        self.error = None;
        self.notice = None;
        self.answer(true);
    }
}

impl Component for AccountProvider {
    type Message = Msg;
    type Properties = AccountProviderProps;

    fn create(ctx: &Context<Self>) -> Self {
        ctx.link().send_future(async { Msg::Me(api::me().await) });
        Self {
            username: None,
            dialog: None,
            waiting: None,
            busy: false,
            error: None,
            notice: None,
            ask: ctx.link().callback(Msg::Ask),
            act: ctx.link().callback(Msg::Act),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Me(Ok(Some(username))) => self.signed_in(username),
            Msg::Me(_) => return false,
            Msg::Ask(ask) => {
                // A page asks when it has no session, or the server said it
                // has none, so the account is signed out either way.
                self.username = None;
                self.answer(false);
                self.waiting = Some(ask.done);
                self.dialog = Some(Some(ask.reason));
                self.error = None;
            }
            Msg::Act(Action::Open) => {
                self.dialog.get_or_insert(None);
                self.error = None;
            }
            Msg::Act(Action::Close) => {
                self.dialog = None;
                self.error = None;
                self.answer(false);
            }
            Msg::Act(Action::SignIn(name)) => {
                self.busy = true;
                self.error = None;
                ctx.link()
                    .send_future(async move { Msg::SignedIn(api::sign_in(&name).await) });
            }
            Msg::Act(Action::Register(name)) => {
                self.busy = true;
                self.error = None;
                ctx.link()
                    .send_future(async move { Msg::SignedIn(api::register(&name).await) });
            }
            Msg::Act(Action::AddPasskey) => {
                self.busy = true;
                self.notice = None;
                ctx.link()
                    .send_future(async { Msg::Added(api::add_passkey().await) });
            }
            Msg::Act(Action::SignOut) => {
                self.busy = true;
                self.notice = None;
                ctx.link()
                    .send_future(async { Msg::SignedOut(api::sign_out().await) });
            }
            Msg::SignedIn(result) => {
                self.busy = false;
                match result {
                    Ok(username) => self.signed_in(username),
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
            Msg::Added(result) => {
                self.busy = false;
                self.notice = Some(match result {
                    Ok(()) => "Passkey added. You can sign in with either.".into(),
                    Err(e) => e.to_string(),
                });
            }
            Msg::SignedOut(result) => {
                self.busy = false;
                match result {
                    Ok(()) => self.username = None,
                    Err(e) => self.notice = Some(e.to_string()),
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
            username: self.username.clone(),
            dialog: self.dialog,
            busy: self.busy,
            error: self.error.clone(),
            notice: self.notice.clone(),
            act: self.act.clone(),
        };
        html! {
            <ContextProvider<Account> context={account}>
                <ContextProvider<Menu> context={menu}>
                    { ctx.props().children.clone() }
                </ContextProvider<Menu>>
            </ContextProvider<Account>>
        }
    }
}

/// The header's account control: signed out, a Sign in button that opens
/// the sign-in dialog; signed in, the username, Add a passkey and Sign out.
#[function_component(AccountMenu)]
pub fn account_menu() -> Html {
    let menu = use_context::<Menu>().expect("AccountMenu is inside an AccountProvider");
    let name = use_state(String::new);
    let input = use_node_ref();
    {
        // Whoever opened the dialog, the username field takes focus.
        let input = input.clone();
        use_effect_with(menu.dialog.is_some(), move |open| {
            if *open {
                if let Some(input) = input.cast::<HtmlInputElement>() {
                    let _ = input.focus();
                }
            }
        });
    }
    let act = |action: Action| {
        let act = menu.act.clone();
        Callback::from(move |_: MouseEvent| act.emit(action.clone()))
    };
    if let Some(username) = &menu.username {
        return html! {
            <div class="account">
                <span class="account-name">{ username }</span>
                <button class="account-button account-add" disabled={menu.busy}
                    onclick={act(Action::AddPasskey)}>{ "Add a passkey" }</button>
                <button class="account-button account-sign-out" disabled={menu.busy}
                    onclick={act(Action::SignOut)}>{ "Sign out" }</button>
                { menu.notice.as_ref().map(|notice| html! {
                    <p class="account-notice" role="status">{ notice }</p>
                }) }
            </div>
        };
    }
    let typed = name.trim().to_string();
    let oninput = {
        let name = name.clone();
        Callback::from(move |e: InputEvent| {
            name.set(e.target_unchecked_into::<HtmlInputElement>().value())
        })
    };
    let onkeydown = {
        let act = menu.act.clone();
        Callback::from(move |e: KeyboardEvent| {
            if e.key() == "Escape" {
                act.emit(Action::Close);
            }
        })
    };
    let onsubmit = {
        let (act, typed) = (menu.act.clone(), typed.clone());
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            if !typed.is_empty() {
                act.emit(Action::SignIn(typed.clone()));
            }
        })
    };
    let no_name = typed.is_empty();
    html! {
        <div class="account">
            { if menu.dialog.is_none() {
                html! {
                    <button class="account-button account-open" onclick={act(Action::Open)}>
                        { "Sign in" }
                    </button>
                }
            } else { Html::default() } }
            { menu.dialog.map(|reason| html! {
                <form class="account-dialog" role="dialog" aria-label="Sign in or create an account"
                    {onsubmit} {onkeydown}>
                    { reason.map(|reason| html! { <p class="account-reason">{ reason }</p> }) }
                    <label for="account-username">{ "Username" }</label>
                    <input id="account-username" ref={input.clone()} type="text"
                        autocomplete="username webauthn" autocapitalize="none" spellcheck="false"
                        maxlength="24" value={(*name).clone()} {oninput} />
                    <div class="account-actions">
                        <button type="submit" class="account-button account-primary account-sign-in"
                            disabled={menu.busy || no_name}>{ "Sign in" }</button>
                        <button type="button" class="account-button account-create"
                            disabled={menu.busy || no_name}
                            onclick={act(Action::Register(typed.clone()))}>{ "Create account" }</button>
                        <button type="button" class="account-button account-cancel"
                            onclick={act(Action::Close)}>{ "Cancel" }</button>
                    </div>
                    { menu.error.as_ref().map(|error| html! {
                        <p class="account-error" role="alert">{ error }</p>
                    }) }
                    <p class="account-help">
                        { "Sign in with a passkey saved on this device or your phone. A new account needs a username of 3 to 24 letters, digits, - or _." }
                    </p>
                </form>
            }) }
        </div>
    }
}
