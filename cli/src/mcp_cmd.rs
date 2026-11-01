use std::process::ExitCode;

use formslot_core::{VERSION, config, mcp};

pub fn run(args: &[String]) -> ExitCode {
    if let Some(other) = args.first() {
        eprintln!("formslot: mcp takes no arguments (got {other}); it speaks MCP over stdio");
        return ExitCode::from(2);
    }
    eprintln!("formslot mcp {VERSION}: stdio, data folder {}", config::data_dir().display());
    match mcp::stdio::serve() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("formslot: {e}");
            ExitCode::FAILURE
        }
    }
}
