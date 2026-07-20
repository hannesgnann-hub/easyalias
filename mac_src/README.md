# EasyAlias macOS

EasyAlias macOS is a Tauri desktop app for creating and managing zsh aliases through a desktop UI.

The app uses web technology for the interface, but runs as a local macOS desktop app and can manage files on your machine.

## Highlights

- create, edit, and delete aliases through a UI
- detect existing simple aliases in `~/.zshrc` on first start and rescan them later from the header import button
- expand optional macOS alias suggestions and add them with one click
- choose an action from a dropdown
- preview the generated shell command before saving
- choose files and folders through the native macOS picker
- store `createdAt` and `updatedAt` per alias
- keep alias data as structured JSON
- automatically generate an `aliases.zsh` file for your terminal
- connect itself to `~/.zshrc` on first Tauri startup
- link to the GitHub repository and EasyAlias subreddit from the footer

## Install

EasyAlias is available as a Homebrew cask:

```zsh
brew tap hannesgnann-hub/tap
brew trust hannesgnann-hub/tap
brew install --cask easyalias
```

## Quickstart

```zsh
npm install
npm run dev
```

This starts only the web UI in the browser. In this mode, EasyAlias stores test data in browser `localStorage`.

For the real macOS app:

```zsh
npm run tauri dev
```

In this mode, EasyAlias writes real files under `~/.easyalias/`.

## Requirements

VS Code is enough as an editor. For the Tauri app, you need:

| Tool | Purpose |
| --- | --- |
| Node.js + npm | frontend, dev server, build |
| Xcode Command Line Tools or Xcode | macOS build toolchain |
| Rust + Cargo | Tauri backend and desktop app |

Check your setup:

```zsh
node -v
npm -v
xcode-select -p
rustc --version
cargo --version
```

If Rust is missing:

```zsh
curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
```

Then open a new terminal.

## Files on Your Mac

EasyAlias intentionally manages its own files and does not directly rewrite your whole `~/.zshrc`.

```text
~/.easyalias/config.json
~/.easyalias/aliases.zsh
~/.easyalias/.zshrc-import-v1
```

On first Tauri startup, EasyAlias appends this line to `~/.zshrc` if it is missing:

```zsh
source ~/.easyalias/aliases.zsh
```

It also creates this shortcut if `easya` does not already exist:

```zsh
alias easya='open /Applications/EasyAlias.app'
```

After installing the app to `/Applications`, you can open it from the terminal:

```zsh
easya
```

New or changed aliases are available automatically in new terminal windows. In an already open terminal, reload them with:

```zsh
source ~/.zshrc
```

## Import Existing Aliases

On a fresh installation, EasyAlias automatically scans `~/.zshrc` for conservative one-line declarations such as:

```zsh
alias ll='ls -lah'
alias project="cd \"$HOME/Desktop/My Project\""
```

The file is parsed as text and is never executed during detection. When matches are found, the first-start dialog lets you select which aliases EasyAlias should manage. The import icon in the top-right corner runs the same safe scan again at any time, including after the first-start prompt was skipped. Selected aliases are imported as Custom Commands so their command text remains intact.

Before changing selected lines, EasyAlias creates a timestamped backup:

```text
~/.zshrc.easyalias-backup-<timestamp>
```

Imported source lines are replaced with harmless `:` markers, while unselected aliases and all other shell configuration remain unchanged. Choosing **Skip Import** leaves every existing alias untouched and records that the automatic first-start prompt was handled. It does not disable the manual import button. Aliases already managed by EasyAlias are excluded from later rescans.

For safety, the automatic scanner skips:

- indented aliases that may belong to conditions or functions
- zsh alias options such as `alias -g`
- multiple aliases declared on one line
- alias names declared more than once
- malformed or multiline declarations
- the `easya` application shortcut

## Suggested Aliases

The optional Suggestions section starts collapsed. Clicking `Use` immediately saves the selected suggestion as a real alias; no second click on `Add` is required. Suggestions whose names are already managed disappear from the available list.

The built-in set includes common shell, Git, Gradle Wrapper, Maven Wrapper, Docker, networking, and folder shortcuts such as `ll`, `gs`, `gw`, `gwb`, and `reloadzsh`.

## Development

| Command | Effect |
| --- | --- |
| `npm run dev` | starts the browser preview |
| `npm run build` | builds and checks the web UI |
| `npm run tauri dev` | starts the real Tauri app |
| `npm run tauri build` | builds the macOS app bundle |

The configured build produces:

```text
src-tauri/target/release/bundle/macos/EasyAlias.app
```

Install the local build and create the repository export archive with:

```zsh
cp -R src-tauri/target/release/bundle/macos/EasyAlias.app /Applications/
ditto -c -k --keepParent src-tauri/target/release/bundle/macos/EasyAlias.app ../mac_export/EasyAlias.zip
```

## Project Structure

```text
mac_src/
  src/
    main.ts            UI logic, data model, command preview
    styles.css         styling

  src-tauri/
    src/main.rs        Tauri commands for loading, rescanning, importing, and saving
    tauri.conf.json    Tauri app configuration
    icons/              PNG and macOS ICNS application icons

  docs/
    ARCHITECTURE.md    technical architecture
```

## Data Model

An alias is stored like this:

```json
{
  "id": "uuid",
  "name": "beerv2",
  "path": "~/Desktop/projects/beerv2_app",
  "action": "navigate",
  "commandPreview": "cd \"$HOME/Desktop/projects/beerv2_app\"",
  "createdAt": "2026-07-08T16:35:00.000Z",
  "updatedAt": "2026-07-08T16:35:00.000Z"
}
```

## Alias Actions

| Action | Generated command |
| --- | --- |
| Navigate to folder | `cd "<path>"` |
| Open | `open "<path>"` |
| Execute | `"<path>"` |
| Gradle Build | `cd "<path>" && ./gradlew build` |
| Maven Build | `cd "<path>" && mvn clean package` |
| Custom Command | user-provided shell command |

## Roadmap

- search and filter for large alias lists
- signed and notarized release automation
- optional structured config export and restore

## Documentation Layout

| Document | Purpose |
| --- | --- |
| `../README.md` | shared project overview for all platforms |
| `README.md` | macOS usage, installation, and zsh behavior |
| `docs/ARCHITECTURE.md` | macOS technical architecture |

## License

EasyAlias is licensed under the MIT License. See `../LICENSE`.
