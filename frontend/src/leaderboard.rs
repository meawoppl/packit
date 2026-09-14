#[cfg(all(test, target_arch = "wasm32"))]
mod browser_tests;

use crate::api;
use crate::benchmark::Benchmark;
use crate::Route;
use shared::{Arrangement, KnownRecord, ScoreDetail, ScoreEntry};
use std::cell::Cell;
use std::rc::Rc;
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::*;

/// Fetch once per `deps` change and hold `None` until loaded. A response that
/// arrives after `deps` changed (or the component unmounted) is dropped.
#[hook]
fn use_fetch<T, D, F, Fut>(deps: D, fetch: F) -> UseStateHandle<Option<Result<T, String>>>
where
    T: 'static,
    D: PartialEq + Clone + 'static,
    F: Fn(D) -> Fut + 'static,
    Fut: std::future::Future<Output = Result<T, String>> + 'static,
{
    let state = use_state(|| None);
    {
        let state = state.clone();
        use_effect_with(deps, move |deps| {
            state.set(None);
            let live = Rc::new(Cell::new(true));
            let fut = fetch(deps.clone());
            {
                let live = live.clone();
                spawn_local(async move {
                    let result = fut.await;
                    if live.get() {
                        state.set(Some(result));
                    }
                });
            }
            move || live.set(false)
        });
    }
    state
}

fn fmt_side(side: f64) -> String {
    format!("{side:.6}")
}

/// How far a side length is above the best known one, as a percentage.
fn gap_pct(side: f64, known: f64) -> String {
    format!("{:+.3}%", (side / known - 1.0) * 100.0)
}

/// A score's name: an account's username, or, for a score from before
/// accounts, the name it was typed with, muted and tagged so it isn't taken
/// for an account.
fn player_name(e: &ScoreEntry) -> Html {
    if e.account {
        return html! { <Link<Route> classes="player" to={Route::PlayerRecords { username: e.player.clone() }}>{ &e.player }</Link<Route>> };
    }
    html! {
        <span class="player legacy" title="Submitted before accounts, under a name anyone could type">
            { &e.player }<span class="badge legacy-badge">{ "legacy" }</span>
        </span>
    }
}

/// A link that opens exactly the board a score was submitted as. `/s/` is a
/// server redirect, so this is a full navigation, not a router link. Scores
/// from before boards never recorded their glue, which it says.
fn board_link(entry: &ScoreEntry, label: &'static str) -> Html {
    html! {
        <>
            <a class="score-board" href={entry.board_link()}>{ label }</a>
            { (!entry.glue_recorded).then(|| html! {
                <span class="muted">{ " · glue not recorded" }</span>
            }) }
        </>
    }
}

fn loading_or_error<T>(state: &Option<Result<T, String>>) -> Option<Html> {
    match state {
        None => Some(html! { <p class="muted">{ "Loading..." }</p> }),
        Some(Err(e)) => Some(html! { <p class="error">{ e }</p> }),
        Some(Ok(_)) => None,
    }
}

#[derive(Properties, PartialEq)]
pub struct LeaderboardProps {
    #[prop_or_default]
    pub player: Option<String>,
    #[prop_or_default]
    pub initial_n: Option<u32>,
    #[prop_or_default]
    pub shape: shared::Shape,
    #[prop_or_default]
    pub container: shared::Shape,
}

