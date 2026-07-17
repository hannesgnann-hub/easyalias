# Architecture

This document describes the technical structure of the Windows version of EasyAlias.

## Overview

EasyAlias consists of a small frontend and a Tauri/Rust backend:

| Layer | File | Responsibility |
| --- | --- | --- |
| Frontend | `src/main.ts` | UI, form state, command preview |
| Styling | `src/styles.css` | layout and visual design |
| Backend | `src-tauri/src/main.rs` | local file read/write and PATH setup |
| Tauri Config | `src-tauri/tauri.conf.json` | app window, build, Windows installer |
| Tauri Dialog Plugin | `@tauri-apps/plugin-dialog` | native file/folder picker |
| Tauri Opener Plugin | `@tauri-apps/plugin-opener` | open GitHub and Reddit in the system browser |

The core idea: EasyAlias creates one `.cmd` file per alias and places those command files in a dedicated folder that is added to the user's `PATH`.

This matches the classic Windows shortcut pattern: a command name is just an executable file name that Windows can discover through `PATH`.

```mermaid
flowchart TB
  UI["Frontend src/main.ts"]
  CSS["Styling src/styles.css"]
  Tauri["Tauri Runtime"]
  Rust["Rust Backend src-tauri/src/main.rs"]
  Dialog["Dialog Plugin file/folder picker"]
  Opener["Opener Plugin GitHub link"]
  Files["~/.easyalias files"]
  Bin["~/.easyalias/bin/*.cmd"]
  Path["User PATH setup"]

  UI --> CSS
  UI --> Tauri
  Tauri --> Rust
  Tauri --> Dialog
  Tauri --> Opener
  Rust --> Files
  Rust --> Bin
  Rust --> Path
```

## Data Flow

```text
UI form
  -> AliasEntry
  -> ~/.easyalias/config.json
  -> ~/.easyalias/bin/name.cmd
  -> user PATH contains ~/.easyalias/bin
  -> new cmd.exe sessions
```

```mermaid
flowchart LR
  Form["UI form"]
  Entry["AliasEntry"]
  Config["config.json"]
  CmdFile["name.cmd"]
  Path["User PATH"]
  Terminal["cmd.exe session"]

  Form --> Entry
  Entry --> Config
  Entry --> CmdFile
  CmdFile --> Path
  Path --> Terminal
```

In browser preview mode without Tauri, state is stored only in `localStorage`. This makes the UI easy to test without changing real shell files.

In Tauri mode, the backend writes real files on Windows.

```mermaid
flowchart TD
  Start["App starts"]
  Runtime{"Tauri runtime?"}
  Browser["Browser preview"]
  Native["Native Tauri app"]
  LocalStorage["localStorage"]
  Backend["Rust commands"]
  RealFiles["Real files"]

  Start --> Runtime
  Runtime -- "no" --> Browser
  Browser --> LocalStorage
  Runtime -- "yes" --> Native
  Native --> Backend
  Backend --> RealFiles
```

## Local Files

| File | Content | Owner |
| --- | --- | --- |
| `~/.easyalias/config.json` | structured shortcut data for the UI | EasyAlias |
| `~/.easyalias/bin/*.cmd` | generated command files | EasyAlias |
| User `PATH` | contains `~/.easyalias/bin` | user + EasyAlias setup |

On first Tauri startup, the backend ensures:

1. `~/.easyalias/` exists.
2. `~/.easyalias/bin/` exists.
3. The user `PATH` contains the command folder.
4. `easya.cmd` exists when it does not conflict with a user alias.

```mermaid
sequenceDiagram
  participant UI as Frontend
  participant Rust as Rust Backend
  participant Dir as ~/.easyalias/
  participant Bin as ~/.easyalias/bin/
  participant Path as User PATH

  UI->>Rust: load_aliases()
  Rust->>Dir: create_dir_all()
  Rust->>Bin: create_dir_all()
  Rust->>Path: check command folder
  Rust->>Path: append folder if missing
  Rust->>Bin: write easya.cmd if safe
  Rust-->>UI: AppState + aliases
```

## Frontend

The frontend is intentionally lightweight:

- no UI framework
- TypeScript
- Vite
- direct DOM updates

Main responsibilities:

- manage form values
- validate shortcut names
- update the cmd command preview live
- persist optional Windows shortcut suggestions with one click
- display, edit, and delete shortcuts
- call Tauri commands when the app runs natively

The most important types:

