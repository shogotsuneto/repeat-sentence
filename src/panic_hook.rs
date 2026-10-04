// Reports Rust panics to the browser console. Without a hook, a wasm panic
// surfaces only as `RuntimeError: unreachable executed`. Replaces the archived
// `console_error_panic_hook` crate, which does the same thing.

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn error(msg: String);

    type Error;
    #[wasm_bindgen(constructor)]
    fn new() -> Error;
    #[wasm_bindgen(method, getter)]
    fn stack(this: &Error) -> String;
}

/// Call once at startup.
pub fn install() {
    std::panic::set_hook(Box::new(|info| {
        // A fresh JS Error captures the JS-side stack, which includes the
        // wasm frames leading to the panic.
        let msg = format!("{info}\n\nStack:\n\n{}\n", Error::new().stack());
        crate::diag::log(format!("panic {msg}"));
        error(msg);
    }));
}
