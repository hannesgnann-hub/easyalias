# Architektur

Dieses Dokument beschreibt den technischen Aufbau von EasyAlias.

## Ueberblick

EasyAlias besteht aus zwei Schichten:

| Schicht | Datei | Aufgabe |
| --- | --- | --- |
| Frontend | `src/main.ts` | UI, Formular-State, Command-Preview |
| Styling | `src/styles.css` | Layout und visuelle Oberflaeche |
| Backend | `src-tauri/src/main.rs` | lokale Dateien lesen/schreiben |
| Tauri Config | `src-tauri/tauri.conf.json` | App-Fenster, Build, Bundle |
| Tauri Dialog Plugin | `@tauri-apps/plugin-dialog` | nativer Datei-/Ordner-Picker |

Die Grundidee: EasyAlias verwaltet nicht die gesamte `~/.zshrc`, sondern erzeugt eine separate Alias-Datei und verbindet diese einmalig mit zsh.

```mermaid
flowchart TB
  UI["Frontend src/main.ts"]
  CSS["Styling src/styles.css"]
  Tauri["Tauri Runtime"]
  Rust["Rust Backend src-tauri/src/main.rs"]
  Dialog["Dialog Plugin Datei/Ordner"]
  Opener["Opener Plugin GitHub-Link"]
  Files["~/.easyalias Dateien"]
  Zshrc["~/.zshrc Setup"]

  UI --> CSS
  UI --> Tauri
  Tauri --> Rust
  Tauri --> Dialog
  Tauri --> Opener
  Rust --> Files
  Rust --> Zshrc
```

## Datenfluss

```text
UI Formular
  -> AliasEntry
  -> ~/.easyalias/config.json
  -> ~/.easyalias/aliases.zsh
  -> source in ~/.zshrc
  -> neue Terminal-Sessions
```

```mermaid
flowchart LR
  Form["UI Formular"]
  Entry["AliasEntry"]
  Config["config.json"]
  Generated["aliases.zsh"]
  Source["source in ~/.zshrc"]
  Terminal["Neue Terminal-Session"]

  Form --> Entry
  Entry --> Config
  Entry --> Generated
  Generated --> Source
  Source --> Terminal
```

Im Browser-Dev-Modus ohne Tauri wird der Zustand nur in `localStorage` gespeichert. So kann die UI schnell getestet werden, ohne echte Shell-Dateien zu veraendern.

Im Tauri-Modus schreibt das Backend echte Dateien auf dem Mac.

```mermaid
flowchart TD
  Start["App startet"]
  Runtime{"Tauri Runtime?"}
  Browser["Browser Preview"]
  Native["Native Tauri App"]
  LocalStorage["localStorage"]
  Backend["Rust Commands"]
  RealFiles["Echte Dateien"]

  Start --> Runtime
  Runtime -- "nein" --> Browser
  Browser --> LocalStorage
  Runtime -- "ja" --> Native
  Native --> Backend
  Backend --> RealFiles
```

## Lokale Dateien

| Datei | Inhalt | Besitzer |
| --- | --- | --- |
| `~/.easyalias/config.json` | strukturierte Alias-Daten fuer die UI | EasyAlias |
| `~/.easyalias/aliases.zsh` | generierte zsh-Aliase | EasyAlias |
| `~/.zshrc` | enthaelt nur die `source`-Zeile | Nutzer + EasyAlias Setup |

Beim ersten Tauri-Start stellt das Backend sicher:

1. `~/.easyalias/` existiert.
2. `~/.easyalias/aliases.zsh` existiert.
3. `~/.zshrc` enthaelt `source ~/.easyalias/aliases.zsh`.
4. `~/.zshrc` enthaelt `alias easya='open /Applications/EasyAlias.app'`, falls `easya` noch nicht existiert.

