use std::io::{self, BufRead, Write};

use super::answer_text;

pub fn serve() -> crate::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for raw in stdin.lock().split(b'\n') {
        let raw = raw.map_err(|e| format!("stdin could not be read ({e})"))?;
        let line = String::from_utf8_lossy(&raw);
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = answer_text(&line, "stdio") {
            writeln!(out, "{reply}")
                .and_then(|()| out.flush())
                .map_err(|e| format!("stdout could not be written ({e})"))?;
        }
    }
    Ok(())
}
