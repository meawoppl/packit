mod account;
mod anneal;
mod api;
mod benchmark;
mod game;
mod leaderboard;
mod webauthn;

use account::{AccountMenu, AccountProvider};
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
    #[at("/play/:shape/:n")]
    Polygon { shape: String, n: u32 },
    #[at("/play/:shape/:container/:n")]
    Container {
        shape: String,
        container: String,
        n: u32,
    },
    #[at("/leaderboard/:shape/:container/:n")]
    ContainerLeaderboard {
        shape: String,
        container: String,
        n: u32,
    },
    #[at("/leaderboard")]
    Leaderboard,
    #[at("/leaderboard/:n")]
    LeaderboardN { n: u32 },
    #[at("/leaderboard/:shape/:n")]
    PolygonLeaderboard { shape: String, n: u32 },
    #[at("/score/:id")]
    Score { id: Uuid },
    #[not_found]
    #[at("/404")]
    NotFound,
}

fn switch(route: Route) -> Html {
    match route {
        Route::Home => html! { <game::Game key={17u32} n={17} /> },
        Route::Play { n } if (1..=MAX_N).contains(&n) => {
            // Keyed so changing n remounts with fresh physics.
            html! { <game::Game key={n} n={n} /> }
        }
        Route::Polygon { shape, n } if (1..=MAX_N).contains(&n) => {
            match shape.parse::<shared::Shape>() {
                Ok(shape) => html! {<game::Game key={format!("{shape}-{n}")} {n} {shape} />},
                Err(_) => html! {<h1>{"Unknown shape"}</h1>},
            }
        }
        Route::Container {
            shape,
            container,
            n,
        } if (1..=MAX_N).contains(&n) => match (
            shape.parse::<shared::Shape>(),
            container.parse::<shared::Shape>(),
        ) {
            (Ok(shape), Ok(container)) => {
                html! {<game::Game key={format!("{shape}-{container}-{n}")} {n} {shape} {container}/>}
            }
            _ => html! {<h1>{"Unknown shape"}</h1>},
        },
        Route::ContainerLeaderboard {
            shape,
            container,
            n,
        } => match (
            shape.parse::<shared::Shape>(),
            container.parse::<shared::Shape>(),
        ) {
            (Ok(shape), Ok(container)) => html! {<LeaderboardN {n} {shape} {container}/>},
            _ => html! {<h1>{"Unknown shape"}</h1>},
        },
        Route::Leaderboard => html! { <Leaderboard /> },
        Route::LeaderboardN { n } => html! { <LeaderboardN {n} /> },
        Route::PolygonLeaderboard { shape, n } => match shape.parse::<shared::Shape>() {
            Ok(shape) => html! {<LeaderboardN {shape} {n} />},
            Err(_) => html! {<h1>{"Unknown shape"}</h1>},
        },
        Route::Score { id } => html! { <ScorePage {id} /> },
        Route::Container { .. } | Route::Play { .. } | Route::Polygon { .. } | Route::NotFound => {
            html! { <h1>{ "404 - Not Found" }</h1> }
        }
    }
}

#[function_component(HeaderPicker)]
fn header_picker() -> Html {
    let configuration = match use_route::<Route>() {
        Some(Route::Home) => Some((17, shared::Shape::Square, shared::Shape::Square)),
        Some(Route::Play { n }) => Some((n, shared::Shape::Square, shared::Shape::Square)),
        Some(Route::Polygon { shape, n }) => shape
            .parse()
            .ok()
            .map(|shape| (n, shape, shared::Shape::Square)),
        Some(Route::Container {
            shape,
            container,
            n,
        }) => shape
            .parse()
            .ok()
            .zip(container.parse().ok())
            .map(|(shape, container)| (n, shape, container)),
        _ => None,
    };
    match configuration {
        Some((n, shape, container)) if (1..=MAX_N).contains(&n) => html! {
            <game::picker::Picker key={format!("{shape}-{container}-{n}")} {n} {shape} {container} />
        },
        _ => Html::default(),
    }
}

#[function_component(App)]
pub fn app() -> Html {
    html! {
        <BrowserRouter>
            <AccountProvider>
                <nav class="topbar">
                    <Link<Route> to={Route::Home} classes="brand">{ "Potatos" }</Link<Route>>
                    <a class="tagline" href="https://x.com/meawoppl/status/2097861010388554039"
                        target="_blank" rel="noopener noreferrer">{ "pack taters, impress your wife" }</a>
                    <Link<Route> to={Route::Leaderboard}>{ "Leaderboard" }</Link<Route>>
                    <HeaderPicker />
                    <AccountMenu />
                </nav>
                <main>
                    <Switch<Route> render={switch} />
                </main>
            </AccountProvider>
        </BrowserRouter>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}

impl Route {
    pub fn play_in(shape: shared::Shape, container: shared::Shape, n: u32) -> Self {
        if container.is_square() {
            Self::play(shape, n)
        } else {
            Self::Container {
                shape: shape.to_string(),
                container: container.to_string(),
                n,
            }
        }
    }
    pub fn leaderboard_in(shape: shared::Shape, container: shared::Shape, n: u32) -> Self {
        if container.is_square() {
            Self::leaderboard(shape, n)
        } else {
            Self::ContainerLeaderboard {
                shape: shape.to_string(),
                container: container.to_string(),
                n,
            }
        }
    }
    pub fn play(shape: shared::Shape, n: u32) -> Self {
        if shape.is_square() {
            Self::Play { n }
        } else {
            Self::Polygon {
                shape: shape.to_string(),
                n,
            }
        }
    }
}

impl Route {
    fn leaderboard(shape: shared::Shape, n: u32) -> Self {
        if shape.is_square() {
            Self::LeaderboardN { n }
        } else {
            Self::PolygonLeaderboard {
                shape: shape.to_string(),
                n,
            }
        }
    }
}
