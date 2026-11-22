# formslot user guide

## What formslot does for you

formslot keeps a small, private memory of your projects on your own computer: the code that already exists, where it lives and the rules you have set for it. When you work in Claude Code it quietly tells Claude about what already exists, so Claude reuses it instead of writing it again. It also stops Claude from rebuilding things you have said you do not want.

Nothing leaves your computer unless you run one of three commands yourself. None of them runs on its own, and setup runs none of them:

- `formslot pack "<task>" --count remote` sends the whole pack text (your task, the code excerpts and the rules in it) and your Anthropic key to `api.anthropic.com`, which answers with a token count. Without `--count remote`, a pack is counted on your computer and nothing is sent.
- `formslot dense fetch --yes` downloads a search model (about 154 MB) from `huggingface.co`. It sends nothing of yours; like any download, that site sees your network address. It exists only in a build made with the optional dense feature, which the released program is not.
- `formslot eval --arm model` runs the `claude` program on your Claude Code login. The evaluation tasks, the packs built for them and whatever Claude reads from a read-only copy of your project go to Anthropic through Claude Code, and the run costs money on that account.

One more thing to know: what formslot tells Claude Code (the map at the start of a session, a reuse note, a pack) becomes part of your Claude Code conversation, and that conversation goes to Anthropic the way every Claude Code conversation does. formslot has no account, no telemetry and no update check.

## Install

