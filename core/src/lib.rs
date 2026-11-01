pub type Result<T> = std::result::Result<T, String>;

pub mod api;
pub mod binres;
pub mod check;
pub mod config;
pub mod count;
pub mod corpus;
pub mod dedup;
#[cfg(feature = "dense")]
pub mod dense;
pub mod discovery;
pub mod eval;
pub mod eval_model;
pub mod facts;
pub mod family;
pub mod git;
pub mod hook;
pub mod http;
pub mod hygiene;
pub mod install;
pub mod jsonedit;
pub mod licence;
pub mod lifecycle;
pub mod log;
pub mod mcp;
pub mod memory;
pub mod redact;
pub mod related;
pub mod search;
pub mod select;
pub mod setup;
pub mod slots;
pub mod store;
pub mod symbols;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const CONTRACT: u32 = 1;
