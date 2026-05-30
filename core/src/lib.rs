#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

mod crypto;
mod invite;
mod message;
mod post;
mod profile;
pub mod service;
mod storage;
#[cfg(target_arch = "wasm32")]
mod wasm;

pub use crypto::*;
pub use invite::*;
pub use message::*;
pub use post::*;
pub use profile::*;
pub use service::{
    CapabilityDescriptor, CoreService, CreateProfileRequest, ProfileEnvelope, UpdateProfileRequest,
};
pub use storage::*;
#[cfg(target_arch = "wasm32")]
pub use wasm::*;

// Removed unused wee_alloc allocator block

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
}

// Core initialization
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn init_core() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    web_sys::console::log_1(&"SnartNet Core initialized".into());
    Ok(())
}