You need [git](https://git-scm.com) and [Claude Code](https://claude.com/product/claude-code). To build formslot from source you also need [Rust](https://rustup.rs). formslot never installs programs for you; it tells you the exact command for anything missing.

Pick one way:

- **macOS:** open the disk image you downloaded and double-click `Install formslot.command` in it. In a folder built from source, double-click the same file there. The released disk image is signed and checked by Apple before it is published. If macOS still refuses to open the file, right-click it, choose Open, then choose Open again in the question that appears. You only do this once.
- **Windows:** double-click `Install formslot.bat`. Windows SmartScreen may show a blue "Windows protected your PC" box. Click "More info", then "Run anyway". This appears because the file is new to Windows, not because anything is wrong.
- **One command, any system:** open a terminal in the formslot folder and run `./formslot_oc_install` (on Windows `formslot_oc_install.bat`). When it finishes it offers to start the guided setup. If formslot is already built, `./formslot setup` does the same.

The guided setup walks you through six short steps and explains each one. Press Enter to accept the answer shown in [brackets]. Press Ctrl+C at any time to leave; nothing is left half-done. Running it again is safe.

## Your first project

During setup, when it asks which folders hold your projects, drag a project folder into the window and press Enter. The folder must be a git repository with at least one commit. formslot reads it and connects Claude Code to it.

Then do the one step only you can do: open a terminal in that project, run `claude`, and approve the formslot server when Claude Code asks. Until you approve it, Claude Code will not use formslot there.

To add another project later, run `formslot add /path/to/project`. To connect it to Claude Code, run `formslot install /path/to/project`.

### If you use Codex, Cursor or Antigravity

These three can use formslot's lookup tools too. In a terminal, run the line for the program you use, with your project's folder:

- Codex: `formslot connect codex /path/to/project`
- Cursor: `formslot connect cursor /path/to/project`
- Antigravity: `formslot connect antigravity /path/to/project`

Each line adds formslot to that program's own settings file inside the project and changes nothing else in it. Add `--user` instead of a folder to connect every project at once. To undo it, run the same line with `--remove`. The program reads the file the next time it starts and may ask you to approve formslot once.

What these three do not get: the quiet reminders described below. Those come from Claude Code's hooks, and only Claude Code has them. In Codex, Cursor and Antigravity, formslot answers when the assistant asks it.

## What you will see in Claude Code

formslot speaks up only when it matters. Three things you may notice:

**The reuse warning.** Before Claude writes code, formslot checks whether something like it already exists. If so, Claude is told, and you may see a note like this:

```
formslot: ui.control-rows already covers a function named buildControlRows
(src/ui/control-rows.ts, line 42). Consider reusing it.
```

The write is not blocked. Claude usually switches to the existing code. The same note is not repeated for an identical write, but appears again if the content changes.

**A decline.** If you have recorded that a feature should not be built, formslot stops a write that would bring it back and says why:

```
formslot: this project declines "undo history for the canvas"
(owner rule, CLAUDE.md line 31). Ask the owner before building it.
```

This one does block the write, and quotes the rule so you can see where it came from.

**The slot card (in Substrate).** If you use Substrate (FormSubstrate), its right panel has a Slots tab. When you open a file it shows which reusable slots or known code units cover that file, for example "ui.control-rows" with its status. A file nothing covers shows "No slot or unit covers this file".

## Everyday commands

```bash
formslot status            # is the service running, how healthy, how long idle
formslot doctor            # check git, the data folder, the service, the store and the hooks
formslot add <path>        # index a project (use --name to choose its name)
formslot reindex <name>    # read a project again after it changed
formslot search "colour row" --corpus <name>   # find code, slots and rules
formslot pack "add a section to the dialog" --corpus <name> --tokens 4000   # a short briefing for a task
formslot slot list         # the reusable components formslot knows
```

`formslot corpus list` shows your projects with their folder. `formslot help` lists everything. The service starts on demand and stops by itself after ten idle minutes; `formslot start` and `formslot stop` do it by hand.

## Keeping it up to date

After you get a new version of formslot, rebuild or run the installer again so the program in your home folder is replaced (setup stops and restarts a running service for you). Then read each project again and refresh its rules:

```bash
formslot reindex <name>
formslot memory ingest <name>
formslot search rebuild
```

You can also run `formslot reindex <name>` any time your project has new commits; only files not seen before are read.

## Renaming or moving a project folder

If you only want a different name inside formslot:

```bash
formslot corpus rename old-name new-name
```

Everything is kept: the index, rules and slot copies. A name is 1 to 64 letters, digits, `.`, `_` or `-`.

If you moved the folder on disk, point formslot at the new place, then read it again:

```bash
formslot corpus move <name> /new/path/to/project
formslot reindex <name>
```

The new folder must be a checkout of the same repository and contain the last indexed commit.

## Undoing everything

```bash
formslot setup --status    # see what setup put on this computer, changes nothing
formslot setup --remove    # guided undo, each step asked and skippable
```

`--remove` disconnects your projects from Claude Code (only formslot's own entries; other settings are untouched and backups are saved as `<file>.formslot-backup`), stops the service, removes the PATH lines setup added, removes the program, and last asks whether to delete formslot's data folder. That last question defaults to No. For scripts, `formslot setup --remove --yes` never deletes your data; add `--purge` to delete it too. Add `--dry-run` to only see what would happen.

## Troubleshooting

**Claude Code does not use formslot ("MCP not approved").** In that project, run `claude` and approve the formslot server when asked. `formslot setup --status` shows whether the project is connected; `formslot install <path> --enable` connects it again and pre-approves the server.

**"formslot: command not found" (not found on PATH).** Open a new terminal window first; the PATH line setup added only applies to new windows. If it still fails, run `formslot setup --status` to see which shell file has the line, and `formslot doctor` to see what Claude Code's hooks can find.

**Windows PATH.** Setup adds `%LOCALAPPDATA%\Programs\formslot` to your user PATH. Close and reopen the terminal, or sign out and in, for it to count. `formslot setup --remove` takes only that entry out again and saves the old value in a file in your home folder.

**The service is not running.** Run `formslot status`, then `formslot start`. `formslot doctor` explains any problem it finds and exits with 1 when something real is wrong.

**The port is in use.** formslot uses port 5645. If another program has it, choose another before starting: `FORMSLOT_PORT=5700 formslot start` (in PowerShell: `$env:FORMSLOT_PORT = 5700; formslot start`). `FORMSLOT_PORT=0` lets the system pick a free port. Use the same value for later commands.

## Where your data lives

Everything is in `~/.formslot` (on Windows, `.formslot` in your user folder). It holds the index database, the service file, the log, and small state files for the hooks. Set `FORMSLOT_DATA_DIR` to use another folder. formslot never changes your project's source; inside a project it writes only `.claude/settings.json` (merged, other keys kept), `.mcp.json`, the two skills under `.claude/skills/` (`formslot-lookup` and `formslot-curation`), the scout agent at `.claude/agents/formslot-scout.md`, the one settings file of Codex, Cursor or Antigravity when you run `formslot connect`, and `slots.lock.json` when you apply slots. If you change the skill or the agent by hand, formslot leaves your version alone from then on.
