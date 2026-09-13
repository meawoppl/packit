//! In-browser tests of the leaderboard pages against a stubbed API: every
//! score links to the board it was submitted as.

use super::*;
use crate::account::browser_tests::{reply, wait_until, Api};
use serde_json::{json, Value};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::Element;

wasm_bindgen_test_configure!(run_in_browser);

const RECORDED: &str = "0123456789abcdef01234567";
const LEGACY: &str = "89abcdef0123456789abcdef";

/// A score as the API returns it, on board `board`.
fn score(id: u128, rank: u32, board: &str, glue_recorded: bool) -> Value {
    json!({
        "id": Uuid::from_u128(id),
        "player": format!("player-{id}"),
        "n": 2,
        "side": 2.0 + rank as f64 / 10.0,
        "submitted_at": "2026-09-13T00:00:00",
        "rank": rank,
        "board": board,
        "glue_recorded": glue_recorded,
    })
}

#[function_component(Ranked)]
fn ranked() -> Html {
    html! { <BrowserRouter><LeaderboardN n={2} /></BrowserRouter> }
}

#[function_component(Page)]
fn page() -> Html {
    html! { <BrowserRouter><ScorePage id={Uuid::from_u128(2)} /></BrowserRouter> }
}

async fn mount<C: BaseComponent<Properties = ()>>() -> (yew::AppHandle<C>, Element) {
    let document = web_sys::window().unwrap().document().unwrap();
    let root = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&root).unwrap();
    let handle = yew::Renderer::<C>::with_root(root.clone()).render();
    wait_until("a board link", || {
        root.query_selector(".score-board").unwrap().is_some()
    })
    .await;
    (handle, root)
}

/// Each board link's target, and the text around it.
fn board_links(root: &Element) -> Vec<(String, String)> {
    let links = root.query_selector_all(".score-board").unwrap();
    (0..links.length())
        .filter_map(|i| links.item(i))
        .map(|node| {
            let link: Element = node.unchecked_into();
            let around = link.parent_element().unwrap().text_content().unwrap();
            (link.get_attribute("href").unwrap(), around)
        })
        .collect()
}

#[wasm_bindgen_test]
async fn every_ranked_score_links_to_its_board() {
    let _api = Api::install(&[
        (
            "/api/scores",
            vec![reply(
                200,
                json!([score(1, 1, RECORDED, true), score(2, 2, LEGACY, false)]),
            )],
        ),
        ("/api/records", vec![reply(200, json!([]))]),
    ]);
    let (handle, root) = mount::<Ranked>().await;
    assert_eq!(
        board_links(&root),
        [
            (format!("/s/{RECORDED}"), "Open".to_string()),
            (
                format!("/s/{LEGACY}"),
                "Open · glue not recorded".to_string()
            ),
        ]
    );
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn a_score_page_opens_its_board() {
    let arrangement = json!({
        "n": 2,
        "side": 2.2,
        "squares": [
            { "cx": 0.5, "cy": 0.5, "theta": 0.0 },
            { "cx": 1.5, "cy": 0.5, "theta": 0.0 },
        ],
    });
    let _api = Api::install(&[
        (
            "/api/scores/00000000-0000-0000-0000-000000000002",
            vec![reply(
                200,
                json!({ "entry": score(2, 2, LEGACY, false), "arrangement": arrangement }),
            )],
        ),
        ("/api/records", vec![reply(200, json!([]))]),
    ]);
    let (handle, root) = mount::<Page>().await;
    assert_eq!(
        board_links(&root),
        [(
            format!("/s/{LEGACY}"),
            "Open this board · glue not recorded".to_string()
        )]
    );
    handle.destroy();
    root.remove();
}
