//! Compact game summary and a native modal for choosing the next game.
use crate::Route;
use shared::{Shape, MAX_N};
use wasm_bindgen::JsCast;
use web_sys::{HtmlDialogElement, HtmlElement, HtmlInputElement};
use yew::prelude::*;
use yew_router::prelude::*;

#[derive(Properties, PartialEq)]
pub struct Props {
    pub n: u32,
    pub shape: Shape,
    pub container: Shape,
}
fn points(shape: Shape, radius: f64) -> String {
    shape
        .vertices(&shared::Placement {
            cx: 0.0,
            cy: 0.0,
            theta: 0.0,
        })
        .into_iter()
        .map(|(x, y)| {
            format!(
                "{},{}",
                32.0 + x / shape.radius() * radius,
                32.0 - y / shape.radius() * radius
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}
#[function_component(Picker)]
pub fn picker(props: &Props) -> Html {
    let open = use_state(|| false);
    let count = use_state(|| props.n.to_string());
    let piece = use_state(|| props.shape);
    let container = use_state(|| props.container);
    let dialog = use_node_ref();
    let opener = use_node_ref();
    let input = use_node_ref();
    {
        let dialog = dialog.clone();
        let opener = opener.clone();
        let input = input.clone();
        use_effect_with(*open, move |open| {
            let body = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.body());
            let mut overflow = None;
            if let Some(d) = dialog.cast::<HtmlDialogElement>() {
                if *open {
                    if d.show_modal().is_ok() {
                        if let Some(b) = &body {
                            overflow = Some((
                                b.style().get_property_value("overflow").unwrap_or_default(),
                                b.style().get_property_priority("overflow"),
                            ));
                            let _ = b.style().set_property("overflow", "hidden");
                        }
                        if let Some(i) = input.cast::<HtmlInputElement>() {
                            let _ = i.focus();
                        }
                    }
                } else if d.open() {
                    d.close();
                    if let Some(o) = opener.cast::<HtmlElement>() {
                        let _ = o.focus();
                    }
                }
            }
            move || {
                if let (Some(b), Some((value, priority))) = (body, overflow) {
                    let _ = b
                        .style()
                        .set_property_with_priority("overflow", &value, &priority);
                }
            }
        });
    }
    let show = {
        let open = open.clone();
        let count = count.clone();
        let piece = piece.clone();
        let container = container.clone();
        let (n, s, c) = (props.n, props.shape, props.container);
        Callback::from(move |_| {
            count.set(n.to_string());
            piece.set(s);
            container.set(c);
            open.set(true);
        })
    };
    let cancel = {
        let open = open.clone();
        Callback::from(move |_| open.set(false))
    };
    let escape = {
        let open = open.clone();
        Callback::from(move |e: Event| {
            e.prevent_default();
            open.set(false);
        })
    };
    let backdrop = {
        let open = open.clone();
        let dialog = dialog.clone();
        Callback::from(move |e: MouseEvent| {
            if e.target()
                == dialog
                    .cast::<HtmlDialogElement>()
                    .map(|d| d.unchecked_into())
            {
                open.set(false);
            }
        })
    };
    let nav = use_navigator();
    let n = count
        .parse::<u32>()
        .ok()
        .filter(|n| (1..=MAX_N).contains(n));
    let apply = {
        let open = open.clone();
        let s = *piece;
        let c = *container;
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            if let (Some(n), Some(nav)) = (n, nav.as_ref()) {
                open.set(false);
                nav.push(&Route::play_in(s, c, n));
            }
        })
    };
    html! {
        <div class="pg-picker">
            <button class="pg-game-picker" ref={opener} onclick={show} aria-haspopup="dialog" aria-label={format!("Choose game: {} {} in a {}",props.n,props.shape.plural(),props.container)}>
                <strong>{props.n}</strong>
                <svg viewBox="0 0 64 64" aria-hidden="true"><polygon class="enclosing" points={points(props.container,29.0)}/><polygon class="enclosed" points={points(props.shape,10.0)}/></svg>
            </button>
            <dialog class="pg-picker-modal" ref={dialog} oncancel={escape} onclick={backdrop} aria-labelledby="game-picker-title">
                <form onsubmit={apply}>
                    <h2 id="game-picker-title">{"Choose your packing"}</h2>
                    <label for="game-count">{"Number of pieces"}</label>
                    <input id="game-count" ref={input} type="number" min="1" max={MAX_N.to_string()} required=true value={(*count).clone()}
                        oninput={let count=count.clone();Callback::from(move |e:InputEvent|count.set(e.target_unchecked_into::<HtmlInputElement>().value()))}/>
                    <fieldset><legend>{"Enclosed shape"}</legend><div class="pg-shape-options">
                        {for Shape::ALL.into_iter().map(|s|{let piece=piece.clone();html!{<button type="button" aria-pressed={(*piece==s).to_string()} onclick={Callback::from(move |_|piece.set(s))}><svg viewBox="0 0 64 64" aria-hidden="true"><polygon points={points(s,26.0)}/></svg>{s.name()}</button>}})}
                    </div></fieldset>
                    <fieldset><legend>{"Enclosing shape"}</legend><div class="pg-shape-options">
                        {for Shape::ALL.into_iter().map(|s|{let container=container.clone();html!{<button type="button" aria-pressed={(*container==s).to_string()} onclick={Callback::from(move |_|container.set(s))}><svg viewBox="0 0 64 64" aria-hidden="true"><polygon points={points(s,26.0)}/></svg>{s.name()}</button>}})}
                    </div></fieldset>
                    <div class="pg-actions"><button type="button" onclick={cancel}>{"Cancel"}</button><button class="pg-picker-confirm" type="submit" disabled={n.is_none()}>{"Play"}</button></div>
                </form>
            </dialog>
        </div>
    }
}
