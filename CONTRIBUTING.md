# Contributing to FormSlot

Thank you for helping. FormSlot is the Form family's codebase memory for coding agents: it indexes repositories at exact commits and serves symbols, slots and owner rules through a command line, an MCP server and Claude Code hooks. It gets better through real use and real contributions.

## Where to start

- **Use it and report what goes wrong.** A clear bug report, with the steps, the output of `formslot version` and `formslot doctor`, your system and the agent program you connected, is one of the most useful things you can give.
- **Look at the open issues.** Comment on one before starting larger work, so nobody builds the same thing twice.

## Setting up

You need git and Rust 1.90 or later. Clone the repository and run `./formslot_oc_install`, then `./formslot setup` (on Windows `formslot_oc_install.bat` and `formslot.bat setup`). `./formslot mcp` runs the MCP server on stdio. The README's "Run it from source" section and [`docs/GUIDE.md`](docs/GUIDE.md) have the details.

The code lives in:

| Folder | What is there |
| --- | --- |
| `core` | `formslot-core`, the engine: index, search, slots, memory, MCP and the local service |
| `cli` | `formslot-cli`, the `formslot` command |
| `plugin/formslot` | the Claude Code plugin: hooks, skills, the scout agent and the MCP entry |
| `data` | the proposed slots and the declined approaches |
| `docs` | the contract, the user guide, the slot schema and the evaluation reports |
| `setup` | the release and skill packaging scripts |

Read [`CLAUDE.md`](CLAUDE.md) and the `HASHOUT_LOG.md` beside the code you change: they hold the rules and the reasons behind the code.

## Before you open a pull request

Run the same checks the `ci` and `validate` workflows run:

```sh
cargo test --locked
setup/build_release
./formslot version
./formslot agents sync --check
for f in formslot formslot_oc_install "Install formslot.command" setup/build_release setup/package_skill; do bash -n "$f"; done
```

The workflows also build the Windows release with `setup\build_release.bat`, refuse an em dash in tracked Markdown or an emoji in any tracked text, and run `agentskills validate` (from `skills-ref`) on every skill folder and `claude plugin validate` on the plugin.

- Keep each pull request to one change, and say in it what you ran and on which system.
- Write commit subjects in Conventional Commits, saying what now happens: `feat(slots): ...`, `fix(mcp): ...`, `docs: ...`. Leave the version alone; a maintainer bumps it when releasing.
- An MCP tool added, removed or renamed needs `./formslot agents sync` and the tool count checked everywhere it is written.
- The service listens on `127.0.0.1` only and checks the access token, the Host and the Origin on every request. A secret lives in the system password store and never reaches another file, the log or an error.
- formslot never edits another repository's source; it writes only its own files and settings entries into a repository, and only when asked.
- Every new dependency, binary, model or data file gets a row in [`NOTICE.md`](NOTICE.md) before it is used.

## Releases

Only a maintainer makes a release. After the version is bumped on `main`, [FormCompiler](https://github.com/nodemode-dev/formcompiler) builds, signs and publishes every platform on this repository's releases page. No push from a contributor starts a release. [`CHANGELOG.md`](CHANGELOG.md) says what each version changed.

## Licensing

FormSlot is licensed under the GNU General Public License v3.0 only ([`LICENSE`](LICENSE), SPDX `GPL-3.0-only`). By contributing, you agree that your contribution is licensed under the same terms. Only add code you have the right to contribute, and name any third-party code and its licence in the pull request.

## Conduct

Everyone taking part follows the [code of conduct](CODE_OF_CONDUCT.md). Security problems go through [`SECURITY.md`](SECURITY.md), never a public issue.
