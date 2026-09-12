mod anneal;
mod api;
mod game;
mod leaderboard;

use leaderboard::{Leaderboard, LeaderboardN, ScorePage};
use shared::MAX_N;
use uuid::Uuid;
use yew::prelude::*;
use yew_router::prelude::*;

#[derive(Clone, Routable, PartialEq)]
enum Route {
    #[at("/")]
    Home,
    #[at("/play/:n")]
    Play { n: u32 },
    #[at("/leaderboard")]
    Leaderboard,
    #[at("/leaderboard/:n")]
    LeaderboardN { n: u32 },
    #[at("/score/:id")]
    Score { id: Uuid },
    #[not_found]
    #[at("/404")]
    NotFound,
}

fn switch(route: Route) -> Html {
    match route {
        Route::Home => html! { <Home /> },
        Route::Play { n } if (1..=MAX_N).contains(&n) => {
            // Keyed so changing n remounts with fresh physics.
            html! { <game::Game key={n} n={n} /> }
        }
        Route::Leaderboard => html! { <Leaderboard /> },
        Route::LeaderboardN { n } => html! { <LeaderboardN {n} /> },
        Route::Score { id } => html! { <ScorePage {id} /> },
        Route::Play { .. } | Route::NotFound => html! { <h1>{ "404 - Not Found" }</h1> },
    }
}

#[function_component(App)]
pub fn app() -> Html {
    html! {
        <BrowserRouter>
            <nav class="topbar">
                <Link<Route> to={Route::Home} classes="brand">{ "packit" }</Link<Route>>
                <Link<Route> to={Route::Leaderboard}>{ "Leaderboard" }</Link<Route>>
            </nav>
            <main>
                <Switch<Route> render={switch} />
            </main>
        </BrowserRouter>
    }
}

#[function_component(Home)]
fn home() -> Html {
    html! {
        <div class="home">
            <h1>{ "Pack the squares" }</h1>
            <p>{ "Fit n unit squares into the smallest square box you can, and chase the best known records." }</p>
            <div class="n-grid">
                { for (1..=30u32).map(|n| html! {
                    <Link<Route> to={Route::Play { n }} classes="n-button">{ n }</Link<Route>>
                }) }
            </div>
        </div>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
