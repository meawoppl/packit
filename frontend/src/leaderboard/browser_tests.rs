//! In-browser tests of the leaderboards against a stubbed API: names from
//! accounts and legacy names must read differently, and every score links
//! to the board it was submitted as.

use super::*;
use crate::account::browser_tests::{reply, sleep, wait_until, Api};
use serde_json::{json, Value};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::Element;

wasm_bindgen_test_configure!(run_in_browser);

const RECORDED: &str = "0123456789abcdef01234567";
const LEGACY: &str = "89abcdef0123456789abcdef";

/// A score as the API returns it: by an account or under a legacy name, on
/// `board`, with its glue recorded or not.
fn score(n: u32, rank: u32, account: bool, board: &str, glue_recorded: bool) -> Value {
    json!({
        "id": format!("00000000-0000-0000-0000-{:012}", n * 10 + rank),
        "player": "ada",
        "n": n,
        "side": n as f64 + 0.5,
        "submitted_at": "2026-09-13T00:00:00",
        "rank": rank,
        "account": account,
        "board": board,
        "glue_recorded": glue_recorded,
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

#[function_component(Ranked)]
fn ranked() -> Html {
    html! { <BrowserRouter><LeaderboardN n={2} /></BrowserRouter> }
}

/// The page of score 22: n = 2, rank 2.
#[function_component(Page)]
fn page() -> Html {
    html! { <BrowserRouter><ScorePage id={Uuid::from_u128(0x22)} /></BrowserRouter> }
}

/// Mount `C` and wait until something matches `ready`.
async fn mount<C: BaseComponent<Properties = ()>>(ready: &str) -> (yew::AppHandle<C>, Element) {
    let document = web_sys::window().unwrap().document().unwrap();
    let root = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&root).unwrap();
    let handle = yew::Renderer::<C>::with_root(root.clone()).render();
    wait_until(ready, || root.query_selector(ready).unwrap().is_some()).await;
    (handle, root)
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
            vec![reply(
                200,
                json!([
                    score(2, 1, true, RECORDED, true),
                    score(2, 1, false, LEGACY, false)
                ]),
            )],
        ),
    ]);
    let (handle, root) = mount::<Boards>(".player").await;
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

#[wasm_bindgen_test]
async fn every_ranked_score_links_to_its_board() {
    let _api = Api::install(&[
        (
            "/api/scores",
            vec![reply(
                200,
                json!([
                    score(2, 1, true, RECORDED, true),
                    score(2, 2, false, LEGACY, false)
                ]),
            )],
        ),
        ("/api/records", vec![reply(200, json!([record(2)]))]),
    ]);
    let (handle, root) = mount::<Ranked>(".score-board").await;
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
        "side": 2.5,
        "squares": [
            { "cx": 0.5, "cy": 0.5, "theta": 0.0 },
            { "cx": 1.5, "cy": 0.5, "theta": 0.0 },
        ],
    });
    let _api = Api::install(&[
        (
            "/api/scores/00000000-0000-0000-0000-000000000022",
            vec![reply(
                200,
                json!({ "entry": score(2, 2, false, LEGACY, false), "arrangement": arrangement }),
            )],
        ),
        ("/api/records", vec![reply(200, json!([record(2)]))]),
    ]);
    let (handle, root) = mount::<Page>(".score-board").await;
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

#[function_component(Personal)]
fn personal() -> Html {
    html! {<BrowserRouter><Leaderboard player={Some("ada".to_string())}/></BrowserRouter>}
}

#[wasm_bindgen_test]
async fn icons_filter_personal_records_and_account_names_link_to_profiles() {
    let api = Api::install(&[
        (
            "/api/scores",
            vec![reply(200, json!([score(2, 3, true, RECORDED, true)]))],
        ),
        ("/api/records", vec![reply(200, json!([]))]),
    ]);
    let (handle, root) = mount::<Personal>(".score-board").await;
    assert!(root.text_content().unwrap().contains("ada’s records"));
    assert_eq!(
        root.query_selector("a.player")
            .unwrap()
            .unwrap()
            .get_attribute("href")
            .as_deref(),
        Some("/players/ada")
    );
    let click = |label: &str| {
        root.query_selector(&format!("button[aria-label='{label}']"))
            .unwrap()
            .unwrap()
            .unchecked_into::<web_sys::HtmlElement>()
            .click()
    };
    let count = api.sent("/api/scores").len();
    click("Pieces: triangle");
    wait_until("piece filter fetch", || {
        api.sent("/api/scores").len() > count
    })
    .await;
    assert_eq!(
        root.query_selector("button[aria-label='Pieces: triangle']")
            .unwrap()
            .unwrap()
            .get_attribute("aria-pressed")
            .as_deref(),
        Some("true")
    );
    let count = api.sent("/api/scores").len();
    click("Container: hexagon");
    wait_until("container filter fetch", || {
        api.sent("/api/scores").len() > count
    })
    .await;
    let count = api.sent("/api/scores").len();
    let select = root
        .query_selector("select")
        .unwrap()
        .unwrap()
        .unchecked_into::<web_sys::HtmlSelectElement>();
    select.set_value("7");
    select
        .dispatch_event(&web_sys::Event::new("change").unwrap())
        .unwrap();
    wait_until("count filter fetch", || {
        api.sent("/api/scores").len() > count
    })
    .await;
    assert!(root
        .text_content()
        .unwrap()
        .contains("ranks are among all players"));
    assert_eq!(
        select.value(),
        "7",
        "count selector keeps the chosen value after rendering"
    );
    handle.destroy();
    root.remove();
}
