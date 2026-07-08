# EasyAlias macOS

EasyAlias macOS is a Tauri prototype for creating and managing zsh aliases through a desktop UI.

The app uses web technology for the interface, but runs as a local macOS desktop app and can manage files on your machine.

## Highlights

- create, edit, and delete aliases through a UI
- choose an action from a dropdown
- preview the generated shell command before saving
- choose files and folders through the native macOS picker
- store `createdAt` and `updatedAt` per alias
- keep alias data as structured JSON
- automatically generate an `aliases.zsh` file for your terminal
- connect itself to `~/.zshrc` on first Tauri startup

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

## Development

| Command | Effect |
| --- | --- |
| `npm run dev` | starts the browser preview |
| `npm run build` | builds and checks the web UI |
| `npm run tauri dev` | starts the real Tauri app |
| `npm run tauri build` | builds the macOS app bundle |

## Project Structure

```text
easyalias/
  src/
    main.ts            UI logic, data model, command preview
    styles.css         styling

  src-tauri/
    src/main.rs        Tauri commands for loading/saving
    tauri.conf.json    Tauri app configuration
    icons/icon.png     placeholder app icon

  docs/
    ARCHITECTURE.md    technical architecture
```

## Data Model

An alias is stored like this:

```json
{
  "id": "uuid",
  "name": "beerv2",
  "path": "~/Desktop/projekte/beerv2_app",
  "action": "navigate",
  "commandPreview": "cd \"$HOME/Desktop/projekte/beerv2_app\"",
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

- import existing aliases from `~/.zshrc`
- search and filter for large alias lists
- polished macOS app icon
- macOS bundle via `npm run tauri build`
