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
