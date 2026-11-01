pub type Result<T> = std::result::Result<T, String>;

pub mod api;
pub mod config;
pub mod corpus;
pub mod dedup;
pub mod discovery;
pub mod facts;
pub mod family;
pub mod git;
pub mod http;
pub mod hygiene;
pub mod licence;
pub mod log;
pub mod memory;
pub mod redact;
pub mod related;
pub mod search;
pub mod select;
pub mod slots;
pub mod store;
pub mod symbols;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const CONTRACT: u32 = 1;