```ts
type AliasAction =
  | "navigate"
  | "open"
  | "execute"
  | "compile_gradle"
  | "compile_maven"
  | "custom";

type AliasEntry = {
  id: string;
  name: string;
  path: string;
  action: AliasAction;
  customCommand?: string;
  commandPreview: string;
  createdAt: string;
  updatedAt: string;
};
```

```mermaid
stateDiagram-v2
  [*] --> EmptyForm
  EmptyForm --> EditingCreateForm: user types
  EditingCreateForm --> PreviewUpdated: path/action changes
  PreviewUpdated --> EditingCreateForm: continue typing
  EditingCreateForm --> ValidateCreate: submit
  ValidateCreate --> SaveAliases: valid
  ValidateCreate --> ShowError: invalid
  ShowError --> EditingCreateForm
  SaveAliases --> EmptyForm

  [*] --> ListReady
  ListReady --> EditModalOpen: click Edit
  EditModalOpen --> PreviewUpdatedInModal: edit fields
  PreviewUpdatedInModal --> EditModalOpen
  EditModalOpen --> SaveAliases: submit valid edit
  EditModalOpen --> ListReady: cancel
```

## Backend

The Tauri backend currently exposes two commands:

```rust
load_aliases()
save_aliases(aliases)
```

`load_aliases` handles startup setup:

- create the app directory
- create the command directory
- ensure the command directory is in the user `PATH`
- write `easya.cmd` when it does not conflict with an alias
- load `config.json` if it exists
- regenerate command files from saved aliases
- migrate older PowerShell-style command previews to cmd-style previews

`save_aliases` writes:

- `config.json` as the data source for the UI
- one `.cmd` file per alias
- removes stale `.cmd` files for deleted aliases
- returns fresh PATH status for the UI

```mermaid
sequenceDiagram
  participant UI as Frontend
  participant Rust as Rust Backend
  participant Config as config.json
  participant Bin as ~/.easyalias/bin/

  UI->>UI: create/edit/delete AliasEntry
  UI->>Rust: save_aliases(aliases)
  Rust->>Rust: validate shortcut names
  Rust->>Config: write pretty JSON
  Rust->>Bin: remove stale .cmd files
  Rust->>Bin: write generated .cmd files
  Rust-->>UI: updated AppState
```

## Command Generation

An alias entry becomes a small `.cmd` file:

```cmd
@echo off
cd /d "%USERPROFILE%\Desktop\projects\beerv2_app"
```

The frontend and backend both know how to derive this command from structured fields. The backend is authoritative and rewrites `commandPreview` on load/save, so older configs from the first PowerShell-based Windows prototype are automatically normalized.

Before writing, the backend validates:

- shortcut name is not empty
- shortcut name starts with a letter or `_`
- shortcut name contains only letters, numbers, `_`, or `-`
- command preview is not empty

```mermaid
flowchart TD
  AliasEntry["AliasEntry"]
  ValidateName{"Name valid?"}
  ValidateCommand{"Command present?"}
  CmdFile["name.cmd"]
  Error["Error shown in UI"]

  AliasEntry --> ValidateName
  ValidateName -- "no" --> Error
  ValidateName -- "yes" --> ValidateCommand
  ValidateCommand -- "no" --> Error
  ValidateCommand -- "yes" --> CmdFile
```

## Safety

EasyAlias changes the user `PATH` only by appending the command folder when it is missing:

```text
%USERPROFILE%\.easyalias\bin
```

Existing PATH entries are preserved.

The backend checks the persisted user PATH through `HKCU\Environment`. When it needs to add the command folder, it uses `setx` for normal-sized PATH values and falls back to `reg add` for long values to avoid `setx` truncation.

Important boundaries:

- Custom commands are real `cmd.exe` / batch commands.
- The generated `.cmd` files are app output and should not be edited manually.
- Standard paths are wrapped in double quotes.
- Folder-changing aliases persist in `cmd.exe`; from PowerShell they run as external commands and cannot change the parent PowerShell location.

## Runtime Notes

After EasyAlias updates User PATH, already-open terminals may still have the old environment. The expected user flow is:

1. Start EasyAlias once.
2. Let it add `~/.easyalias/bin` to User PATH.
3. Open a new `cmd.exe` window.
4. Run `where <alias>` to confirm resolution.

```cmd
where beerv2
```

The generated command files are intentionally human-readable:

```cmd
type "%USERPROFILE%\.easyalias\bin\beerv2.cmd"
```

## Roadmap

Short term:

- import existing `.cmd` shortcuts
- tests for command generation

Later:

- settings window
- polished app icon
- Windows installer
- optional export/backup mechanism
