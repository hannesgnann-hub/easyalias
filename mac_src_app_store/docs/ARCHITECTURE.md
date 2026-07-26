# Mac App Store Architecture

This document describes the sandboxed EasyAlias macOS edition in `mac_src_app_store/`.

## Design Goal

The Homebrew edition can access the user's home directory directly. The Mac App Store edition cannot. It must run inside App Sandbox and receive explicit permission for every external file it manages.

The Store edition therefore separates data into two ownership domains:

| Domain | Data |
| --- | --- |
| EasyAlias App Sandbox container | structured alias JSON, bookmark data, import marker, backups |
| user-selected file | one `.zshrc` containing the EasyAlias managed block |

```mermaid
flowchart LR
  UI["TypeScript UI"] --> Tauri["Tauri IPC"]
  Tauri --> Rust["Rust backend"]
  Rust --> Container["App Sandbox container"]
  Rust --> Bookmark["Security-scoped bookmark"]
  Bookmark --> Zshrc["User-selected .zshrc"]
  Zshrc --> Terminal["New zsh sessions"]
```

## Components

| Layer | File | Responsibility |
| --- | --- | --- |
| Frontend | `src/main.ts` | forms, suggestions, setup flow, import review, status |
| Styling | `src/styles.css` | responsive desktop UI |
| Backend | `src-tauri/src/main.rs` | container storage, bookmark lifecycle, parsing, backups, managed block |
| Base config | `src-tauri/tauri.conf.json` | identity, version, window, category |
| Sandbox config | `src-tauri/tauri.sandbox.conf.json` | local sandbox bundle |
| Store config | `src-tauri/tauri.appstore.conf.json` | final provisioning-profile bundle |
| Local entitlements | `src-tauri/Entitlements.local.plist` | ad-hoc sandbox permissions without Store identity fields |
| Store entitlements | `src-tauri/Entitlements.plist` | sandbox permissions plus Team ID and Application ID |
| Info plist | `src-tauri/Info.plist` | export-compliance declaration |

## Connection Flow

The frontend uses Tauri's dialog plugin, which opens a native `NSOpenPanel`. The user selects `.zshrc`, and the selected path is immediately sent to `connect_zshrc`.

```mermaid
sequenceDiagram
  participant User
  participant UI as TypeScript UI
  participant Panel as NSOpenPanel
  participant Rust as Rust backend
  participant Container as App container
  participant Zshrc as Selected .zshrc

  User->>UI: Choose .zshrc
  UI->>Panel: open native picker
  Panel-->>UI: selected path + temporary access
  UI->>Rust: connect_zshrc(path)
  Rust->>Rust: validate filename and read/write access
  Rust->>Rust: create security-scoped bookmark
  Rust->>Container: save zshrc.bookmark
  Rust->>Zshrc: start security-scoped access
  Rust->>Container: create backup
  Rust->>Zshrc: create managed block
  Rust->>Zshrc: stop security-scoped access
  Rust-->>UI: connected AppState
```

The temporary permission from the picker is converted into bookmark data before the panel access ends.

## Bookmark Lifecycle

Every later `.zshrc` operation follows the same boundary:

1. Read `zshrc.bookmark` from the app container.
2. Resolve it with `NSURLBookmarkResolutionWithSecurityScope`.
3. Refresh stale bookmark data if macOS requests it.
4. Call `startAccessingSecurityScopedResource()`.
5. Perform one bounded read/write operation.
6. Call `stopAccessingSecurityScopedResource()`.

```mermaid
stateDiagram-v2
  [*] --> LoadBookmark
  LoadBookmark --> Resolve
  Resolve --> ReconnectRequired: invalid
  Resolve --> Refresh: stale
  Resolve --> StartAccess: current
  Refresh --> StartAccess
  StartAccess --> Operation: granted
  StartAccess --> ReconnectRequired: denied
  Operation --> StopAccess
  StopAccess --> [*]
```

The resolved path is never treated as permanent authority by itself. The bookmark is the authority.

## Managed Block

Aliases are rendered directly into the selected `.zshrc`:

```zsh
# >>> EasyAlias managed aliases >>>
# Managed by EasyAlias. Edit these aliases in the app.
alias gs='git status --short --branch'
# <<< EasyAlias managed aliases <<<
```

Before writing, the backend:

- validates every alias name
- rejects empty commands
- escapes commands for a single-quoted zsh alias
- validates that markers are paired
- rejects duplicate or ambiguous blocks
- removes only the previous managed block
- appends one newly rendered block

