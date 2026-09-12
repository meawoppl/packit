use wasm_bindgen::{closure::Closure, prelude::*};
use yew::prelude::*;

#[wasm_bindgen(module = "/../physics/engine.js")]
extern "C" {
    #[wasm_bindgen(js_name = createPhysics)]
    fn create_physics(n: u32, side: f64) -> JsValue;
}
#[wasm_bindgen(module = "/src/game/game.js")]
extern "C" {
    #[wasm_bindgen(js_name = mountGame)]
    fn mount_game(
        root: web_sys::Element,
        engine: JsValue,
        shader: &str,
        refine: &js_sys::Function,
    ) -> JsValue;
    #[wasm_bindgen(js_name = unmountGame)]
    fn unmount_game(handle: &JsValue);
}
#[derive(Properties, PartialEq)]
pub struct GameProps {
    pub n: u32,
}

#[function_component(Game)]
pub fn game(props: &GameProps) -> Html {
    let root = use_node_ref();
    let n = props.n;
    {
        let root = root.clone();
        use_effect_with(n, move |n| {
            let mut handle = None;
            let mut callback = None;
            if (1..=shared::MAX_N).contains(n) {
                if let Some(element) = root.cast::<web_sys::Element>() {
                    let refine = Closure::<dyn Fn(String) -> String>::new(|input: String| {
                        match serde_json::from_str::<shared::Arrangement>(&input) {
                            Ok(a) => match solver::refine(&a) {
                                Ok(r) => serde_json::to_string(&r).unwrap_or_default(),
                                Err(e) => serde_json::json!({"error":e}).to_string(),
                            },
                            Err(e) => serde_json::json!({"error":e.to_string()}).to_string(),
                        }
                    });
                    handle = Some(mount_game(
                        element,
                        create_physics(*n, (*n as f64).sqrt().ceil() + 0.5),
                        physics::GPU_KERNEL,
                        refine.as_ref().unchecked_ref(),
                    ));
                    callback = Some(refine);
                }
            }
            move || {
                if let Some(h) = handle {
                    unmount_game(&h);
                }
                drop(callback);
            }
        });
    }
    if !(1..=shared::MAX_N).contains(&n) {
        return html! {<p>{format!("Choose between 1 and {} squares.",shared::MAX_N)}</p>};
    }
    html! {<div ref={root} class="packing-game"/>}
}
