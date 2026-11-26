# setup HASHOUT log

Rationale for the packaging scripts in this folder. Entries are added at the end.

### setup/build_release · build_release

- refers-to: setup/build_release :: build_release
- kind: rationale
- logged: v0.10.2 · 2026-10-30

formMCP packages with a single `build_app` script at the repository root that builds, bundles and logs; formcompiler drives a `compile_vX.Y.Z` branch and collects what the script leaves. formslot has no window, so the release is only the binary plus what it reads at run time. The version is read from the workspace `version` line of `Cargo.toml`, so the folder name can never disagree with `formslot version`. `FORMSLOT_DIST_DIR` and `CARGO_TARGET_DIR` let formcompiler or a test run stage outside the tree. The binary is renamed from `formslot-cli` to `formslot` because hooks and the doctor look for that name. Checksums are written with relative paths in sorted order so the file is reproducible and `sha256sum -c` works from inside the folder.

### setup/build_release · data files

- refers-to: core/src/memory/declines.rs :: DEFAULT
- kind: rationale
- logged: v0.10.2 · 2026-10-30

`data/declines.json` is already embedded with `include_str!` and replaced by `<data folder>/declines.json`, so the binary needs nothing beside it for that. `data/slots/` is not read by the binary on its own: `formslot slot seed <dir>` takes the folder as an argument, so there is no lookup to relocate. The release therefore ships `data/` whole beside the binary and no core change was made.

### setup/build_release.bat · Windows

- refers-to: setup/build_release.bat :: build_release
- kind: warning-context
- logged: v0.10.2 · 2026-10-30

The Windows script could not be run in this environment (Linux only). It uses PowerShell `Get-FileHash` and `Compress-Archive`, both present on Windows 10 and later. Run it once on Windows before relying on it.

### formslot_oc_install · --user

- refers-to: formslot_oc_install :: --user
- kind: rationale
- logged: v0.10.2 · 2026-10-30

The installer decides between a source checkout and an unpacked release by the presence of `cli/` and `core/`, because the file named `formslot` is a launcher script in a checkout and the binary in a release. The copy goes through a temporary name and a rename so a running service never sees a half-written binary. Shell profiles and the Windows PATH are never edited; the line to add is printed.

### setup/package_skill · package_skill

- refers-to: setup/package_skill :: package_skill
- kind: rationale
- logged: v0.10.2 · 2026-10-30

The gates script is run from the god-tier skill creator, not copied into the repository. The extra checks (name equals folder, no XML-like tags in the frontmatter, no emoji, no U+2014, one SKILL.md, no README) are repeated inline so the gate does not silently weaken if the validator changes. The archive is made with `zip -X` from the skills folder so the skill folder is the archive root.

### Install formslot.command · double-click installer

- refers-to: Install formslot.command :: close_window
- kind: rationale
- logged: v0.13.0 · 2026-10-30

Finder opens a `.command` file in a Terminal window that closes when the script ends, so the script changes into its own folder, builds with `formslot_oc_install` only when a source checkout has no binary, runs `./formslot setup` and waits for Enter in an EXIT trap, which also covers the failed-build exit. `FORMSLOT_NO_SETUP_OFFER=1` stops `formslot_oc_install` from asking to run setup a second time. The file is executable; a downloaded copy needs a right-click and Open once because of Gatekeeper. `Install formslot.bat` is the same flow with `call` and a final prompt; it was not run here (no Windows machine). Both are staged by `setup/build_release` and `build_release.bat`.

### formslot, formslot.bat · working directory

- refers-to: formslot :: here
- kind: warning-context
- logged: v0.13.0 · 2026-10-30

Both launchers used to change into the checkout before running the binary, so `formslot setup` would have taken the checkout as "the current folder" and resolved a relative `--repo` against it. They now build inside the checkout (a subshell, `pushd`) and run the binary from where the caller stood. A first run with no binary says that building takes a few minutes.

### formslot_oc_install · offer_setup

