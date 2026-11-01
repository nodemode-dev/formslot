# NOTICE

formslot
Copyright (C) 2026 NodeAgentc, Inc.

Licensed under the GNU General Public License v3.0 (SPDX `GPL-3.0-only`). See `LICENSE`. The third-party work listed below keeps its own licence; all of it is compatible with GPL-3.0.

## Dependencies

Crates added directly, with the versions in `Cargo.lock`. Each runs in the distributed binary.

| Crate | Version | Licence | Where it runs | Used for |
| --- | --- | --- | --- | --- |
| serde | 1.0.229 | MIT OR Apache-2.0 | distributed binary | the discovery file and messages (`core`) |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | distributed binary | JSON on the wire and in the log (`core`, `cli`) |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | distributed binary | content digests for the corpus store (`core`) |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | distributed binary | the access token, from the operating system's random source (`core`) |
| regex | 1.13.1 | MIT OR Apache-2.0 | distributed binary | secret shapes in redaction (`core`) |
| rusqlite | 0.41.1 | MIT | distributed binary | the corpus store (`core`, `cli`); its bundled SQLite (libsqlite3-sys 0.39.1) is public domain |
| tree-sitter | 0.27.1 | MIT | distributed binary | parsing source files for symbols (`core`) |
| tree-sitter-typescript | 0.23.2 | MIT | distributed binary | TypeScript and TSX grammar (`core`) |
| tree-sitter-python | 0.25.0 | MIT | distributed binary | Python grammar (`core`) |
| tree-sitter-rust | 0.24.2 | MIT | distributed binary | Rust grammar (`core`) |
| tree-sitter-css | 0.25.0 | MIT | distributed binary | CSS grammar (`core`) |
| tree-sitter-html | 0.23.2 | MIT | distributed binary | HTML grammar (`core`) |
| blake3 | 1.8.7 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception; used under Apache-2.0 | distributed binary | identity hashes for dedup (`core`) |

## Derived from formMCP

The files `core/src/http.rs`, `core/src/log.rs`, `core/src/redact.rs` and the root launcher scripts (`formslot`, `formslot.bat`, `formslot_oc_install`, `formslot_oc_install.bat`) come from formMCP (`nodemode-ai/formMCP`), GPL-3.0-only, the same owner.

## Found on the system, never bundled

git (GPL-2.0) is run as a separate program to read repositories; gitleaks (MIT), when present, is run the same way. Neither is linked or shipped.
