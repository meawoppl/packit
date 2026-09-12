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

fn loading_or_error<T>(state: &Option<Result<T, String>>) -> Option<Html> {
    match state {
        None => Some(html! { <p class="muted">{ "Loading..." }</p> }),
        Some(Err(e)) => Some(html! { <p class="error">{ e }</p> }),
        Some(Ok(_)) => None,
    }
}

/// Overview: every `n` with a known record, alongside the best player result.
#[function_component(Leaderboard)]
pub fn leaderboard() -> Html {
    let records = use_fetch((), |_| api::known_records());
    let leaders = use_fetch((), |_| api::list_scores(None, Some(shared::MAX_N)));

    if let Some(h) = loading_or_error(&records).or_else(|| loading_or_error(&leaders)) {
        return h;
    }
    let records = records.as_ref().and_then(|r| r.as_ref().ok()).unwrap();
    let leaders: &[ScoreEntry] = leaders.as_ref().and_then(|r| r.as_ref().ok()).unwrap();

    html! {
        <div class="leaderboard">
            <h1>{ "Leaderboard" }</h1>
            <table>
                <thead>
                    <tr>
                        <th>{ "n" }</th>
                        <th>{ "Best known" }</th>
                        <th>{ "Top player" }</th>
                        <th>{ "Side" }</th>
                        <th>{ "Gap" }</th>
                    </tr>
                </thead>
                <tbody>
                    { for records.iter().map(|rec| {
                        let top = leaders.iter().find(|e| e.n == rec.n);
                        html! {
                            <tr>
                                <td>
                                    <Link<Route> to={Route::LeaderboardN { n: rec.n }}>{ rec.n }</Link<Route>>
                                </td>
                                <td>{ known_cell(rec) }</td>
                                { match top {
                                    Some(e) => html! {
                                        <>
                                            <td>{ &e.player }</td>
                                            <td>{ fmt_side(e.side) }</td>
                                            <td>{ gap_pct(e.side, rec.side) }</td>
                                        </>
                                    },
                                    None => html! {
                                        <td colspan="3" class="muted">
                                            <Link<Route> to={Route::Play { n: rec.n }}>{ "unclaimed, play it" }</Link<Route>>
                                        </td>
                                    },
                                } }
                            </tr>
                        }
                    }) }
                </tbody>
            </table>
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
}

/// Every submission for one `n`, ranked.
#[function_component(LeaderboardN)]
pub fn leaderboard_n(props: &LeaderboardNProps) -> Html {
    let n = props.n;
    let scores = use_fetch(n, |n| api::list_scores(Some(n), None));
    let records = use_fetch((), |_| api::known_records());
    let known = records
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .and_then(|r| r.iter().find(|k| k.n == n).cloned());

    html! {
        <div class="leaderboard">
            <h1>{ format!("{n} squares") }</h1>
            <p>
                { match &known {
                    Some(k) => html! { <>{ "Best known side: " }{ known_cell(k) }{ format!(" ({})", k.source) }</> },
                    None => html! { <span class="muted">{ "No literature record loaded for this n." }</span> },
                } }
            </p>
            <Link<Route> to={Route::Play { n }} classes="button">{ "Play this n" }</Link<Route>>
            { loading_or_error(&scores).unwrap_or_else(|| {
                let scores = scores.as_ref().and_then(|r| r.as_ref().ok()).unwrap();
                if scores.is_empty() {
                    return html! { <p class="muted">{ "No submissions yet. Be the first." }</p> };
                }
                html! {
                    <table>
                        <thead>
                            <tr>
                                <th>{ "#" }</th>
                                <th>{ "Player" }</th>
                                <th>{ "Side" }</th>
                                <th>{ "Gap" }</th>
                                <th>{ "When" }</th>
                            </tr>
                        </thead>
                        <tbody>
                            { for scores.iter().map(|e| html! {
                                <tr>
                                    <td>{ e.rank }</td>
                                    <td>
                                        <Link<Route> to={Route::Score { id: e.id }}>{ &e.player }</Link<Route>>
                                    </td>
                                    <td>{ fmt_side(e.side) }</td>
                                    <td>{ known.as_ref().map(|k| gap_pct(e.side, k.side)).unwrap_or_default() }</td>
                                    <td class="muted">{ e.submitted_at.format("%Y-%m-%d %H:%M").to_string() }</td>
                                </tr>
                            }) }
                        </tbody>
                    </table>
                }
            }) }
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct ScorePageProps {
    pub id: Uuid,
}

/// A single submission, with its arrangement drawn.
#[function_component(ScorePage)]
pub fn score_page(props: &ScorePageProps) -> Html {
    let detail = use_fetch(props.id, api::get_score);
    let records = use_fetch((), |_| api::known_records());
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
            <h1>{ format!("{} squares by {}", entry.n, entry.player) }</h1>
            <p>{ format!("Side {} · rank #{}", fmt_side(entry.side), entry.rank) }</p>
            // Stored scores passed server-side validation.
            <Benchmark
                side={entry.side}
                reference_side={known.map(|k| k.side)}
                proven={known.is_some_and(|k| k.proven_optimal)}
                validated=true />

            <ArrangementSvg arrangement={arrangement.clone()} />
            <p>
                <Link<Route> to={Route::LeaderboardN { n: entry.n }}>{ "Back to leaderboard" }</Link<Route>>
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
    let view = format!("{} {} {} {}", -0.05 * s, -0.05 * s, 1.1 * s, 1.1 * s);
    html! {
        <svg class="arrangement" viewBox={view}>
            <rect class="container" x="0" y="0" width={s.to_string()} height={s.to_string()} />
            { for arr.squares.iter().enumerate().map(|(i, p)| {
                let points = p
                    .corners()
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
