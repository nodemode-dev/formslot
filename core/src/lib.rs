pub type Result<T> = std::result::Result<T, String>;

pub mod api;
pub mod config;
pub mod corpus;
pub mod discovery;
pub mod family;
pub mod git;
pub mod http;
pub mod hygiene;
pub mod licence;
pub mod log;
pub mod redact;
pub mod select;
pub mod store;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const CONTRACT: u32 = 1;
