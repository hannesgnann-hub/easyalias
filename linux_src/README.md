# EasyAlias Linux

EasyAlias Linux is a Tauri desktop app for creating and managing terminal aliases through a UI.

It detects bash or zsh, keeps the alias data in its own directory, and connects one generated shell file to the matching startup file.

Project website: [easyalias.org](https://easyalias.org)

## ❤️ Support EasyAlias

Hi, I'm Hannes, the creator of EasyAlias and a Software Engineering student.

If EasyAlias saves you time, consider supporting its development.

Your sponsorship helps me fix bugs, develop new features, and keep EasyAlias free and open source.

[Become a GitHub Sponsor](https://github.com/sponsors/hannesgnann-hub)

## Highlights

- create and edit aliases through a UI, with 30-day recovery after deletion
- pin favorites above the regular alias list
- export all or selected aliases to a portable JSON backup and restore selected entries
- detect existing aliases in the active shell startup file on first start and rescan them later from the header import button
- browse 31 paginated Linux suggestions and add them with one click
- choose files and folders with the native Linux picker
- preview the generated shell command before saving
- store `createdAt` and `updatedAt` for every alias
- automatically detect bash or zsh from `$SHELL`
- generate `~/.easyalias/aliases.sh`
- connect the generated file to `~/.bashrc` or `~/.zshrc`
- add the `easya` shortcut for opening the installed application
- build `.deb`, `.rpm`, and `.AppImage` packages
- dismiss status messages manually or let them disappear after three seconds
- build and run multi-step Automations (bash/zsh commands and timed waits) in a chosen working directory
- link to the website, GitHub repository, EasyAlias subreddit, and sponsor page from the footer

The [shared feature tour](../README.md#feature-tour) illustrates favorites, paged suggestions, portable backups, and Trash. Its screenshots use macOS window chrome, but the workflow is the same on Linux.

## Install with Homebrew

The current Homebrew formula supports ARM64 Linux systems:

```bash
brew tap hannesgnann-hub/tap
brew trust hannesgnann-hub/tap
brew install easyalias
```

Then launch the app with `easyalias` or through the desktop application menu.

## Requirements

VS Code is enough as the editor. Building the desktop app also needs Node.js, Rust, Cargo, and Tauri's Linux system libraries.

For Debian or Ubuntu:

```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

For Fedora:

```bash
sudo dnf check-update
sudo dnf install webkit2gtk4.1-devel \
  openssl-devel \
  curl \
  wget \
  file \
  libappindicator-gtk3-devel \
  librsvg2-devel \
  libxdo-devel
sudo dnf group install "c-development"
```

See the [official Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for Arch, openSUSE, Alpine, NixOS, and other distributions.

Install the current Node.js LTS release and Rust. Rust can be installed with:

```bash
curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
```

Check the setup:

```bash
node -v
npm -v
rustc --version
cargo --version
```

## Development

Install JavaScript dependencies:

```bash
cd linux_src
npm install
```

Run only the browser UI:

```bash
npm run dev
```

Browser preview stores test data in `localStorage` and does not edit shell files.

Run the real Linux desktop app:

```bash
npm run tauri dev
```

The Tauri app writes real files below `~/.easyalias/` and performs the one-time shell connection.

## Shell Integration

EasyAlias reads `$SHELL` on startup:

| Detected shell | Startup file |
| --- | --- |
| bash | `~/.bashrc` |
| zsh | `~/.zshrc` |
| unknown or missing | `~/.bashrc` |

The app manages these files:

```text
~/.easyalias/config.json
~/.easyalias/aliases.sh
~/.easyalias/.shell-import-v1
~/.easyalias/trash.json
~/.easyalias/automations.json
~/.easyalias/automations-trash.json
```

On first native startup it appends the missing lines to the detected startup file:

```bash
# EasyAlias aliases
source ~/.easyalias/aliases.sh

# EasyAlias app shortcut
alias easya='setsid -f easyalias >/dev/null 2>&1'
```

Existing shell configuration is preserved. EasyAlias only appends lines that are not already present.

## Import Existing Aliases

On a fresh installation, EasyAlias automatically scans the detected `~/.bashrc` or `~/.zshrc` as text. It never executes the startup file during detection. The first-start dialog lists safe, unindented, single-line aliases and lets you choose which ones EasyAlias should manage. The import icon in the top-right corner repeats the scan at any time.

Before selected lines are changed, EasyAlias creates a timestamped backup next to the startup file, for example:

```text
~/.bashrc.easyalias-backup-<timestamp>
~/.zshrc.easyalias-backup-<timestamp>
```

Imported entries become Custom Commands. Their original lines are replaced with harmless `:` markers; unselected aliases and all other configuration remain unchanged. Choosing **Skip Import** leaves the detected aliases untouched and closes only the automatic first-start prompt. The manual import icon remains available, and aliases already managed by EasyAlias are excluded from later rescans.

For safety, alias options, indented or multiline declarations, repeated names, malformed aliases, multiple assignments on one line, and the `easya` shortcut are skipped.

After the first start or after adding an alias, open a new terminal. To refresh the current terminal immediately, use one of these commands:

```bash
source ~/.bashrc
```

```zsh
source ~/.zshrc
```

## Alias Actions

| Action | Generated command |
| --- | --- |
| Go to Folder | `cd "<path>"` |
| Open | `xdg-open "<path>"` |
| Run | `"<path>"` |
| Gradle Build | `cd "<path>" && ./gradlew build` |
| Maven Build | `cd "<path>" && mvn clean package` |
| Custom Command | user-provided bash/zsh command |

The Run action expects the selected file to be executable. Make a script executable with:

```bash
chmod +x /path/to/script.sh
```

## Suggested Aliases

The optional Suggestions section starts collapsed. Clicking `Use` immediately saves the alias and removes that name from the available suggestions.

The catalog contains 31 shell, Git, Gradle Wrapper, Maven Wrapper, Docker, networking, and folder commands, displayed nine per page. Examples include `ll`, `gs`, `gaa`, `gcm`, `gw`, `mw`, `dcu`, `dcd`, `dcub`, `ports`, and `downloads`.

Wrapper aliases still accept additional arguments from bash or zsh. For example, `gw build` expands to `./gradlew build`, while `mw test` expands to `./mvnw test`.

## Favorites, Backups, and Trash

Favorites stay above regular aliases, with both groups sorted alphabetically. The header export button writes all or selected aliases to a versioned EasyAlias JSON backup. Import accepts that backup through the native picker or drag and drop, validates it, and allows a selective restore. Selected matching names replace current managed entries; unselected aliases stay unchanged.

Deleted aliases move to `~/.easyalias/trash.json` for 30 days. Trash can restore or permanently remove an individual entry, or empty all deleted entries immediately.

## Automations

The automations view (top-right play icon) is a separate workspace for repeatable, multi-step workflows, independent from the alias list.

An automation has a name, a working directory, and an ordered list of steps. Each step is either:

- **Command** – a shell command that runs through the detected shell (bash or zsh, same detection as aliases) in the automation's working directory. Choose whether the next step waits for the command to finish, or starts as soon as the process begins (useful for long-running dev servers).
- **Wait** – a pause of 1 second to 24 hours before the next step runs.

All command steps in one run share a single shell session started in the automation's working directory, so it behaves like one continuous terminal: a `cd` or an exported variable in one step is still in effect for every step after it, not just the one it was written in.

Steps run top to bottom. Running an automation opens a progress dialog showing each step's status and captured output; a **Stop** button ends the session immediately, interrupting a command that is still running (a background process that already launched keeps running on its own). If any foreground command exits with a non-zero status, the run stops and later steps are marked skipped.

Each automation can optionally carry a **group** label — a free-text tag entered in the editor (with a picker suggesting existing group names). The automations list has its own search and filter, matching aliases: search by name, working directory, command text, or group label, and filter to Favorites, Background (any step that starts a process without waiting), Git, Docker, Build, or any specific group. Choosing **Group view** in the filter dropdown replaces the list with one card per group (plus an "Ungrouped" card when applicable); clicking a card, or clicking the group chip on an automation card, filters straight to that group.

Automations are stored separately from aliases in `~/.easyalias/automations.json`, keep their own 30-day Trash in `~/.easyalias/automations-trash.json`, and support the same selective JSON backup export/import as aliases. Automations are only available in the real desktop app; the browser preview keeps its automations in `localStorage` and cannot execute commands.

## Build And Export

Build all configured Linux packages on Linux:

```bash
npm run tauri build
```

Tauri writes the packages below:

```text
src-tauri/target/release/bundle/deb/
src-tauri/target/release/bundle/rpm/
src-tauri/target/release/bundle/appimage/
```

Copy the finished packages into the project export folder:

```bash
cp src-tauri/target/release/bundle/deb/*.deb ../linux_export/
cp src-tauri/target/release/bundle/rpm/*.rpm ../linux_export/
cp src-tauri/target/release/bundle/appimage/*.AppImage ../linux_export/
```

Linux packages should be built on Linux. Tauri recommends Ubuntu 22.04 or Debian 12 as useful compatibility baselines for AppImage builds; see the [official AppImage guidance](https://v2.tauri.app/distribute/appimage/).

## Install A Local Build

Debian or Ubuntu:

```bash
sudo apt install ./src-tauri/target/release/bundle/deb/*.deb
```

Fedora:

```bash
sudo dnf install ./src-tauri/target/release/bundle/rpm/*.rpm
```

AppImage:

```bash
chmod +x src-tauri/target/release/bundle/appimage/*.AppImage
./src-tauri/target/release/bundle/appimage/*.AppImage
```

The `easya` terminal shortcut uses `setsid` to launch the app independently and expects an installed `easyalias` executable in `PATH`. This is provided by normal `.deb` or `.rpm` installation. For a standalone AppImage, either launch the AppImage directly or place/symlink it somewhere in `PATH` as `easyalias`.

## Troubleshooting

If a new alias is not found, first reload the active shell:

```bash
source ~/.bashrc
```

or:

```zsh
source ~/.zshrc
```

Check the detected login shell:

```bash
echo "$SHELL"
```

Inspect the generated aliases:

```bash
cat ~/.easyalias/aliases.sh
```

Check whether the source line is present:

```bash
grep -n "easyalias/aliases.sh" ~/.bashrc ~/.zshrc 2>/dev/null
```

If `easya` is not found, verify the package-installed executable:

```bash
command -v easyalias
```

## Project Structure

```text
linux_src/
  src/
    main.ts            UI state, validation, and Linux command previews
    styles.css         shared responsive styling

  src-tauri/
    src/main.rs        shell detection, first-start/manual import, setup, and persistence
    tauri.conf.json    Linux window and bundle targets
    icons/icon.png     application icon

  docs/
    ARCHITECTURE.md    technical Linux architecture
```

## License

EasyAlias is licensed under the MIT License. See `../LICENSE`.
