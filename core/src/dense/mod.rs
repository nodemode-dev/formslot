use std::path::PathBuf;
use std::sync::Once;

use rusqlite::Connection;

use crate::config;

pub mod embed;
pub mod fetch;
pub mod vectors;

pub use vectors::{build, run, status, BuildStats, CorpusStatus};

pub const MODEL: &str = "gte-modernbert-base";
pub const MODEL_REVISION: &str = "e7f32e3c00f91d699e8c43b53106206bcc72bb22";
pub const MODEL_LICENCE: &str = "Apache-2.0";
pub const MODEL_PAGE: &str = "https://huggingface.co/Alibaba-NLP/gte-modernbert-base";
pub const DIM: usize = 768;
pub const MAX_TOKENS: usize = 512;
pub const BODY_CLIP: usize = 1600;
pub const TABLE: &str = "unit_vec";

pub fn dense_dir() -> PathBuf {
    config::data_dir().join("dense")
}

pub fn model_dir() -> PathBuf {
    dense_dir().join(MODEL)
}

pub fn runtime_dir() -> PathBuf {
    dense_dir().join("lib")
}

pub fn register() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(sqlite_vec::sqlite3_vec_init as *const ())));
    });
}

pub fn ensure_table(conn: &Connection) -> crate::Result<()> {
    conn.execute_batch(&format!(
        "CREATE VIRTUAL TABLE IF NOT EXISTS {TABLE} USING vec0(embedding float[{DIM}] distance_metric=cosine)"
    ))
    .map_err(|e| format!("dense: {e}"))
}

pub fn vec_version(conn: &Connection) -> crate::Result<String> {
    conn.query_row("SELECT vec_version()", [], |r| r.get(0))
        .map_err(|e| format!("dense: sqlite-vec is not loaded ({e})"))
}

pub fn fetch_hint() -> String {
    format!(
        "the dense model {MODEL} is not in {}. `formslot dense fetch` shows what it would download; `formslot dense fetch --yes` downloads it",
        model_dir().display()
    )
}
