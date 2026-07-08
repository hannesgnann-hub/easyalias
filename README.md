# EasyAlias

EasyAlias ist ein kleiner macOS-Prototyp zum Erstellen, Anzeigen und Verwalten von zsh-Aliasen.

Die App nutzt Tauri: Die Oberflaeche wird mit Web-Technologie gebaut, laeuft aber als lokale Desktop-App und darf Dateien auf deinem Mac verwalten.

## Highlights

- Aliase ueber eine UI erstellen, bearbeiten und loeschen
- Aktion per Dropdown auswaehlen
- Shell-Befehl vor dem Speichern als Vorschau sehen
- `createdAt` und `updatedAt` pro Alias speichern
- Alias-Daten strukturiert als JSON halten
- automatisch eine `aliases.zsh` fuer dein Terminal generieren
- beim ersten Tauri-Start automatisch mit `~/.zshrc` verbinden

## Quickstart

```zsh
npm install
npm run dev
```

Das startet nur die Web-UI im Browser. In diesem Modus speichert EasyAlias testweise im Browser-`localStorage`.

Fuer die echte macOS-App:

```zsh
npm run tauri dev
```

Dann schreibt EasyAlias echte Dateien unter `~/.easyalias/`.

## Voraussetzungen

VS Code reicht als Editor. Fuer die Tauri-App brauchst du lokal:

| Tool | Zweck |
| --- | --- |
| Node.js + npm | Frontend, Dev-Server, Build |
| Xcode Command Line Tools oder Xcode | macOS Build-Toolchain |
| Rust + Cargo | Tauri Backend und Desktop-App |

Pruefen:

```zsh
node -v
npm -v
xcode-select -p
rustc --version
cargo --version
```

Falls Rust fehlt:

```zsh
curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
```

Danach ein neues Terminal oeffnen.

## Dateien auf deinem Mac

EasyAlias verwaltet bewusst eigene Dateien und schreibt nicht direkt deine komplette `~/.zshrc` um.

```text
~/.easyalias/config.json
~/.easyalias/aliases.zsh
```

Beim ersten Tauri-Start haengt EasyAlias diese Zeile an `~/.zshrc` an, falls sie noch fehlt:

```zsh
source ~/.easyalias/aliases.zsh
```

Neue oder geaenderte Aliase gelten in neuen Terminal-Fenstern automatisch. In einem bereits offenen Terminal kannst du sie direkt nachladen:

```zsh
source ~/.zshrc
```

## Entwicklung

| Kommando | Wirkung |
| --- | --- |
| `npm run dev` | startet die Browser-Vorschau |
| `npm run build` | baut und prueft die Web-UI |
| `npm run tauri dev` | startet die echte Tauri-App |
| `npm run tauri build` | baut spaeter das macOS-App-Bundle |

## Projektstruktur

```text
easyalias/
  src/
    main.ts            UI-Logik, Datenmodell, Command-Preview
    styles.css         Styling

  src-tauri/
    src/main.rs        Tauri Commands fuer Laden/Speichern
    tauri.conf.json    Tauri App-Konfiguration
    icons/icon.png     Platzhalter-App-Icon

  docs/
    ARCHITECTURE.md    Technischer Aufbau
```

## Datenmodell

Ein Alias sieht intern so aus:

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

## Alias-Aktionen

| Aktion | Generierter Befehl |
| --- | --- |
| Navigiere zu Ordner | `cd "<pfad>"` |
| Oeffnen | `open "<pfad>"` |
| Ausfuehren | `"<pfad>"` |
| Gradle Build | `cd "<pfad>" && ./gradlew build` |
| Maven Build | `cd "<pfad>" && mvn clean package` |
| Custom Command | frei eingetragener Shell-Befehl |

## Roadmap

- Finder-Dialog fuer Datei-/Ordnerauswahl
- Import bestehender Aliase aus `~/.zshrc`
- Suchfeld und Filter fuer viele Aliase
- richtiges macOS-App-Icon
- macOS-Bundle mit `npm run tauri build`
