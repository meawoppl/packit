//! In-browser tests of the leaderboards against a stubbed API: names from
//! accounts and legacy names must read differently.

use super::*;
use crate::account::browser_tests::{reply, sleep, wait_until, Api};
use serde_json::{json, Value};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::Element;

wasm_bindgen_test_configure!(run_in_browser);

fn score(n: u32, rank: u32, account: bool) -> Value {
    json!({
        "id": format!("00000000-0000-0000-0000-{:012}", n * 10 + rank),
        "player": "ada",
        "n": n,
        "side": n as f64 + 0.5,
        "submitted_at": "2026-09-13T00:00:00",
        "rank": rank,
        "account": account,
    })
}

fn record(n: u32) -> Value {
    json!({
        "n": n,
        "side": n as f64,
        "side_expr": null,
        "proven_optimal": true,
        "source": "grid",
    })
}

#[function_component(Boards)]
fn boards() -> Html {
    html! { <BrowserRouter><Leaderboard /><LeaderboardN n={2} /></BrowserRouter> }
}

/// Each player cell's text, and whether it's marked as a legacy name.
fn players(root: &Element, table: &str) -> Vec<(String, bool, Option<String>)> {
    let cells = root
        .query_selector_all(&format!("{table} .player"))
        .unwrap();
    (0..cells.length())
        .filter_map(|i| cells.item(i))
        .map(|node| {
            let cell: Element = node.unchecked_into();
            let tag = cell
                .query_selector(".legacy-badge")
                .unwrap()
                .map(|b| b.text_content().unwrap_or_default());
            (
                cell.text_content().unwrap_or_default(),
                cell.get_attribute("class")
                    .is_some_and(|c| c.split_whitespace().any(|c| c == "legacy")),
                tag.and(cell.get_attribute("title")),
            )
        })
        .collect()
}

#[wasm_bindgen_test]
async fn legacy_names_read_apart_from_account_names() {
    let _api = Api::install(&[
        (
            "/api/records",
            vec![reply(200, json!([record(1), record(2)]))],
        ),
        // Both boards get the same reply: the overview picks one score per
        // n, and the n = 2 board lists both.
        (
            "/api/scores",
            vec![reply(200, json!([score(1, 1, true), score(2, 1, false)]))],
        ),
    ]);
    let document = web_sys::window().unwrap().document().unwrap();
    let root = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&root).unwrap();
    let handle = yew::Renderer::<Boards>::with_root(root.clone()).render();
    wait_until("both boards", || {
        root.query_selector_all(".player").unwrap().length() == 4
    })
    .await;
    sleep(30).await;
    let legacy = || {
        (
            "adalegacy".to_string(),
            true,
            Some("Submitted before accounts, under a name anyone could type".to_string()),
        )
    };
    let account = ("ada".to_string(), false, None);
    let cells = players(&root, "table");
    assert_eq!(
        cells,
        [account.clone(), legacy(), account, legacy()],
        "overview rows, then the n = 2 board"
    );
    handle.destroy();
    root.remove();
}
