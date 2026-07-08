# EasyAlias

EasyAlias ist ein kleiner macOS-Prototyp zum Erstellen, Anzeigen und Verwalten von zsh-Aliasen.

Die App ist als Tauri-App angelegt: Die Oberflaeche ist Web-Technologie, die fertige App laeuft aber als lokale Desktop-App und kann Dateien auf deinem Mac schreiben.

## Was die App macht

- Aliase in einer UI anlegen, bearbeiten und loeschen
- Aktionen per Dropdown auswaehlen
- den generierten Shell-Befehl vor dem Speichern anzeigen
- Erstellungsdatum und letztes Update speichern
- eine strukturierte JSON-Datei fuer die UI verwalten
- daraus eine `aliases.zsh` fuer dein Terminal generieren

## Dateien auf deinem Mac

Die Tauri-App schreibt spaeter diese Dateien:

```text
~/.easyalias/config.json
~/.easyalias/aliases.zsh
```

Damit zsh die generierten Aliase kennt, muss einmalig diese Zeile in `~/.zshrc` stehen:

```zsh
source ~/.easyalias/aliases.zsh
```

Neue oder geaenderte Aliase gelten in neuen Terminal-Fenstern automatisch. In einem offenen Terminal kannst du sie direkt nachladen:

```zsh
source ~/.zshrc
```

## Setup

VS Code reicht als Editor.

Du brauchst:

- Node.js und npm
- Xcode Command Line Tools oder Xcode
- Rust/Cargo fuer Tauri

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

## Entwicklung

Abhaengigkeiten installieren:

```zsh
npm install
```

Nur die Web-UI im Browser starten:

```zsh
npm run dev
```

In diesem Modus speichert die App nur testweise im Browser-`localStorage`.

Native Tauri-App starten:

```zsh
npm run tauri dev
```

In diesem Modus schreibt die App echte Dateien unter `~/.easyalias/`.

Produktionsbuild der Web-UI pruefen:

```zsh
npm run build
```

## Projektstruktur

```text
src/
  main.ts        UI-Logik, Datenmodell, Command-Preview
  styles.css     Styling

src-tauri/
  src/main.rs    Tauri Commands fuer Laden/Speichern
  tauri.conf.json

docs/
  ARCHITECTURE.md
```

## Datenmodell

Ein Alias sieht intern ungefaehr so aus:

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

## Naechste sinnvolle Schritte

- Finder-Dialog fuer Datei-/Ordnerauswahl einbauen
- Import bestehender Aliase aus `~/.zshrc`
- Button zum automatischen Eintragen der `source`-Zeile in `~/.zshrc`
- Suchfeld und Filter fuer viele Aliase
- App-Icon und macOS-Bundle bauen
