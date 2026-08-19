#[cfg(any(target_os = "macos", target_os = "windows"))]
pub mod computer;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[path = "computer_unsupported.rs"]
pub mod computer;
mod computer_protocol;
mod error_taxonomy;
pub mod hub;
pub mod server;
pub mod token;
pub mod update;
pub mod ws_auth;

pub use server::{BridgeServer, ServerConfig};
pub use token::{create_token, load_or_create_token, tokens_equal};
pub use update::{UpdateState, UpdateStatus, check_for_update};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROTOCOL_VERSION: u64 = 1;
