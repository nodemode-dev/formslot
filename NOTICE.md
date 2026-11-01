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
| ureq | 3.5.0 | MIT OR Apache-2.0 | distributed binary | the `pack --count remote` HTTPS call, with rustls and the platform certificate verifier, no redirects followed (`core`) |
| keyring | 4.2.0 | MIT OR Apache-2.0 | distributed binary | the Anthropic key from the system password store (`core`) |
| blake3 | 1.8.7 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception; used under Apache-2.0 | distributed binary | identity hashes for dedup (`core`) |
| ctrlc | 3.5.2 | MIT OR Apache-2.0 | distributed binary | SIGINT and SIGTERM (and Ctrl+C on Windows) end `formslot serve` cleanly, so service.json and the lock are removed (`cli`, feature `termination`); its own dependencies are nix 0.31.3 (MIT), cfg-if 1.0.5 and libc 0.2.191 (MIT OR Apache-2.0), bitflags 2.13.2 (MIT OR Apache-2.0), cfg_aliases 0.2.2 (MIT) on Unix, windows-sys 0.61.2 (MIT OR Apache-2.0) on Windows, and dispatch2 0.3.1 (Zlib OR Apache-2.0 OR MIT) with block2 0.6.2, objc2 0.6.5 and objc2-encode 4.1.0 (MIT) on macOS |

The default binary also contains the transitive crates of ureq and keyring, with the versions in `Cargo.lock` and the licences from `cargo metadata`:

| Crate | Version | Licence | Used for |
| --- | --- | --- | --- |
| ureq-proto | 0.6.5 | MIT OR Apache-2.0 | ureq's HTTP/1.1 protocol layer |
| rustls | 0.23.46 | Apache-2.0 OR ISC OR MIT | TLS for ureq |
| rustls-webpki | 0.103.16 | ISC | certificate path validation |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 | certificate and key types |
| rustls-platform-verifier | 0.7.1 | MIT OR Apache-2.0 | checks server certificates against the operating system's store |
| rustls-native-certs | 0.8.5 | Apache-2.0 OR ISC OR MIT | reads the system root store on Linux and other Unix systems |
| openssl-probe | 0.2.2 | MIT OR Apache-2.0 | finds the system certificate files on Linux |
| ring | 0.17.14 | Apache-2.0 AND ISC | cryptography for rustls; parts derive from BoringSSL and OpenSSL, under ISC and the OpenSSL-compatible terms ring's licence file records |
| untrusted | 0.9.0 | ISC | input parsing for ring and webpki |
| subtle | 2.6.1 | BSD-3-Clause | constant-time comparisons |
| webpki-roots | 1.0.11 | CDLA-Permissive-2.0 | the bundled Mozilla root certificate list |
| zbus, zbus_macros, zbus_names | 5.19.0, 5.19.0, 4.3.5 | MIT | D-Bus client for the Linux Secret Service |
| zvariant, zvariant_derive, zvariant_utils | 5.15.1, 5.15.1, 4.2.0 | MIT | D-Bus value encoding |
| secret-service | 5.2.0 | MIT OR Apache-2.0 | Secret Service access |
| zbus-secret-service-keyring-store | 1.0.1 | MIT OR Apache-2.0 | keyring store over the Secret Service |
| keyring-core | 1.0.1 | MIT OR Apache-2.0 | keyring store interface |
| apple-native-keyring-store, windows-native-keyring-store | 1.0.2, 1.1.0 | MIT OR Apache-2.0 | macOS and Windows password stores (those targets only) |
| security-framework, security-framework-sys, core-foundation, core-foundation-sys | 3.7.0, 2.17.0, 0.10.1, 0.8.7 | MIT OR Apache-2.0 | macOS system bindings (macOS only) |
| schannel | 0.1.30 | MIT | Windows certificate store (Windows only) |

