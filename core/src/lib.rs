use wasm_bindgen::prelude::*;

mod crypto;
mod profile;
mod post;
mod message;
mod storage;
mod wasm;

pub use crypto::*;
pub use profile::*;
pub use post::*;
pub use message::*;
pub use storage::*;
pub use wasm::*;

// Set up panic hook and allocator for WASM
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
}

// Core initialization
#[wasm_bindgen]
pub fn init_core() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    web_sys::console::log_1(&"SnartNet Core initialized".into());
    Ok(())
}