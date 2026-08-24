# Architecture

This document describes the technical structure of EasyAlias.

## Overview

EasyAlias consists of a small frontend and a Tauri/Rust backend:

| Layer | File | Responsibility |
| --- | --- | --- |
| Frontend | `src/main.ts` | UI, form state, favorites, paged suggestions, imports, portable backups, Trash, command preview |
| Styling | `src/styles.css` | layout and visual design |
| Backend | `src-tauri/src/main.rs` | `.zshrc` detection, backup, migration, and local file writes |
| Tauri Config | `src-tauri/tauri.conf.json` | app window, build, bundle |
| Tauri Dialog Plugin | `@tauri-apps/plugin-dialog` | native file/folder picker |
| Tauri Opener Plugin | `@tauri-apps/plugin-opener` | open GitHub and Reddit in the system browser |

The core idea: EasyAlias does not manage the entire `~/.zshrc`. It creates a dedicated alias file and connects it to zsh once.

```mermaid
flowchart TB
  UI["Frontend src/main.ts"]
  CSS["Styling src/styles.css"]
  Tauri["Tauri Runtime"]
  Rust["Rust Backend src-tauri/src/main.rs"]
  Dialog["Dialog Plugin file/folder picker"]
  Opener["Opener Plugin GitHub and Reddit links"]
  Files["~/.easyalias files"]
  Zshrc["~/.zshrc setup"]

  UI --> CSS
  UI --> Tauri
  Tauri --> Rust
  Tauri --> Dialog
  Tauri --> Opener
  Rust --> Files
  Rust --> Zshrc
```

## Data Flow

```text
UI form
  -> AliasEntry
  -> ~/.easyalias/config.json
  -> ~/.easyalias/aliases.zsh
  -> source line in ~/.zshrc
  -> new terminal sessions
```

```mermaid
flowchart LR
  Form["UI form"]
  Entry["AliasEntry"]
  Config["config.json"]
  Generated["aliases.zsh"]
  Source["source line in ~/.zshrc"]
  Terminal["New terminal session"]

  Form --> Entry
  Entry --> Config
  Entry --> Generated
  Generated --> Source
  Source --> Terminal
```

In browser preview mode without Tauri, state is stored only in `localStorage`. This makes the UI easy to test without changing real shell files.

In Tauri mode, the backend writes real files on the Mac.

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
| `~/.easyalias/config.json` | structured alias data for the UI | EasyAlias |
| `~/.easyalias/trash.json` | deleted aliases retained for up to 30 days | EasyAlias |
| `~/.easyalias/automations.json` | saved multi-step automations | EasyAlias |
| `~/.easyalias/aliases.zsh` | generated zsh aliases | EasyAlias |
| `~/.easyalias/.zshrc-import-v1` | records that the automatic first-start import prompt was handled | EasyAlias |
| `~/.zshrc.easyalias-backup-*` | timestamped copy created before an import | user backup |
| `~/.zshrc` | user configuration plus EasyAlias source/shortcut lines and confirmed import markers | user + EasyAlias setup |

On first Tauri startup, the backend ensures:

1. `~/.easyalias/` exists.
2. Existing safe one-line aliases are detected before EasyAlias appends its own lines.
3. `~/.easyalias/aliases.zsh` exists.
4. `~/.zshrc` contains `source ~/.easyalias/aliases.zsh`.
5. `~/.zshrc` contains `alias easya='open /Applications/EasyAlias.app'` if `easya` does not already exist.

```mermaid
sequenceDiagram
  participant UI as Frontend
  participant Rust as Rust Backend
  participant Dir as ~/.easyalias/
  participant AliasFile as aliases.zsh
  participant Zshrc as ~/.zshrc

  UI->>Rust: load_aliases()
  Rust->>Dir: create_dir_all()
  Rust->>Zshrc: scan simple alias lines as text
  Rust->>AliasFile: create if missing
  Rust->>Zshrc: check source line
  Rust->>Zshrc: append source if missing
  Rust->>Zshrc: append easya shortcut if missing
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
- validate alias names
- update the command preview live
- persist safe macOS suggestions directly with one click
- paginate the 31 built-in Git, Docker, build, and utility suggestions
- open the import scanner from the header and review `.zshrc` candidates
- sort favorites before regular aliases
- search aliases by name/command and filter by favorites, Git, Docker, navigation, or build
- selectively export and restore portable JSON backups
- restore or permanently remove aliases from the 30-day Trash
- display, edit, and move aliases to Trash
- build, save, and run multi-step Automations in a separate view
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
  favorite: boolean;
  createdAt: string;
  updatedAt: string;
};

type AutomationStep = {
  id: string;
  kind: "command" | "wait";
  command: string;
  seconds: number;
  behavior: "wait" | "background";
};

type Automation = {
  id: string;
  name: string;
  path: string;
  steps: AutomationStep[];
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

The Tauri backend exposes commands in four groups:

```rust
// Core persistence
load_aliases()
save_aliases(aliases)