- refers-to: formslot_oc_install :: offer_setup
- kind: rationale
- logged: v0.13.0 · 2026-10-30

The installer ends by asking `Run the guided setup now? [Y/n]` only when stdin and stdout are terminals and `FORMSLOT_NO_SETUP_OFFER` is unset, so piped use and the double-click installers behave. The commands it used to print are kept for the non-terminal case. The `.bat` twin asks with `set /p` and runs setup when the answer is empty or starts with y; setup itself refuses without a terminal.

### setup/build_release.bat · --no-sums

- refers-to: setup/build_release.bat :: SUMS
- kind: rationale
- logged: v0.16.3 · 2026-11-22

formcompiler signs every unsigned `.exe` inside a release `.zip` after the build and writes the zip again with the signed program in it, then publishes its own `SHA256SUMS.txt` over the files it releases. The `SHA256SUMS` this script wrote inside the folder before zipping then named the unsigned `formslot.exe`, so the shipped file was wrong, and the `.zip.sha256` beside the zip named a zip that no longer existed. `--no-sums` writes neither, and removes a `.zip.sha256` left by an earlier run, so a family build ships no checksum that can go stale; the release's own `SHA256SUMS.txt` covers the zip. Without the option the script behaves as before. The option is a flag rather than a test for formcompiler's environment, so the registry entry says what it wants and a local run never changes by accident. The registry's Windows command is `call setup\build_release.bat --no-sums`. The shell script has no such option: nothing touches the `.tar.gz` after it is written.

### setup/build_release · --dmg and APPLE_SIGNING_IDENTITY

- refers-to: setup/build_release :: dmg
- kind: rationale
- logged: v0.16.3 · 2026-11-22

formcompiler notarizes and staples disk images only, and signs an unsigned one with the Developer ID first; Apple's notary refuses an image whose program is not signed with a Developer ID, the hardened runtime and a secure timestamp. So the program is signed here, right after it is staged and before the checksums are written, whenever `APPLE_SIGNING_IDENTITY` is set on macOS (formcompiler's Build step sets it only when its build keychain holds that identity), and `--dmg` packs the staged folder into `formslot-vX.Y.Z-macos-<arch>.dmg` with `hdiutil`, as the family's other disk images are made. The release build has no `dense` feature, so no library is loaded at run time and the hardened runtime needs no entitlement. No `.dmg.sha256` is written, because formcompiler signs and staples the image after the build. The `.tar.gz` is still made and holds the same signed program; its checksums stay true. A bare program cannot carry a stapled ticket, so a copy taken out of the image or the archive is checked online by Gatekeeper on first run. `--dmg` on Linux stops with an error. The registry's macOS command is `setup/build_release --dmg`, and its artifacts gain `dist/formslot-v*-macos-*.dmg`.

### .github/workflows/ci.yml · the check formcompiler waits for

- refers-to: .github/workflows/ci.yml :: ci
- kind: rationale
- logged: v0.16.3 · 2026-11-22

formcompiler builds a commit only when the registry's `checks` (`ci.yml`) concluded success on that exact commit, so the workflow runs on pushes to `main` and `compile_v*`, on pull requests and by hand, and never cancels a run on those branches. Jobs: `cargo test --locked` for the workspace (it compiles both crates; there are no tests yet, by the owner's rule); the Linux release script, with the archive's listing and both checksum files checked and the program asked for its version; the Windows script run once as it stands alone and once with `--no-sums`, as formcompiler runs it, so the `.bat` runs on every commit and not only in a compile; `bash -n` on the shell scripts and the em dash and emoji scan the family uses, reading `git ls-files -z` so the file names with spaces are scanned. Actions are pinned by major version as in the family's other check workflows. The macOS `--dmg` path is not run here (macOS runner minutes); it first runs in the compile.

### formslot, formslot.bat · the twins agree on cargo and on the build folder

- refers-to: formslot :: target
- kind: rationale
- logged: v0.16.4 · 2026-11-22