This avoids asking Terminal to read generated scripts from another app's sandbox container.

## Commands

The backend exposes seven Tauri commands:

```rust
load_aliases()
connect_zshrc(path)
disconnect_zshrc()
save_aliases(aliases)
scan_zshrc_import()
dismiss_zshrc_import()
import_zshrc_aliases(selected_ids, timestamp)
```

### `load_aliases`

- creates the app container data and backup directories
- loads `config.json`
- resolves the bookmark if one exists
- reports connection and managed-block status
- offers a first import only after a valid connection

It does not scan or modify the home directory.

### `connect_zshrc`

- accepts the path returned by the native picker
- requires the filename `.zshrc`
- validates read/write access
- creates and stores the security-scoped bookmark
- backs up the file before adding the first block
- returns existing safe aliases for optional import

### `save_aliases`

- validates and renders all aliases
- resolves and activates the bookmark
- replaces the managed block
- stops security-scoped access
- writes structured `config.json`

### `disconnect_zshrc`

- resolves the current bookmark
- backs up the connected `.zshrc`
- removes the managed block from the old file
- deletes the old bookmark
- keeps structured aliases in the container for the next connection

### `scan_zshrc_import`

- reads the connected file as text
- ignores aliases inside the managed block
- ignores names already managed by EasyAlias
- returns conservative one-line candidates

### `import_zshrc_aliases`

- rescans the connected file
- verifies selected ids against current line numbers
- creates a container backup
- replaces confirmed source lines with zsh no-op markers
- writes imported commands into the managed block
- stores the updated structured config

## Container Data

```text
Application Support/
  config.json
  zshrc.bookmark
  .zshrc-import-v1
  backups/
    zshrc-<timestamp>.backup
```

No Apple certificate, private key, API key, or provisioning profile belongs in this directory or in Git.

## Entitlements

`Entitlements.plist` contains:

| Entitlement | Reason |
| --- | --- |
| `com.apple.security.app-sandbox` | required for Mac App Store distribution |
| `com.apple.application-identifier` | binds the app to Team ID and Bundle ID |
| `com.apple.developer.team-identifier` | identifies the Apple Developer team |
| `com.apple.security.files.user-selected.read-write` | permits `.zshrc` selected through `NSOpenPanel` |
| `com.apple.security.files.user-selected.executable` | permits writing shell script content without command-line quarantine problems |
| `com.apple.security.files.bookmarks.app-scope` | persists selected-file access between launches |

No broad home-directory entitlement, temporary exception, network entitlement, shell execution plugin, or child process is used.

## Import Safety

The parser skips:

- indented aliases that may belong to shell conditions or functions
- global aliases and other zsh alias options
- multiple definitions on one line
- repeated names
- malformed or multiline declarations
- the reserved `easya` application shortcut
- every alias inside the managed block

The `.zshrc` is parsed as text and never sourced or executed.

```mermaid
flowchart TD
  Scan["Read selected .zshrc as text"] --> Validate["Parse conservative aliases"]
  Validate --> Review["User reviews candidates"]
  Review --> Confirm{"Confirmed?"}
  Confirm -- "no" --> Unchanged["Leave .zshrc unchanged"]
  Confirm -- "yes" --> Backup["Write container backup"]
  Backup --> Replace["Replace selected lines with no-op markers"]
  Replace --> Block["Render aliases in managed block"]
```

## Packaging Flow

```mermaid
flowchart LR
  Source["mac_src_app_store"] --> Checks["npm build + cargo test"]
  Checks --> Universal["Universal signed .app"]
  Profile["Mac App Store Connect profile"] --> Universal
  Distribution["Apple Distribution certificate"] --> Universal
  Universal --> Pkg["Installer-signed .pkg"]
  Installer["Mac Installer Distribution certificate"] --> Pkg
  Pkg --> Transporter["Transporter / altool"]
  Transporter --> TestFlight["App Store Connect + TestFlight"]
```

The final Store build merges `tauri.appstore.conf.json` into the base Tauri configuration and embeds `EasyAlias_AppStore.provisionprofile`.

## Tests

Rust unit tests cover:

- ignoring aliases inside the managed block
- replacing one managed block without changing unrelated lines
- rejecting malformed markers
- replacing only confirmed import lines

The production checks are:

```zsh
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
plutil -lint \
  src-tauri/Entitlements.local.plist \
  src-tauri/Entitlements.plist \
  src-tauri/Info.plist
```

A signed manual test is still required because bookmark persistence and App Sandbox enforcement depend on macOS code signing.