fn shape_icon(shape: shared::Shape) -> Html {
    let points = shape
        .vertices(&shared::Placement {
            cx: 0.0,
            cy: 0.0,
            theta: 0.0,
        })
        .iter()
        .map(|(x, y)| {
            format!(
                "{},{}",
                24.0 + x / shape.radius() * 19.0,
                24.0 - y / shape.radius() * 19.0
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    html! { <svg viewBox="0 0 48 48" aria-hidden="true"><polygon {points}/></svg> }
}

/// Filter the public ranking or a unique account's personal records.
#[function_component(Leaderboard)]
pub fn leaderboard(props: &LeaderboardProps) -> Html {
    let account = use_context::<crate::account::Account>();
    let shape = use_state(|| props.shape);
    let container = use_state(|| props.container);
    let n = use_state(|| props.initial_n);
    let records = use_fetch((*shape, *container), |(s, c)| api::known_records_in(s, c));
    let leaders = use_fetch(
        (*shape, *container, *n, props.player.clone()),
        |(s, c, n, p)| api::list_player_scores_in(s, c, n, Some(shared::MAX_N), p),
    );
    let known: &[KnownRecord] = records
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map_or(&[], |v| v.as_slice());
    let scores: &[ScoreEntry] = leaders
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map_or(&[], |v| v.as_slice());
    let counts: Vec<u32> = if let Some(n) = *n {
        vec![n]
    } else if props.player.is_some() {
        scores.iter().map(|e| e.n).collect()
    } else {
        (1..=shared::MAX_N).collect()
    };
    let choose_n = {
        let n = n.clone();
        Callback::from(move |e: Event| {
            let input: web_sys::HtmlSelectElement = e.target_unchecked_into();
            n.set(input.value().parse().ok());
        })
    };
    html! {
        <div class="leaderboard">
            <h1>{props.player.as_ref().map_or_else(||"Leaderboard".into(),|p|format!("{p}’s records"))}</h1>
            {props.player.as_ref().map(|_|html!{<Link<Route> to={Route::Leaderboard}>{"All players"}</Link<Route>>})}
            {account.and_then(|a|a.username).filter(|name|Some(name)!=props.player.as_ref()).map(|username|html!{<p><Link<Route> to={Route::PlayerRecords{username}}>{"My records"}</Link<Route>></p>})}
            <div class="leaderboard-filters">
                <label>{"Number"}<select aria-label="Number of pieces" onchange={choose_n}>
                    <option value="all" selected={n.is_none()}>{"All"}</option>
                    {for (1..=shared::MAX_N).map(|i|html!{<option value={i.to_string()} selected={*n==Some(i)}>{i}</option>})}
                </select></label>
                {for [("Pieces", shape.clone()), ("Container", container.clone())].into_iter().map(|(label, state)|html!{
                    <fieldset><legend>{label}</legend><div class="leaderboard-shapes">
                        {for shared::Shape::ALL.into_iter().map(|s| {let state=state.clone();html!{
                            <button type="button" aria-label={format!("{label}: {s}")} title={s.name()}
                                aria-pressed={(*state==s).to_string()} onclick={Callback::from(move |_|state.set(s))}>{shape_icon(s)}</button>
                        }})}
                    </div></fieldset>
                })}
            </div>
            <p class="muted">{format!("{} in a {}", shape.plural(), *container)}{if props.player.is_some(){" · ranks are among all players"}else{""}}</p>
            {n.map(|n|html!{<p><Link<Route> to={Route::play_in(*shape,*container,n)} classes="button">{"Play this game"}</Link<Route>></p>})}
            {records.as_ref().and_then(|r|r.as_ref().err()).map(|e|html!{<p class="error">{format!("Could not load reference records: {e}")}</p>})}
            {loading_or_error(&leaders).unwrap_or_else(||{
                if props.player.is_some() && scores.is_empty() {return html!{<p class="muted">{"No records for this account in this configuration."}</p>};}
                html!{<table><thead><tr><th>{"n"}</th><th>{"Rank"}</th><th>{"Player"}</th><th>{"Side"}</th><th>{"Best known"}</th><th>{"Gap"}</th><th>{"When"}</th><th>{"Board"}</th></tr></thead>
                    <tbody>{for counts.iter().map(|count|{
                        let rec=known.iter().find(|r|r.n==*count);
                        let entries: Vec<_>=scores.iter().filter(|e|e.n==*count).collect();
                        if entries.is_empty(){return html!{<tr><td>{count}</td><td colspan="3"><Link<Route> to={Route::play_in(*shape,*container,*count)}>{"No submissions · play"}</Link<Route>></td><td>{rec.map(known_cell)}</td><td colspan="3"></td></tr>};}
                        html!{<>{for entries.into_iter().map(|e|html!{<tr>
                            <td><Link<Route> to={Route::leaderboard_in(*shape,*container,*count)}>{count}</Link<Route>></td>
                            <td>{e.rank}</td><td>{player_name(e)}</td>
                            <td><Link<Route> to={Route::Score{id:e.id}}>{fmt_side(e.side)}</Link<Route>></td>
                            <td>{rec.map(known_cell)}</td><td>{rec.map(|r|gap_pct(e.side,r.side)).unwrap_or_default()}</td>
                            <td class="muted">{e.submitted_at.format("%Y-%m-%d %H:%M").to_string()}</td><td>{board_link(e,"Open")}</td>
                        </tr>})}</>}
                    })}</tbody>
                </table>}
            })}
        </div>
    }
}

/// Closed forms longer than this (e.g. "root of a degree-8 polynomial") move
/// into a tooltip so they don't blow out the table column.
const MAX_INLINE_EXPR: usize = 24;

fn known_cell(rec: &KnownRecord) -> Html {
    html! {
        <span title={rec.source.clone()}>
            { fmt_side(rec.side) }
            { rec.side_expr.as_ref().map(|e| if e.chars().count() <= MAX_INLINE_EXPR {
                html! { <span class="muted">{ format!(" = {e}") }</span> }
            } else {
                html! { <span class="muted long-expr" title={e.clone()}>{ " = algebraic ⓘ" }</span> }
            }) }
            { if rec.proven_optimal { html! { <span class="badge">{ "proven" }</span> } } else { html! {} } }
        </span>
    }
}

#[derive(Properties, PartialEq)]
pub struct LeaderboardNProps {
    pub n: u32,
    #[prop_or_default]
    pub shape: shared::Shape,
    #[prop_or_default]
    pub container: shared::Shape,
}

/// Category links initialize the same interactive leaderboard filters.
#[function_component(LeaderboardN)]
pub fn leaderboard_n(props: &LeaderboardNProps) -> Html {
    html! { <Leaderboard key={format!("{}-{}-{}",props.shape,props.container,props.n)} initial_n={Some(props.n)} shape={props.shape} container={props.container} /> }
}

#[derive(Properties, PartialEq)]
pub struct ScorePageProps {
    pub id: Uuid,
}

/// A single submission, with its arrangement drawn.
#[function_component(ScorePage)]
pub fn score_page(props: &ScorePageProps) -> Html {
    let detail = use_fetch(props.id, api::get_score);
    let shape = detail
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(|d| d.entry.shape)
        .unwrap_or_default();
    let container = detail
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(|d| d.entry.container)
        .unwrap_or_default();
    let records = use_fetch((shape, container), |(s, c)| api::known_records_in(s, c));
    if let Some(h) = loading_or_error(&detail) {
        return h;
    }
    let ScoreDetail { entry, arrangement } = detail.as_ref().and_then(|r| r.as_ref().ok()).unwrap();
    let known = records
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .and_then(|r| r.iter().find(|k| k.n == entry.n));
    html! {
        <div class="score-page">
            <h1>{ format!("{} {} by ", entry.n,entry.shape.plural()) }{ player_name(entry) }</h1>
            <p>{ format!("Side {} · rank #{}", fmt_side(entry.side), entry.rank) }</p>
            // Stored scores passed server-side validation.
            <Benchmark
                side={entry.side}
                reference_side={known.map(|k| k.side)}
                proven={known.is_some_and(|k| k.proven_optimal)}
                validated=true />

            <ArrangementSvg arrangement={arrangement.clone()} />
            <p>{ board_link(entry, "Open this board") }</p>
            <p>
                <Link<Route> to={Route::leaderboard_in(entry.shape,entry.container,entry.n)}>{ "Back to leaderboard" }</Link<Route>>
            </p>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct ArrangementSvgProps {
    pub arrangement: Arrangement,
}

/// Static SVG rendering of an arrangement (y axis flipped so the origin is bottom-left).
#[function_component(ArrangementSvg)]
pub fn arrangement_svg(props: &ArrangementSvgProps) -> Html {
    let arr = &props.arrangement;
    let s = arr.side;
    let extent = s * arr.container.extent() * 1.1;
    let view = format!(
        "{} {} {} {}",
        (s - extent) / 2.0,
        (s - extent) / 2.0,
        extent,
        extent
    );
    html! {
        <svg class="arrangement" viewBox={view}>
            <polygon class="container" points={arr.container.container_vertices(s).into_iter().map(|(x,y)|format!("{x},{}",s-y)).collect::<Vec<_>>().join(" ")} />
            { for arr.squares.iter().enumerate().map(|(i, p)| {
                let points = arr.shape.vertices(p)
                    .iter()
                    .map(|(x, y)| format!("{x},{}", s - y))
                    .collect::<Vec<_>>()
                    .join(" ");
                let hue = (i as f64 * 137.508) % 360.0;
                html! { <polygon class="square" points={points} style={format!("fill: hsl({hue:.0}, 60%, 60%)")} /> }
            }) }
        </svg>
    }
}
