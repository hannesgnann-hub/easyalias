# EasyAlias Windows

EasyAlias Windows is a Tauri desktop app for creating and managing Windows command shortcuts through a desktop UI.

The app uses web technology for the interface, but runs as a local Windows desktop app and can manage files on your machine.

## Highlights

- create, edit, and delete shortcuts through a UI
- detect simple existing `.cmd`/`.bat` aliases in user-owned `PATH` folders on first start and rescan them later from the header import button
- expand optional Windows suggestions and add them with one click
- choose an action from a dropdown
- preview the generated `cmd.exe` command before saving
- choose files and folders through the native Windows picker
- store `createdAt` and `updatedAt` per shortcut
- keep shortcut data as structured JSON
- automatically generate `.cmd` files for `cmd.exe`
- connect `~\.easyalias\bin` to the user's `PATH` on first Tauri startup
- link to the GitHub repository and EasyAlias subreddit from the footer

## Quickstart

```powershell
npm install
npm run dev
```

This starts only the web UI in the browser. In this mode, EasyAlias stores test data in browser `localStorage`.

For the real Windows app:

```powershell
npm run tauri dev
```

In this mode, EasyAlias writes real files under `~\.easyalias\`.

## Requirements

VS Code is enough as an editor. For the Tauri app, you need:

| Tool | Purpose |
| --- | --- |
| Node.js + npm | frontend, dev server, build |
| Rust + Cargo | Tauri backend and desktop app |
| Microsoft C++ Build Tools | Windows Rust/Tauri build toolchain |
| WebView2 Runtime | desktop WebView runtime, usually already installed |

Check your setup:

```powershell
node -v
npm -v
rustc --version
cargo --version
```

If Rust is missing, install it from [rustup.rs](https://rustup.rs/).

If the C++ build tools are missing, install "Desktop development with C++" through the Visual Studio Installer.

## Files on Windows

EasyAlias intentionally manages its own files and does not directly rewrite shell startup files.

```text
~\.easyalias\config.json
~\.easyalias\bin\
~\.easyalias\.cmd-import-v1
~\.easyalias\import-backup-*\
```

Each alias becomes one command file:

```text
~\.easyalias\bin\beerv2.cmd
~\.easyalias\bin\test1.cmd
```

On first Tauri startup, EasyAlias adds the command folder to your user `PATH` if it is missing:

```text
%USERPROFILE%\.easyalias\bin
```

After the first setup, open a new terminal window. Then commands work in `cmd.exe`:

```cmd
beerv2
test1
```

Verify that Windows can find a generated command:

```cmd
where test1
```

Expected output:

```text
C:\Users\<you>\.easyalias\bin\test1.cmd
```

They can also be called from PowerShell as external commands, but folder-changing aliases only persist in `cmd.exe`.

EasyAlias also creates this helper command if `easya.cmd` does not already conflict with one of your aliases:

```cmd
easya
```

## Import Existing Command Files

On a fresh installation, EasyAlias automatically checks `.cmd` and `.bat` files in `PATH` directories located inside `%USERPROFILE%`. The import icon in the top-right corner repeats this scan at any time. System directories and EasyAlias' own command directory are never scanned.

Only simple alias files with one executable command are offered. Standard `@echo off`, blank lines, and comments are ignored. Multiline batch logic, labels, duplicate names, and location-dependent commands using `%~dp0`, `%~f0`, or `%0` are skipped.

The first-start or manually opened dialog lets you select which files EasyAlias should manage. Before originals are removed, every selected file is copied to:

```text
~\.easyalias\import-backup-<timestamp>\
```

Imported entries become Custom Commands and are regenerated as `.cmd` files under `~\.easyalias\bin`. Choosing **Skip Import** leaves all existing files untouched and closes only the automatic first-start prompt. The manual import icon remains available, and command names already managed by EasyAlias are excluded from later rescans.

## How It Works

EasyAlias does not create PowerShell aliases on Windows. It creates normal command files:

```cmd
@echo off
cd /d "%USERPROFILE%\Desktop\projects\beerv2_app"
```

Because `~\.easyalias\bin` is added to the user `PATH`, Windows can resolve `beerv2` as `beerv2.cmd`.

This means:

- aliases work naturally in `cmd.exe`
- new aliases are available in new terminal windows
- the app does not need to edit PowerShell profiles
- each command can be inspected or debugged with `type`

Example:

```cmd
type "%USERPROFILE%\.easyalias\bin\beerv2.cmd"
```

## Troubleshooting

If a command is not found, first open a new `cmd.exe` window. PATH changes only apply to new terminal processes.

Check whether the command folder is in PATH:

```cmd
echo %PATH%
```

Check whether the command file exists:

```cmd
dir "%USERPROFILE%\.easyalias\bin"
```

Check where Windows resolves a command from:

```cmd
where test1
```

If `where test1` finds nothing, start EasyAlias once so it can regenerate `.cmd` files and ensure the PATH entry exists.

If you run a folder-changing alias from PowerShell, the child `cmd.exe` process can change its own directory, but PowerShell's parent location will not change. Use `cmd.exe` for folder-jump aliases.

## Development

| Command | Effect |
| --- | --- |
| `npm run dev` | starts the browser preview |
| `npm run build` | builds and checks the web UI |
| `npm run tauri dev` | starts the real Tauri app |
| `npm run tauri build` | builds the Windows installer |

The configured NSIS build writes its installer below:

```text
src-tauri\target\release\bundle\nsis\
```

Copy the finished installer into the repository export folder from PowerShell:

```powershell
Copy-Item .\src-tauri\target\release\bundle\nsis\*.exe ..\windows_export\
```

## Project Structure

```text
windows_src/
  src/
    main.ts            UI logic, data model, command preview
    styles.css         styling

  src-tauri/
    src/main.rs        PATH setup, first-start/manual command import, and persistence
    tauri.conf.json    Tauri app configuration
    icons/              PNG and Windows ICO application icons

  docs/
    ARCHITECTURE.md    technical architecture
