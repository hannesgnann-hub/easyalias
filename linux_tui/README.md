# EasyAlias TUI (Linux)

EasyAlias for the terminal: manage shell aliases and multi-step automations without leaving the shell. It is a standalone program. You don't need the desktop app, but if you have it, both work on the same data in `~/.easyalias`.

```
 EasyAlias    1 Aliases   2 Automations   3 Settings                        ? help
──────────────────────────────────────────────────────────────────────────────────
 ✓ bash connected · ~/.bashrc
 / search                                               f All aliases   3 aliases
╭ Aliases ───────────────────────────────────────────────────────────────────────╮
│     Name                Action           Command                               │
│▸  ★ proj                Go to Folder     cd "$HOME/Projects/app"               │
│     gs                  Custom Command   git status --short --branch           │
│     ll                  Custom Command   ls -la                                │
```

## Install

Requires Rust (https://rustup.rs):

```bash
cd linux_tui
cargo install --path .
easyalias-tui
```

Or build a release binary with `cargo build --release` (it lands in `target/release/easyalias-tui`).

## Features

Everything from the desktop app that makes sense in a terminal:

- **Aliases:** create, edit, favorite, search, filter (Favorites, Git, Docker, Navigation, Build), the six actions (Go to Folder, Open, Run, Gradle, Maven, Custom Command) with a live preview, and Tab completion for paths.
- **Suggestions:** the same catalog of ready-made aliases, added with one key.
- **Import:** reads the aliases you already have in your shell startup file (`~/.bashrc`, or `~/.zshrc` if zsh is your login shell) and writes a backup of it first. It is offered automatically on first start.
- **Backups:** JSON export and import for aliases and automations, in the same format as the desktop app.
- **Trash:** a 30-day trash for aliases and automations, with restore, delete forever, and empty.
- **Automations:** command and wait steps, background steps, one shared shell session per run, a live run view with per-step output, and Stop.
- **Organize:** favorites, groups, a group view, search, and filters (Background, Git, Docker, Build, per group).
- **Schedules:** fixed time, sunrise, or sunset, on chosen weekdays, through systemd user timers. They also run when the TUI is closed.
- **Settings:** theme (terminal colors, light, dark), alias suggestions on or off, and the sunrise/sunset region.
- **Help:** press `?` for the built-in tutorial and a full key reference.

Not included, because they only make sense for a windowed app: global hotkeys, the tray icon, and autostart. An automation's hotkey set in the desktop app is kept when you edit it in the TUI.

## Keys

| Where | Keys |
| --- | --- |
| Everywhere | `1` `2` `3` / Tab: switch tab · `?`: help · `q`: quit |
| Aliases | `n` new · Enter edit · Space favorite · `d` trash · `/` search · `f` filter · `s` suggestions · `i` import from shell · `b`/`B` export/import backup · `t` trash |
| Automations | `n` new · `e` edit · Enter run · `o` last run · Space favorite · `g` group · `c` schedule · `d` trash · `/` · `f` · `b`/`B` · `t` |
| Automation editor | Ctrl+N add command · Ctrl+P add wait · Ctrl+T wait/background · Shift+↑↓ move step · Ctrl+X remove step · Ctrl+S save |
| Run view | `s` stop · ↑↓ pick step · PgUp/PgDn scroll · `r` run again · Esc hide |

## Data

| File | Shared with the desktop app |
| --- | --- |
| `~/.easyalias/config.json`, `aliases.sh` | yes |
| `~/.easyalias/automations.json`, `timed-automations.json`, trash files | yes |
| `~/.easyalias/sun-location.json` | yes |
| `~/.easyalias/tui-settings.json` | no (theme and suggestions of the TUI only) |

The TUI adds `source ~/.easyalias/aliases.sh` to the startup file of your login shell once (bash → `~/.bashrc`, zsh → `~/.zshrc`), just like the app. It does not add the app's `easya` launcher alias. Automations run in that same shell.

Schedules are systemd user timers in `~/.config/systemd/user`, under the same unit names the desktop app uses, so an automation has one schedule no matter which program saved it. They need a systemd user session; to let them run while you are logged out, enable lingering once with `loginctl enable-linger`. When the TUI runs from Homebrew, it registers the stable `bin/` path rather than the versioned Cellar path, so schedules keep working after `brew upgrade`.

Opening links uses `xdg-open`.

## Development

```bash
cargo test      # core logic plus end-to-end TUI tests (keys in, files and screens out)
cargo run
```

The data layer (`models`, `aliases`, `automations`, `session`, `timed`, `sun`, `systemd`, …) is the desktop app's backend (`linux_src/src-tauri/src`) without Tauri.

`src/tui/` is the terminal interface and is **identical in `mac_tui`, `linux_tui` and `windows_tui`**. Everything that differs per platform lives in `src/platform.rs` (generated commands, import, processes, links), `src/help.rs` and `src/suggestions.rs`. A UI change is made once and copied to the other two folders.
