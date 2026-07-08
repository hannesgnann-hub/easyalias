# EasyAlias Windows

EasyAlias Windows is a Tauri prototype for creating and managing PowerShell shortcuts through a desktop UI.

The app uses web technology for the interface, but runs as a local Windows desktop app and can manage files on your machine.

## Highlights

- create, edit, and delete shortcuts through a UI
- choose an action from a dropdown
- preview the generated PowerShell command before saving
- choose files and folders through the native Windows picker
- store `createdAt` and `updatedAt` per shortcut
- keep shortcut data as structured JSON
- automatically generate an `aliases.ps1` file for PowerShell
- connect itself to the PowerShell profile on first Tauri startup

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

EasyAlias intentionally manages its own files and does not directly rewrite your whole PowerShell profile.

```text
~\.easyalias\config.json
~\.easyalias\aliases.ps1
```

On first Tauri startup, EasyAlias appends this line to both common PowerShell profile files if it is missing:

```powershell
. "$HOME\.easyalias\aliases.ps1"
```

The app checks these profile files:

```text
~\Documents\PowerShell\Microsoft.PowerShell_profile.ps1
~\Documents\WindowsPowerShell\Microsoft.PowerShell_profile.ps1
```

It also creates this shortcut if `easya` does not already exist:

```powershell
function easya { Start-Process "$env:LOCALAPPDATA\Programs\EasyAlias\EasyAlias.exe" }
```

After installing the app with the generated installer, you can open it from PowerShell:

```powershell
easya
```

New or changed shortcuts are available automatically in new PowerShell windows. In an already open PowerShell session, reload them with:

```powershell
. $PROFILE
```

## Development

| Command | Effect |
| --- | --- |
| `npm run dev` | starts the browser preview |
| `npm run build` | builds and checks the web UI |
| `npm run tauri dev` | starts the real Tauri app |
| `npm run tauri build` | builds the Windows installer |

## Project Structure

```text
windows_src/
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

A shortcut is stored like this:

```json
{
  "id": "uuid",
  "name": "beerv2",
  "path": "~/Desktop/projects/beerv2_app",
  "action": "navigate",
  "commandPreview": "Set-Location \"$HOME\\Desktop\\projects\\beerv2_app\"",
  "createdAt": "2026-07-08T16:35:00.000Z",
  "updatedAt": "2026-07-08T16:35:00.000Z"
}
```

## Shortcut Actions

| Action | Generated command |
| --- | --- |
| Navigate to folder | `Set-Location "<path>"` |
| Open | `Start-Process "<path>"` |
| Execute | `& "<path>"` |
| Gradle Build | `Set-Location "<path>"; .\gradlew.bat build` |
| Maven Build | `Set-Location "<path>"; mvn clean package` |
| Custom Command | user-provided PowerShell command |

## Roadmap

- import existing functions from a PowerShell profile
- search and filter for large shortcut lists
- polished Windows app icon
- Windows installer via `npm run tauri build`