```mermaid
sequenceDiagram
  participant UI as Frontend
  participant Rust as Rust Backend
  participant Dir as ~/.easyalias/
  participant AliasFile as aliases.zsh
  participant Zshrc as ~/.zshrc

  UI->>Rust: load_aliases()
  Rust->>Dir: create_dir_all()
  Rust->>AliasFile: create if missing
  Rust->>Zshrc: check source line
  Rust->>Zshrc: append source if missing
  Rust->>Zshrc: append easya shortcut if missing
  Rust-->>UI: AppState + aliases
```

## Frontend

Das Frontend ist bewusst leichtgewichtig:

- kein UI-Framework
- TypeScript
- Vite
- direkte DOM-Updates

Wichtige Aufgaben:

- Formularwerte verwalten
- Alias-Namen validieren
- Command-Preview live aktualisieren
- Aliase anzeigen, auswaehlen und loeschen
- Tauri Commands aufrufen, wenn die App nativ laeuft

Die wichtigsten Typen:

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

Das Tauri-Backend stellt aktuell zwei Commands bereit:

```rust
load_aliases()
save_aliases(aliases)
```

`load_aliases` erledigt den Start-Setup:

- App-Ordner erstellen
- leere `aliases.zsh` anlegen, falls sie fehlt
- `source`-Zeile in `~/.zshrc` sicherstellen
- `easya`-Shortcut in `~/.zshrc` sicherstellen
- `config.json` laden, falls vorhanden

`save_aliases` schreibt:

- `config.json` als Datenbasis fuer die UI
- `aliases.zsh` als generierte Shell-Datei

```mermaid
sequenceDiagram
  participant UI as Frontend
  participant Rust as Rust Backend
  participant Config as config.json
  participant Zsh as aliases.zsh

  UI->>UI: create/edit/delete AliasEntry
  UI->>Rust: save_aliases(aliases)
  Rust->>Rust: validate alias names
  Rust->>Config: write pretty JSON
  Rust->>Zsh: write generated zsh aliases
  Rust-->>UI: updated AppState
```

## Shell-Generierung

Aus einem Alias-Eintrag wird eine zsh-Zeile:

```zsh
# Generated by EasyAlias.
# Edit aliases in the app, not by hand.

alias beerv2='cd "$HOME/Desktop/projekte/beerv2_app"'
```

Das Backend validiert vor dem Schreiben:

- Alias-Name ist nicht leer
- Alias-Name beginnt mit Buchstabe oder `_`
- Alias-Name enthaelt nur Buchstaben, Zahlen, `_` oder `-`
- Command-Preview ist nicht leer

```mermaid
flowchart TD
  AliasEntry["AliasEntry"]
  ValidateName{"Name gueltig?"}
  ValidateCommand{"Command vorhanden?"}
  Quote["Command single-quote escapen"]
  Line["alias name='command'"]
  Error["Fehler an UI"]

  AliasEntry --> ValidateName
  ValidateName -- "nein" --> Error
  ValidateName -- "ja" --> ValidateCommand
  ValidateCommand -- "nein" --> Error
  ValidateCommand -- "ja" --> Quote
  Quote --> Line
```

## Sicherheit

EasyAlias veraendert `~/.zshrc` nur minimal:

```zsh
# EasyAlias aliases
source ~/.easyalias/aliases.zsh

# EasyAlias app shortcut
alias easya='open /Applications/EasyAlias.app'
```

Bestehende Inhalte bleiben erhalten.

Wichtige Grenzen:

- Custom Commands sind echte Shell-Befehle.
- Die generierte `aliases.zsh` ist Output der App und sollte nicht manuell editiert werden.
- Standardpfade werden in doppelte Anfuehrungszeichen gesetzt.
- Bestehende Aliase aus `~/.zshrc` werden aktuell noch nicht importiert.

## Roadmap

Kurzfristig:

- Import bestehender Aliase
- Tests fuer Command-Generierung

Spaeter:

- Settings-Fenster
- echtes App-Icon
- macOS `.app` Bundle
- optionaler Export/Backup-Mechanismus
