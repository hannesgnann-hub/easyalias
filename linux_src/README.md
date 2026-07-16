# EasyAlias Linux

EasyAlias Linux is a Tauri desktop app for creating and managing terminal aliases through a UI.

It detects bash or zsh, keeps the alias data in its own directory, and connects one generated shell file to the matching startup file.

## Highlights

- create, edit, and delete aliases through a UI
- choose files and folders with the native Linux picker
- preview the generated shell command before saving
- store `createdAt` and `updatedAt` for every alias
- automatically detect bash or zsh from `$SHELL`
- generate `~/.easyalias/aliases.sh`
- connect the generated file to `~/.bashrc` or `~/.zshrc`
- add the `easya` shortcut for opening the installed application
- build `.deb`, `.rpm`, and `.AppImage` packages

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
```

On first native startup it appends the missing lines to the detected startup file:

```bash
# EasyAlias aliases
source ~/.easyalias/aliases.sh

# EasyAlias app shortcut
alias easya='setsid -f easyalias >/dev/null 2>&1'
```

Existing shell configuration is preserved. EasyAlias only appends lines that are not already present.

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
    src/main.rs        shell detection, setup, and local file persistence
    tauri.conf.json    Linux window and bundle targets
    icons/icon.png     application icon

  docs/
    ARCHITECTURE.md    technical Linux architecture
```

## License

EasyAlias is licensed under the MIT License. See `LICENSE`.