// Recoverable deletion
list_trash()
move_alias_to_trash(id)
restore_trash_alias(id)
permanently_delete_trash_alias(id)
empty_trash()

// Portable JSON backups
export_alias_backup(selected_ids)
inspect_alias_backup(path)
import_alias_backup(path, selected_ids)

// Legacy .zshrc migration
scan_zshrc_import()
dismiss_zshrc_import()
import_zshrc_aliases(selected_ids, timestamp)

// Automations
load_automations()
save_automations(automations)
start_automation_session(session_id, path)
run_session_command(session_id, command, background)
stop_automation_session(session_id)
```

`load_aliases` handles startup setup:

- create the app directory
- create an empty `aliases.zsh` if missing
- ensure the `source` line in `~/.zshrc`
- ensure the `easya` shortcut in `~/.zshrc`
- load `config.json` if it exists

`save_aliases` writes:

- `config.json` as the data source for the UI
- `aliases.zsh` as the generated shell file

Deleting an alias calls `move_alias_to_trash`. The backend removes it from the active config and generated shell file, records its deletion time in `trash.json`, and purges entries older than 30 days whenever Trash is loaded. Restore regenerates the active files; permanent deletion and `empty_trash` cannot be undone.

Portable backup commands use a versioned EasyAlias JSON format. Export includes only selected aliases. Import first validates the file, rejects unsupported or oversized input, then lets the user choose which entries to merge. Matching alias names are replaced only when selected.

`scan_zshrc_import` ignores the first-start marker, scans `~/.zshrc` again, filters names already managed by EasyAlias, and returns the remaining candidates for the header import dialog. It does not modify alias lines.

`import_zshrc_aliases` rescans the file, verifies the selected line ids, creates a timestamped backup, writes imported Custom Commands, and replaces only confirmed source lines with zsh no-op markers. The scanner never sources or executes `~/.zshrc`.

```mermaid
sequenceDiagram
  participant User
  participant UI as Frontend
  participant Rust as Rust Backend
  participant Zshrc as ~/.zshrc
  participant Managed as ~/.easyalias files

  User->>UI: click import icon
  UI->>Rust: scan_zshrc_import()
  Rust->>Zshrc: parse safe aliases as text
  Rust-->>UI: unmanaged candidates
  User->>UI: confirm selected aliases
  UI->>Rust: import_zshrc_aliases(ids, timestamp)
  Rust->>Zshrc: create timestamped backup
  Rust->>Managed: write config.json and aliases.zsh
  Rust->>Zshrc: replace confirmed source lines
  Rust-->>UI: updated AppState
