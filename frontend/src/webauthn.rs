//! Passkey ceremonies through the browser's WebAuthn API, from Rust.
//!
//! The server sends webauthn-rs options as JSON with binary fields in
//! base64url; `navigator.credentials` wants those fields as `ArrayBuffer`s
//! and returns a credential whose binary fields go back as base64url. Both
//! directions read and write plain JS objects with `Reflect`, so anything
//! shaped like a credential works, including the fakes browser tests
//! install.

use crate::api::Failure;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use js_sys::{Array, ArrayBuffer, Promise, Reflect, Uint8Array, JSON};
use serde_json::{json, Value};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::CredentialsContainer;

/// Create a passkey from registration options (`{ publicKey: … }`), and
/// return the credential as the finish endpoint's JSON.
pub async fn create(options: &Value) -> Result<Value, Failure> {
    let options = to_js(options)?;
    let public_key = field(&options, "publicKey")?;
    decode_field(&public_key, "challenge")?;
    decode_field(&field(&public_key, "user")?, "id")?;
    decode_ids(&public_key, "excludeCredentials")?;
    let credential = ceremony(true, |c| c.create_with_options(options.unchecked_ref())).await?;
    let response = field(&credential, "response")?;
    Ok(json!({
        "id": text(&credential, "id")?,
        "rawId": encode_field(&credential, "rawId")?,
        "type": text(&credential, "type")?,
        "response": {
            "attestationObject": encode_field(&response, "attestationObject")?,
            "clientDataJSON": encode_field(&response, "clientDataJSON")?,
        },
    }))
}

/// Sign in with a passkey from authentication options, and return the
/// assertion as the finish endpoint's JSON.
pub async fn get(options: &Value) -> Result<Value, Failure> {
    let options = to_js(options)?;
    let public_key = field(&options, "publicKey")?;
    decode_field(&public_key, "challenge")?;
    decode_ids(&public_key, "allowCredentials")?;
    let credential = ceremony(false, |c| c.get_with_options(options.unchecked_ref())).await?;
    let response = field(&credential, "response")?;
    let user_handle = field(&response, "userHandle")?;
    let user_handle = if user_handle.is_null() || user_handle.is_undefined() {
        Value::Null
    } else {
        Value::String(encode(&user_handle).ok_or_else(|| unreadable("userHandle"))?)
    };
    Ok(json!({
        "id": text(&credential, "id")?,
        "rawId": encode_field(&credential, "rawId")?,
        "type": text(&credential, "type")?,
        "response": {
            "authenticatorData": encode_field(&response, "authenticatorData")?,
            "clientDataJSON": encode_field(&response, "clientDataJSON")?,
            "signature": encode_field(&response, "signature")?,
            "userHandle": user_handle,
        },
    }))
}

/// Run one `navigator.credentials` call and wait for its credential.
async fn ceremony(
    creating: bool,
    call: impl FnOnce(&CredentialsContainer) -> Result<Promise, JsValue>,
) -> Result<JsValue, Failure> {
    let unsupported = || Failure::Other("This browser doesn't support passkeys.".into());
    let window = web_sys::window().ok_or_else(unsupported)?;
    let container = Reflect::get(&window.navigator(), &"credentials".into())
        .ok()
        .filter(|c| c.is_object())
        .ok_or_else(unsupported)?;
    let promise = call(container.unchecked_ref()).map_err(|e| refused(&e, creating))?;
    let credential = JsFuture::from(promise)
        .await
        .map_err(|e| refused(&e, creating))?;
    if credential.is_object() {
        Ok(credential)
    } else {
        Err(Failure::Other("No passkey was chosen.".into()))
    }
}

/// A readable message for a rejected WebAuthn call, by its DOMException name.
fn refused(error: &JsValue, creating: bool) -> Failure {
    let name = Reflect::get(error, &"name".into())
        .ok()
        .and_then(|n| n.as_string())
        .unwrap_or_default();
    Failure::Other(match name.as_str() {
        "NotAllowedError" | "AbortError" => {
            "The passkey request was cancelled or timed out.".into()
        }
        "InvalidStateError" if creating => {
            "This device already has a passkey for this account.".into()
        }
        "SecurityError" => "Passkeys can't be used on this address.".into(),
        "NotSupportedError" => "This device can't make a passkey for this site.".into(),
        _ => {
            let message = Reflect::get(error, &"message".into())
                .ok()
                .and_then(|m| m.as_string())
                .unwrap_or(name);
            format!("The passkey request failed: {message}")
        }
    })
}

fn to_js(options: &Value) -> Result<JsValue, Failure> {
    JSON::parse(&options.to_string()).map_err(|_| unreadable("options"))
}

fn unreadable(what: &str) -> Failure {
    Failure::Other(format!("Unexpected passkey data ({what})."))
}

fn field(object: &JsValue, key: &str) -> Result<JsValue, Failure> {
    Reflect::get(object, &key.into()).map_err(|_| unreadable(key))
}

fn text(object: &JsValue, key: &str) -> Result<String, Failure> {
    field(object, key)?
        .as_string()
        .ok_or_else(|| unreadable(key))
}

/// Replace the base64url string at `object[key]` with its bytes.
fn decode_field(object: &JsValue, key: &str) -> Result<(), Failure> {
    let bytes = URL_SAFE_NO_PAD
        .decode(text(object, key)?.trim_end_matches('='))
        .map_err(|_| unreadable(key))?;
    let buffer = Uint8Array::from(bytes.as_slice()).buffer();
    Reflect::set(object, &key.into(), &buffer).map_err(|_| unreadable(key))?;
    Ok(())
}

/// Decode the `id` of every credential descriptor in `object[key]`, if any.
fn decode_ids(object: &JsValue, key: &str) -> Result<(), Failure> {
    let list = field(object, key)?;
    if list.is_undefined() || list.is_null() {
        return Ok(());
    }
    if !Array::is_array(&list) {
        return Err(unreadable(key));
    }
    Array::from(&list)
        .iter()
        .try_for_each(|descriptor| decode_field(&descriptor, "id"))
}

/// An `ArrayBuffer` or typed array as base64url.
fn encode(bytes: &JsValue) -> Option<String> {
    if !bytes.is_instance_of::<ArrayBuffer>() && !ArrayBuffer::is_view(bytes) {
        return None;
    }
    Some(URL_SAFE_NO_PAD.encode(Uint8Array::new(bytes).to_vec()))
}

fn encode_field(object: &JsValue, key: &str) -> Result<String, Failure> {
    encode(&field(object, key)?).ok_or_else(|| unreadable(key))
}
