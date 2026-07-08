# Architecture

This document describes the technical structure of the Windows version of EasyAlias.

## Overview

EasyAlias consists of a small frontend and a Tauri/Rust backend:

| Layer | File | Responsibility |
| --- | --- | --- |
| Frontend | `src/main.ts` | UI, form state, command preview |
| Styling | `src/styles.css` | layout and visual design |
| Backend | `src-tauri/src/main.rs` | local file read/write logic |
| Tauri Config | `src-tauri/tauri.conf.json` | app window, build, Windows installer |
| Tauri Dialog Plugin | `@tauri-apps/plugin-dialog` | native file/folder picker |
| Tauri Opener Plugin | `@tauri-apps/plugin-opener` | open GitHub in the system browser |

The core idea: EasyAlias does not manage the entire PowerShell profile. It creates a dedicated `aliases.ps1` file and dot-sources it from the profile once.

```mermaid
flowchart TB
  UI["Frontend src/main.ts"]
  CSS["Styling src/styles.css"]
  Tauri["Tauri Runtime"]
  Rust["Rust Backend src-tauri/src/main.rs"]
  Dialog["Dialog Plugin file/folder picker"]
  Opener["Opener Plugin GitHub link"]
  Files["~/.easyalias files"]
  Profile["PowerShell profile setup"]

  UI --> CSS
  UI --> Tauri
  Tauri --> Rust
  Tauri --> Dialog
  Tauri --> Opener
  Rust --> Files
  Rust --> Profile
```

## Data Flow

```text
UI form
  -> AliasEntry
  -> ~/.easyalias/config.json
  -> ~/.easyalias/aliases.ps1
  -> dot-source line in the PowerShell profile
  -> new PowerShell sessions
```

```mermaid
flowchart LR
  Form["UI form"]
  Entry["AliasEntry"]
  Config["config.json"]
  Generated["aliases.ps1"]
  Source["dot-source line in PowerShell profile"]
  Terminal["New PowerShell session"]

  Form --> Entry
  Entry --> Config
  Entry --> Generated
  Generated --> Source
  Source --> Terminal
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
| `~/.easyalias/aliases.ps1` | generated PowerShell functions | EasyAlias |
| `~/Documents/PowerShell/Microsoft.PowerShell_profile.ps1` | PowerShell 7+ profile | user + EasyAlias setup |
| `~/Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1` | Windows PowerShell profile | user + EasyAlias setup |

On first Tauri startup, the backend ensures:

1. `~/.easyalias/` exists.
2. `~/.easyalias/aliases.ps1` exists.
3. Both common PowerShell profiles contain `. "$HOME\.easyalias\aliases.ps1"`.
4. Both profiles contain an `easya` function if `easya` does not already exist.

```mermaid
sequenceDiagram
  participant UI as Frontend
  participant Rust as Rust Backend
  participant Dir as ~/.easyalias/
  participant AliasFile as aliases.ps1
  participant PS7 as PowerShell profile
  participant WinPS as WindowsPowerShell profile

  UI->>Rust: load_aliases()
  Rust->>Dir: create_dir_all()
  Rust->>AliasFile: create if missing
  Rust->>PS7: check dot-source line
  Rust->>WinPS: check dot-source line
  Rust->>PS7: append EasyAlias lines if missing
  Rust->>WinPS: append EasyAlias lines if missing
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
- update the PowerShell command preview live
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
- create an empty `aliases.ps1` if missing
- ensure the dot-source line in both common PowerShell profiles
- ensure the `easya` app shortcut in both profiles
- load `config.json` if it exists

`save_aliases` writes:

- `config.json` as the data source for the UI
- `aliases.ps1` as the generated PowerShell file

```mermaid
sequenceDiagram
  participant UI as Frontend
  participant Rust as Rust Backend
  participant Config as config.json
  participant PS as aliases.ps1

  UI->>UI: create/edit/delete AliasEntry
  UI->>Rust: save_aliases(aliases)
  Rust->>Rust: validate shortcut names
  Rust->>Config: write pretty JSON
  Rust->>PS: write generated PowerShell functions
  Rust-->>UI: updated AppState
```

## Shell Generation

PowerShell aliases cannot directly represent complex commands like `Set-Location ...; mvn clean package`, so EasyAlias generates small functions instead.

An alias entry becomes a PowerShell function:

```powershell
# Generated by EasyAlias.
# Edit aliases in the app, not by hand.

function beerv2 { Set-Location "$HOME\Desktop\projekte\beerv2_app" }
```

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
  FunctionLine["function name { command }"]
  Error["Error shown in UI"]

  AliasEntry --> ValidateName
  ValidateName -- "no" --> Error
  ValidateName -- "yes" --> ValidateCommand
  ValidateCommand -- "no" --> Error
  ValidateCommand -- "yes" --> FunctionLine
```

## Safety

EasyAlias changes PowerShell profiles only minimally:

```powershell
# EasyAlias aliases
. "$HOME\.easyalias\aliases.ps1"

# EasyAlias app shortcut
function easya { Start-Process "$env:LOCALAPPDATA\Programs\EasyAlias\EasyAlias.exe" }
```

Existing content is preserved.

Important boundaries:

- Custom commands are real PowerShell commands.
- The generated `aliases.ps1` is app output and should not be edited manually.
- Standard paths are wrapped in double quotes.
- Existing functions from PowerShell profiles are not imported yet.

## Roadmap

Short term:

- import existing profile functions
- tests for command generation

Later:

- settings window
- polished app icon
- Windows installer
- optional export/backup mechanism