```

Automations are stored independently of aliases in `~/.easyalias/automations.json` and validated on every load and save: up to `MAX_AUTOMATIONS` (200) automations, each with 1-`MAX_AUTOMATION_STEPS` (100) steps, unique automation and step ids, command steps under `MAX_AUTOMATION_COMMAND_BYTES` (16 KB), and wait steps between 1 second and `MAX_WAIT_SECONDS` (24 hours).

Each run gets one persistent `/bin/zsh -l` process (an `AutomationSessionHandle`, keyed by a frontend-generated `session_id` in the `AutomationSessions` Tauri-managed state) instead of a fresh process per step. This is what lets `cd` and exported environment variables from one step carry over to the next, the same way they would in a real terminal.

- `start_automation_session` resolves the working directory (expanding a leading `~`, then `canonicalize`-ing and requiring it to exist), spawns the shell with piped stdin/stdout, and starts a background thread that streams output lines into an `mpsc` channel.
- `run_session_command` writes the step's command to the session's stdin followed by a unique echoed sentinel, then reads from the channel until that sentinel appears. Foreground commands are wrapped as `{ command ; } 2>&1` so stderr merges into the captured output, and the sentinel carries `$?`; background commands are wrapped as `{ command ; } >/dev/null 2>&1 &` and the sentinel carries `$!`, so the step returns as soon as the job has started rather than waiting for it to finish. Captured output is truncated to `MAX_AUTOMATION_OUTPUT_CHARS` (20,000 characters).
- `stop_automation_session` kills the session's shell process specifically (not its process group), so it can interrupt a stuck foreground command without touching background jobs that process already started with `&` — those keep running detached, reparented once the shell exits. It is called both when the user clicks Stop and automatically once a run finishes.

```mermaid
sequenceDiagram
  participant UI as Frontend
  participant Rust as Rust Backend
  participant Shell as Persistent /bin/zsh -l

  UI->>Rust: start_automation_session(sessionId, path)
  Rust->>Shell: spawn once in working directory
  loop each step
    UI->>Rust: run_session_command(sessionId, command, background)
    Rust->>Shell: write command + sentinel to stdin
    Shell-->>Rust: output lines until sentinel
    Rust-->>UI: AutomationCommandResult
    UI->>UI: mark step success/error, advance or stop
  end
  UI->>Rust: stop_automation_session(sessionId)
  Rust->>Shell: kill (background jobs already started keep running)
```

```mermaid
sequenceDiagram
  participant UI as Frontend
  participant Rust as Rust Backend
  participant Config as config.json
  participant Zsh as aliases.zsh

  UI->>UI: create, edit, favorite, or use suggestion
  UI->>Rust: save_aliases(aliases)
  Rust->>Rust: validate alias names
  Rust->>Config: write pretty JSON
  Rust->>Zsh: write generated zsh aliases
  Rust-->>UI: updated AppState
```

```mermaid
flowchart LR
  Delete["Delete alias"] --> Trash["trash.json"]
  Trash --> Restore["Restore to active aliases"]
  Trash --> Permanent["Delete permanently"]
  Trash --> Expire["Automatic purge after 30 days"]
  Restore --> Config["config.json + aliases.zsh"]
```

## Shell Generation

An alias entry becomes a zsh line:

```zsh
# Generated by EasyAlias.
# Edit aliases in the app, not by hand.

alias beerv2='cd "$HOME/Desktop/projects/beerv2_app"'
```

Before writing, the backend validates:

- alias name is not empty
- alias name starts with a letter or `_`
- alias name contains only letters, numbers, `_`, or `-`
- command preview is not empty

```mermaid
flowchart TD
  AliasEntry["AliasEntry"]
  ValidateName{"Name valid?"}
  ValidateCommand{"Command present?"}
  Quote["Escape command for single quotes"]
  Line["alias name='command'"]
  Error["Error shown in UI"]

  AliasEntry --> ValidateName
  ValidateName -- "no" --> Error
  ValidateName -- "yes" --> ValidateCommand
  ValidateCommand -- "no" --> Error
  ValidateCommand -- "yes" --> Quote
  Quote --> Line
```

## Safety

EasyAlias changes `~/.zshrc` only minimally:

```zsh
# EasyAlias aliases
source ~/.easyalias/aliases.zsh

# EasyAlias app shortcut
alias easya='open /Applications/EasyAlias.app'
```

Existing content is preserved.

Important boundaries:

- Custom commands are real shell commands.
- The generated `aliases.zsh` is app output and should not be edited manually.
- Standard paths are wrapped in double quotes.
- Import scanning handles only unindented, one-line aliases with one assignment.
- Alias options, nested declarations, repeated names, malformed lines, and multiple assignments are skipped.
- A backup is written before any selected source line is changed.
- Portable backup files are parsed as data and never executed.
- Trash provides a 30-day recovery window unless the user explicitly deletes an entry permanently or empties it.
- Automation commands run only when the user explicitly clicks Run, execute in the automation's own shell session, and are rejected outright in browser preview mode (no `start_automation_session`/`run_session_command` backend to call).

## Roadmap

Short term:

- tests for command generation

Later:

- settings window
- signed and notarized release automation
- port Automations to Linux (Windows already has it; the sandboxed Mac App Store edition is intentionally excluded, see its architecture doc)
