# Seeded slots

The 26 first-load manifests in this folder. All are status proposed, version 0.1.0, origin formvis 1.98.4 at commit 8fe4ce45bcfa1b85c509db4bb34da29942db964a.

| id | title | kind | status | B06 row | one line |
| --- | --- | --- | --- | --- | --- |
| tokens.studio-dark | Studio Dark tokens | token-set | proposed | 1 | 150 design tokens, one z scale, names equal NodeMode |
| theme.profiles-14 | Theme profiles and accent | token-set | proposed | 2 | THEMES table, readChoice/applyTheme, accent, user skins; key nodemode_ui_settings |
| theme.prepaint-boot | Pre-paint theme boot | pattern | proposed | 3 | Inline head script that paints the saved theme before first paint |
| css.shell | Window shell stylesheet | style | proposed | 4 | Title bar, rail, tabs, options row, status and home stylesheet |
| css.panels-dialogs | Panels, dialogs and light layer | style | proposed | 5 | Panel, dock, menu, dialog and toggle styles, no-scrollbar rule, fonts, light layer |
| ui.dom-helpers | DOM helpers (no-flash) | module | proposed | 7 | el, mountOnce, patchList and guarded set* helpers (zero DOM flash) |
| ui.icons | Inline SVG icon module | module | proposed | 8 | 239 inline currentColor SVG icons and icon(name, size) |
| ui.menu-glass | Glass menus and native menu bridge | component | proposed | 9 | PopupMenu, MenuBar and the macOS native menu bridge over command ids |
| ui.dropdown | Glass dropdown for selects | component | proposed | 10 | enhanceSelect: glass list with search and hover preview over a native select |
| ui.toast | Toast notices | component | proposed | 11 | toast(text, kind, opts) with actions and a notice history |
| ui.dialog-kit | Dialog kit | component | proposed | 12 | Dialog class, remember(), confirmAction, promptName and field undo |
| ui.control-rows | Control rows | component | proposed | 13 | sliderRow, selectRow, checkRow, toggleRow, colorRow, numberRow; canonical home of the colour-row twins |
| ui.number-fields | Number fields and sticky modifiers | component | proposed | 14 | Arithmetic and scrub on every number input; sticky modifier keys |
| ui.panel-group-dock | Panel groups, dock and tab strip | component | proposed | 15 | PanelGroup, Dock (reorder, float, remember) and the document tab strip |
| ui.prefs-store | Preferences store and option memory | module | proposed | 19 | Validated localStorage preferences, per-dialog memory and tool option memory |
| ui.prefs-dialog | Preferences dialog shell | component | proposed | S7 | SettingsDialog: rail of section tabs, foldable cards, scroll spy; add a section by registering a name and rows |
| ui.terminal-dock | Terminal dock | component | proposed | 21 | Bottom dock of PTY terminals with Claude Code tabs, plus engine terminal service |
| ui.chat-pane | Chat pane (Claude Code stream) | component | proposed | 22 | Dock Chat tab streaming Claude Code with permission cards |
| svc.claude-code-agents | Claude Code launch and Agent Connect | service | proposed | 23 | Claude Code launch, Agent Connect policies and approvals, skills sync with drift check |
| svc.command-catalogue | Command catalogue | service | proposed | 24 | One command catalogue for the palette, MCP command tools and the CLI |
| svc.mcp-server | MCP server | service | proposed | 25 | Streamable-HTTP MCP server with tool schemas, confinement, approval and a stdio launcher |
| svc.agent-bridge | Agent bridge and window control | service | proposed | 26 | Window side of the MCP bridge, UiControl window driving and a script sandbox |
| svc.model-manager | Model manager and durable jobs | service | proposed | 29 | SHA-256 pinned model downloads, job queue, atomic JSON stores |
| tauri.desktop-shell | Tauri desktop shell | tauri-command | proposed | 34 | Tauri 2 sidecar shell with 26 commands: updates, fonts, window, native canvas |
| script.counts-check | Counts and rules check | module | proposed | 36 | counts --check gate: README sizes, scrollbars, declined features, sparkle ban |
| rule.family-laws | Family laws | pattern | proposed | 37 | The laws every form app obeys, with the declined features list |