Their remaining transitive crates (the async runtime pieces async-*, event-listener, futures-*, polling, rustix, uuid, endi, enumflags2, ordered-stream, the aes, cbc, cipher, hkdf, hmac, sha2 0.11 and num-* helpers, base64 0.23, http, httparse, percent-encoding, utf8-zero, log, bytes, bitflags, hex, windows-sys and the windows_* target crates, and similar) are MIT, Apache-2.0, ISC, BSD-3-Clause, Zlib, Unlicense or MIT-0, alone or as a choice. One lists a copyleft option beside permissive ones (r-efi: MIT OR Apache-2.0 OR LGPL-2.1-or-later), and the permissive choice is the one used; constant_time_eq is CC0-1.0 OR MIT-0 OR Apache-2.0, all permissive. unicode-ident is (MIT OR Apache-2.0) AND Unicode-3.0.

The webpki-roots crate is the Mozilla CA Certificate Store as a Rust data file, licensed CDLA-Permissive-2.0 (Community Data License Agreement, Permissive, Version 2.0). Attribution: the root certificate list is derived from data published by the Mozilla Foundation (https://wiki.mozilla.org/CA) and is redistributed here under that licence, whose text is at https://cdla.dev/permissive-2-0/. The data is unmodified and carries no warranty. With the platform verifier selected, formslot checks `count_tokens` replies against the system store; the bundled list is compiled in and is not consulted by that call.

## Optional dense tier (cargo feature `dense`, off by default)

Compiled only with `--features dense`; the default binary contains none of it.

| Crate | Version | Licence | Where it runs | Used for |
| --- | --- | --- | --- | --- |
| sqlite-vec | 0.1.9 | MIT/Apache-2.0 | dense build only | the `vec0` vector table (`core`) |
| ort | 2.0.0-rc.15 | MIT OR Apache-2.0 | dense build only | onnxruntime bindings, loaded dynamically, nothing fetched at build time (`core`) |
| tokenizers | 0.23.2 | Apache-2.0 | dense build only | the model's tokenizer, pure Rust regex (`core`) |

Their transitive crates (ahash, base64, bit-set, bit-vec, castaway, compact_str, crossbeam-*, daachorse, darling, dary_heap, derive_builder, either, esaxx-rs, fancy-regex, fnv, getrandom 0.3, ident_case, itertools, libloading, log, macro_rules_attribute, minimal-lexical, monostate, nom, once_cell, paste, pastey, rand 0.9, rayon, rayon-cond, rustversion, ryu, spm_precompiled, static_assertions, strsim, thiserror 2, unicode-normalization-alignments, unicode-segmentation, unicode_categories, zerocopy) are MIT, Apache-2.0, ISC, BSD-2-Clause, Zlib or BSL-1.0, alone or as a choice.

| Model or library | Licence | How it arrives |
| --- | --- | --- |
| gte-modernbert-base (`onnx/model_int8.onnx`, `tokenizer.json`, revision e7f32e3c00f91d699e8c43b53106206bcc72bb22), Alibaba-NLP | Apache-2.0 | never bundled; downloaded only by `formslot dense fetch --yes` into the data folder, SHA-256 checked |
| ONNX Runtime shared library (1.23 or newer) | MIT | never bundled and never downloaded by formslot; the owner installs it and points `ORT_DYLIB_PATH` at it, or places it in `<data folder>/dense/lib/` |

curl is found on the system and run as a separate program by `dense fetch`; it is not linked or shipped.

## Derived from formMCP

The files `core/src/http.rs`, `core/src/log.rs`, `core/src/redact.rs` and the root launcher scripts (`formslot`, `formslot.bat`, `formslot_oc_install`, `formslot_oc_install.bat`) come from formMCP (`nodemode-ai/formMCP`), GPL-3.0-only, the same owner.

## Found on the system, never bundled

git (GPL-2.0) is run as a separate program to read repositories; gitleaks (MIT), when present, is run the same way. Neither is linked or shipped. ripgrep (`rg`, MIT or Unlicense) and curl are optional and run the same way; all of gitleaks, rg and curl are found by one resolver: `FORMSLOT_<NAME>_BIN`, then `<data folder>/bin/`, then PATH.