The `.bat` launcher demanded cargo before it looked for the program, so a built checkout stopped working the moment cargo left PATH, while the shell launcher needed cargo only to rebuild. Both now ask the same question in the same order: with cargo on PATH they build what is missing or out of date (the shell launcher compares file times, the `.bat` lets cargo decide, which costs a fraction of a second); with no cargo they run the program that is already built, and only a checkout with no program at all prints the Rust line and exits 1. Both installers build into `CARGO_TARGET_DIR` when it is set, but the launchers looked only in `target/`, so a person with that variable set built a second copy on the first `./formslot`. The launchers now read the same variable; a relative value is taken from the checkout, because that is where they run cargo. The two double-click installers still test `target/release` before calling the installer: with the variable set they call it once more, cargo finds nothing to do, and setup starts.

### .gitignore · only what this repository makes

- refers-to: .gitignore :: target
- kind: removed-code
- logged: v0.16.4 · 2026-11-22

The file carried the family template's lines for Node, Python and a Tauri shell (`node_modules/`, `venv/`, `__pycache__/`, `src-tauri/target/`, `build_app.log`, `.env` and others). This repository has none of those. A build writes `target/` and the release and skill scripts write `dist/`; those two lines remain, anchored at the root. The log file is written in the data folder (`core/src/config.rs`), never in the checkout.

### .github/workflows/ci.yml · the launchers run on every commit

- refers-to: .github/workflows/ci.yml :: release-linux
- kind: rationale
- logged: v0.16.4 · 2026-11-22

The Linux and Windows release jobs now also run `formslot version` through the launcher after the release script has built the program, and once more with cargo off PATH. The Linux job also sets `CARGO_TARGET_DIR` to a folder outside the checkout and checks that the launcher built there. The Windows steps have not run anywhere yet; the first run on GitHub is their test.

### CHANGELOG.md · a section with every release

- refers-to: CHANGELOG.md
- kind: rationale
- logged: v0.18.2 · 2026-11-22

There was no changelog: the version notes left the README at the overhaul, and 0.16.1 to 0.16.3 lived only in commits and log entries. The file starts at 0.16.0 and was written from `git log` (subjects of the commits between release commits), in words for a person who uses formslot, not for one who reads the code. 0.16.1 has no commit on `main` (the overhaul commit moved the version from 0.16.0 to 0.16.2), and the file says so and guesses nothing. From here the release commit adds its section, and the development skill lists the changelog beside the five version sites. Dates are US Eastern, as on the commits.

### setup/build_release · the Linux .zip

- refers-to: setup/build_release :: the zip step; .github/workflows/ci.yml :: release-linux (The archive holds the program, and its checksums hold)
- kind: rationale
- logged: v0.18.4 · 2026-11-25

On Linux, FormPortal listed FormSlot as "Not built for this computer yet". The Linux build was there, as a `.tar.gz`, but formcompiler's catalogue and FormPortal both leave `.tar.gz` files out of what can be installed (for apps with a window that ending is the updater's file), and both already read a `.zip` whose name says `linux`. A new file kind in the catalogue was the other way and was not taken: FormPortal versions already on people's computers would meet a kind they do not know.

So on Linux the script also packs the staged folder into `dist/formslot-vX.Y.Z-linux-<arch>.zip`, with `zip -r -y -X` (links kept as links, no extra attributes) run from `dist`, so the zip holds the one folder the `.tar.gz` holds and the file modes of `formslot`, `formslot_oc_install` and `Install formslot.command` (755) ride in it. FormPortal's unpacker restores those modes, finds `Install formslot.command` one folder down, and Open runs it in a terminal. `zip` is asked for before the build starts, with the line that says how to install it, so a missing tool costs no compile. The `.tar.gz` stays for a download by hand. macOS is unchanged (its installable file is the disk image) and so is Windows.

The check in ci.yml that already opened the `.tar.gz` now also reads the zip's listing for the program and the installer, and checks the zip's `.sha256`.
