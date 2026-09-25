# EasyAlias TUI (Windows)

EasyAlias for the terminal: manage command aliases and multi-step automations without leaving cmd, PowerShell or Windows Terminal. It is a standalone program. You don't need the desktop app, but if you have it, both work on the same data in `~/.easyalias`.

```
 EasyAlias    1 Aliases   2 Automations   3 Settings                        ? help
──────────────────────────────────────────────────────────────────────────────────
 ✓ ~/.easyalias/bin is on your PATH
 / search                                               f All aliases   3 aliases
╭ Aliases ───────────────────────────────────────────────────────────────────────╮
│     Name                Action           Command                               │
│▸  ★ proj                Go to Folder     cd /d "%USERPROFILE%\Projects\app"     │
│     gs                  Custom Command   git status --short --branch           │
│     ll                  Custom Command   ls -la                                │
```

## Install

Requires Rust (https://rustup.rs):

```powershell
cd windows_tui
cargo install --path .
easyalias-tui
```

Or build a release binary with `cargo build --release` (it lands in `target\release\easyalias-tui.exe`). Windows Terminal is recommended; the classic console works too.

## Features

Everything from the desktop app that makes sense in a terminal:

- **Aliases:** create, edit, favorite, search, filter (Favorites, Git, Docker, Navigation, Build), the six actions (Go to Folder, Open, Run, Gradle, Maven, Custom Command) with a live preview, and Tab completion for paths.
- **Suggestions:** the same catalog of ready-made aliases, added with one key.
- **Import:** finds simple one-command `.cmd`/`.bat` shortcut files in your own PATH folders and moves the ones you pick into EasyAlias, keeping a copy of each in a backup folder first. It is offered automatically on first start.
- **Backups:** JSON export and import for aliases and automations, in the same format as the desktop app.
- **Trash:** a 30-day trash for aliases and automations, with restore, delete forever, and empty.
- **Automations:** command and wait steps (cmd.exe syntax), background steps, one shared cmd.exe session per run, a live run view with per-step output, and Stop.
- **Organize:** favorites, groups, a group view, search, and filters (Background, Git, Docker, Build, per group).
- **Schedules:** fixed time, sunrise, or sunset, on chosen weekdays, through Windows Task Scheduler. They also run when the TUI is closed.
- **Settings:** theme (terminal colors, light, dark), alias suggestions on or off, and the sunrise/sunset region.
- **Help:** press `?` for the built-in tutorial and a full key reference.

Not included, because they only make sense for a windowed app: global hotkeys, the tray icon, and autostart. An automation's hotkey set in the desktop app is kept when you edit it in the TUI.

## Keys

| Where | Keys |
| --- | --- |
| Everywhere | `1` `2` `3` / Tab: switch tab · `?`: help · `q`: quit |
| Aliases | `n` new · Enter edit · Space favorite · `d` trash · `/` search · `f` filter · `s` suggestions · `i` import command files · `b`/`B` export/import backup · `t` trash |
| Automations | `n` new · `e` edit · Enter run · `o` last run · Space favorite · `g` group · `c` schedule · `d` trash · `/` · `f` · `b`/`B` · `t` |
| Automation editor | Ctrl+N add command · Ctrl+P add wait · Ctrl+T wait/background · Shift+↑↓ move step · Ctrl+X remove step · Ctrl+S save |
| Run view | `s` stop · ↑↓ pick step · PgUp/PgDn scroll · `r` run again · Esc hide |

## Data

| File | Shared with the desktop app |
| --- | --- |
| `~\.easyalias\config.json`, `bin\*.cmd` | yes |
| `~\.easyalias\automations.json`, `timed-automations.json`, trash files | yes |
| `~\.easyalias\sun-location.json` | yes |
| `~\.easyalias\tui-settings.json` | no (theme and suggestions of the TUI only) |

Every alias becomes a tiny `.cmd` file in `~\.easyalias\bin`, and the TUI adds that folder to your user PATH once, just like the app. Open a new terminal afterwards so it sees the new PATH. The TUI does not create the app's `easya.cmd` launcher (an existing one is left alone). Command names are case-insensitive on Windows, so `GS` and `gs` count as the same alias.

Schedules are Task Scheduler tasks under the same names the desktop app uses, so an automation has one schedule no matter which program saved it. The task starts `easyalias-tui.exe` from where it was installed; if you move the program, save the schedule again. Because the TUI is a console program, a console window may appear briefly while a scheduled automation runs.

## Development

```powershell
cargo test      # core logic plus end-to-end TUI tests (keys in, files and screens out);
                # the tests that run automations need a real cmd.exe and only run on Windows
cargo run
```

The data layer (`models`, `aliases`, `automations`, `session`, `timed`, `sun`, `schtasks`, …) is the desktop app's backend (`windows_src/src-tauri/src`) without Tauri.

`src/tui/` is the terminal interface and is **identical in `mac_tui`, `linux_tui` and `windows_tui`**. Everything that differs per platform lives in `src/platform.rs` (generated commands, import, processes, links), `src/help.rs` and `src/suggestions.rs`. A UI change is made once and copied to the other two folders.