```

## Data Model

A shortcut is stored like this:

```json
{
  "id": "uuid",
  "name": "beerv2",
  "path": "~/Desktop/projects/beerv2_app",
  "action": "navigate",
  "commandPreview": "cd /d \"%USERPROFILE%\\Desktop\\projects\\beerv2_app\"",
  "createdAt": "2026-07-08T16:35:00.000Z",
  "updatedAt": "2026-07-08T16:35:00.000Z"
}
```

## Shortcut Actions

| Action | Generated command |
| --- | --- |
| Navigate to folder | `cd /d "<path>"` |
| Open | `start "" "<path>"` |
| Execute | `call "<path>" %*` |
| Gradle Build | `cd /d "<path>" && call gradlew.bat build` |
| Maven Build | `cd /d "<path>" && call mvn clean package` |
| Custom Command | user-provided cmd/batch command |

## Suggested Shortcuts

The optional Suggestions section starts collapsed. Clicking `Use` immediately creates the matching `.cmd` shortcut and removes that name from the available suggestions.

Suggestions include common cmd, Git, Gradle Wrapper, Maven Wrapper, Docker, networking, and folder commands. Wrapper suggestions use Windows batch syntax such as:

```cmd
call gradlew.bat %*
call mvnw.cmd %*
```

`%*` forwards additional arguments, so `gw clean` runs `gradlew.bat clean`.

## Documentation Layout

EasyAlias keeps a shared project overview plus platform-specific usage and architecture documents:

| Document | Purpose |
| --- | --- |
| `../README.md` | shared project overview for macOS, Windows, and Linux |
| `../mac_src/README.md` | macOS-specific usage and zsh behavior |
| `../mac_src/docs/ARCHITECTURE.md` | macOS technical architecture |
| `README.md` | Windows-specific usage and cmd/PATH behavior |
| `docs/ARCHITECTURE.md` | Windows technical architecture |
| `../linux_src/README.md` | Linux-specific usage and shell behavior |
| `../linux_src/docs/ARCHITECTURE.md` | Linux technical architecture |

## Roadmap

- search and filter for large shortcut lists
- signed Windows release automation
- optional structured config export and restore

## License

EasyAlias is licensed under the MIT License. See `../LICENSE`.
