//! JSON export and import of arrangements.

use shared::{board, Arrangement};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement, Url};

/// Largest import accepted, in bytes.
pub const MAX_IMPORT_BYTES: f64 = 1_000_000.0;

/// Offer `value` as a pretty-printed JSON download named `name`.
pub fn download_json(name: &str, value: &serde_json::Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    let options = BlobPropertyBag::new();
    options.set_type("application/json");
    let blob = Blob::new_with_str_sequence_and_options(
        &js_sys::Array::of1(&JsValue::from_str(&text)),
        &options,
    )
    .map_err(|_| "could not create download".to_string())?;
    let url = Url::create_object_url_with_blob(&blob)
        .map_err(|_| "could not create download".to_string())?;
    let anchor = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.create_element("a").ok())
        .and_then(|a| a.dyn_into::<HtmlAnchorElement>().ok())
        .ok_or("could not create download")?;
    anchor.set_href(&url);
    anchor.set_download(name);
    anchor.click();
    wasm_bindgen_futures::spawn_local(async move {
        gloo_timers::future::sleep(std::time::Duration::from_secs(1)).await;
        let _ = Url::revoke_object_url(&url);
    });
    Ok(())
}

/// Parse an exported file (a bare `Arrangement` or an object with an
/// `arrangement` field) and check it fits the current `n`.
pub fn parse_arrangement(text: &str, n: u32) -> Result<Arrangement, String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let inner = value.get("arrangement").unwrap_or(&value).clone();
    let arr: Arrangement = serde_json::from_value(inner)
        .map_err(|_| format!("Expected {n} squares with finite coordinates"))?;
    board::check_arrangement(&arr, n)?;
    Ok(arr)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO: &str = r#"{"n":2,"side":2.0,"squares":[
        {"cx":0.5,"cy":0.5,"theta":0.0},{"cx":1.5,"cy":0.5,"theta":0.0}]}"#;

    #[test]
    fn accepts_bare_and_wrapped_arrangements() {
        assert_eq!(parse_arrangement(TWO, 2).unwrap().squares.len(), 2);
        let wrapped = format!(r#"{{"valid":true,"arrangement":{TWO}}}"#);
        assert_eq!(parse_arrangement(&wrapped, 2).unwrap().side, 2.0);
    }

    #[test]
    fn rejects_wrong_count_bad_side_and_garbage() {
        assert!(parse_arrangement(TWO, 3).is_err());
        assert!(parse_arrangement(&TWO.replace("2.0,", "0.5,"), 2).is_err());
        assert!(parse_arrangement(&TWO.replace("\"cx\":1.5", "\"cx\":1e9"), 2).is_err());
        assert!(parse_arrangement("not json", 2).is_err());
    }
}
