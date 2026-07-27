# EasyAlias macOS App Store

This folder contains the sandboxed EasyAlias edition intended for distribution through the Mac App Store.

It is separate from `mac_src/`, so the existing Homebrew/GitHub version keeps its automatic home-directory integration while the Store edition follows Apple's App Sandbox rules.

[Website](https://easyalias.org) | [GitHub](https://github.com/hannesgnann-hub/easyalias) | [Reddit](https://www.reddit.com/r/easyalias/)

## ❤️ Support EasyAlias

Hi, I'm Hannes, the creator of EasyAlias and a Software Engineering student.

If EasyAlias saves you time, consider supporting its development.

Your sponsorship helps me fix bugs, develop new features, and keep EasyAlias free and open source.

[Become a GitHub Sponsor](https://github.com/sponsors/hannesgnann-hub)

## Store Edition Differences

| Homebrew/GitHub edition | Mac App Store edition |
| --- | --- |
| reads and updates `~/.zshrc` automatically | asks the user to select `.zshrc` |
| stores files under `~/.easyalias/` | stores app data and backups in the App Sandbox container |
| generates `~/.easyalias/aliases.zsh` | writes a clearly marked managed block into the selected `.zshrc` |
| installs the `easya` application alias | does not create an application-launch alias |
| unrestricted local desktop build | sandboxed and signed App Store build |

The Store edition never scans the home directory on its own. A standard macOS file picker grants initial access to one `.zshrc`, and the backend immediately persists that permission as an app-scoped security bookmark.

When the user changes the connected file, EasyAlias first backs up the previous `.zshrc`, removes its managed block, and drops the old bookmark.

## First Start

1. Open EasyAlias.
2. Click **Choose .zshrc**.
3. If hidden files are not visible, press `Command-Shift-.` in the picker.
4. Select the `.zshrc` file.
5. Review any existing aliases offered for import.

EasyAlias then owns only this block:

```zsh
# >>> EasyAlias managed aliases >>>
# Managed by EasyAlias. Edit these aliases in the app.
alias ll='ls -lah'
# <<< EasyAlias managed aliases <<<
```

All unrelated `.zshrc` content remains outside the block.

## Local Data

The App Sandbox container stores:

```text
config.json
zshrc.bookmark
.zshrc-import-v1
backups/zshrc-<timestamp>.backup
```

The exact container path is resolved by Tauri at runtime and shown in the UI. The security-scoped bookmark contains the persistent permission for the selected `.zshrc`.

## Development Setup

VS Code is enough as the editor. Building requires:

| Tool | Purpose |
| --- | --- |
| Node.js + npm | TypeScript frontend and Vite build |
| Rust + Cargo | Tauri backend |
| Xcode | macOS SDK, signing, `productbuild`, and upload tools |
| Apple Developer membership | App Store certificates and provisioning |

Check the toolchain:

```zsh
node -v
npm -v
rustc --version
cargo --version
xcodebuild -version
```

Install dependencies and run the code checks:

```zsh
cd mac_src_app_store
npm install
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

`npm run dev` is only a browser preview and uses `localStorage`. It cannot test App Sandbox permissions or security-scoped bookmarks.

## Apple Identity

The registered values used by this source project are:

```text
Bundle ID: dev.hannesgnann.easyalias
Team ID:   ZAYL5AN372
Category:  DeveloperTool
Version:   1.0.0
Build:     2
```

The Bundle ID in Apple Developer and App Store Connect must match exactly.

Check installed signing identities:

```zsh
security find-identity -v -p codesigning
```

For App Store distribution, install identities corresponding to:

```text
Apple Distribution: Hannes Gnann (ZAYL5AN372)
3rd Party Mac Developer Installer: Hannes Gnann (ZAYL5AN372)
```

Use the exact names printed by `security find-identity`; Apple may display certificate names slightly differently.

## Provisioning Profile

In Apple Developer:

1. Register the explicit App ID `dev.hannesgnann.easyalias`.
2. Create or install the Apple Distribution certificate.
3. Create a **Mac App Store Connect** provisioning profile for that App ID.
4. Download the profile.
5. Place it here:

```text
src-tauri/EasyAlias_AppStore.provisionprofile
```

The profile is ignored by Git and must never be committed.

## Store Configuration

| File | Purpose |
| --- | --- |
| `src-tauri/tauri.conf.json` | shared app identity, window, and bundle configuration |
| `src-tauri/tauri.sandbox.conf.json` | local ad-hoc sandbox bundle without Store identity fields |
| `src-tauri/tauri.appstore.conf.json` | final Store bundle with provisioning profile |
| `src-tauri/Entitlements.local.plist` | local sandbox permissions |
| `src-tauri/Entitlements.plist` | final Store permissions plus Team ID and Application ID |
| `src-tauri/Info.plist` | encryption export declaration |

The provisioning profile is embedded as:

```text
EasyAlias.app/Contents/embedded.provisionprofile
```

Build an ad-hoc-signed local bundle to validate configuration and inspect its sandbox entitlements:

```zsh
npm run store:build:sandbox
```

This checks packaging but is not an App Store submission artifact.

## Build the App Bundle

Install both Rust targets once for a universal Intel + Apple Silicon build:

```zsh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
```

Build with the Apple Distribution identity:

```zsh
export APPLE_SIGNING_IDENTITY="Apple Distribution: Hannes Gnann (ZAYL5AN372)"
npm run store:build
```

The app bundle is generated at:

```text
/private/tmp/easyalias-appstore-target/universal-apple-darwin/release/bundle/macos/EasyAlias.app
```

The final Store build deliberately uses a target directory outside Desktop and other File Provider locations. Otherwise macOS can attach `com.apple.FinderInfo` while Tauri is still assembling the bundle, causing code-signing error `resource fork, Finder information, or similar detritus not allowed`.

## Verify the Sandbox

Inspect the signature and embedded entitlements:

```zsh
codesign --verify --deep --strict --verbose=2 \
  /private/tmp/easyalias-appstore-target/universal-apple-darwin/release/bundle/macos/EasyAlias.app

codesign -d --entitlements - \
  /private/tmp/easyalias-appstore-target/universal-apple-darwin/release/bundle/macos/EasyAlias.app
```

The output must include at least:

```text
com.apple.security.app-sandbox
com.apple.security.network.client
com.apple.security.files.user-selected.read-write
com.apple.security.files.bookmarks.app-scope
```

Test the signed app before creating the installer:

1. Start the app.
2. Select a test `.zshrc`.
3. Create an alias.
4. Quit and reopen EasyAlias.
5. Edit the alias without selecting `.zshrc` again.
6. Open a fresh terminal and verify the alias.

Use a temporary macOS user account or a test `.zshrc` during this validation.

## Create the Signed PKG

Set the exact installer identity shown by Keychain:

```zsh
export INSTALLER_IDENTITY="3rd Party Mac Developer Installer: Hannes Gnann (ZAYL5AN372)"
```

Remove Finder metadata and other extended attributes from the signed app bundle before packaging. This prevents App Store validation error `90303`:

```zsh
APP="/private/tmp/easyalias-appstore-target/universal-apple-darwin/release/bundle/macos/EasyAlias.app"
xattr -cr "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"
```

Create the App Store package:

```zsh
xcrun productbuild \
  --sign "$INSTALLER_IDENTITY" \
  --component "$APP" \
  /Applications \
  EasyAlias.pkg
```

Verify it:

```zsh
pkgutil --check-signature EasyAlias.pkg
```

## Upload

The simplest manual upload is Apple's **Transporter** app. Drop `EasyAlias.pkg` into Transporter and submit it to App Store Connect.

Tauri also documents an API-key upload:

```zsh
xcrun altool \
  --upload-app \
  --type macos \
  --file EasyAlias.pkg \
  --apiKey "$APPLE_API_KEY_ID" \
  --apiIssuer "$APPLE_API_ISSUER"
```

After Apple processes the package, test it through TestFlight before submitting it for review.

## Safety Guarantees

- no automatic home-directory scan
- no `.zshrc` access before explicit user selection
- persistent access uses an app-scoped security bookmark
- security scope starts only around each file operation and always stops afterward
- malformed or duplicate managed markers abort instead of rewriting the file
- imports are parsed as text and never executed
- an App Sandbox container backup is created before connection setup and imports
- the `easya` application alias is not generated

## Architecture

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the command flow, sandbox boundaries, bookmark lifecycle, and managed-block format.

## Official References

- [Tauri App Store distribution](https://v2.tauri.app/distribute/app-store/)
- [Tauri macOS application bundles](https://v2.tauri.app/distribute/macos-application-bundle/)
- [Tauri macOS code signing](https://v2.tauri.app/distribute/sign/macos/)
- [Apple: Accessing files from the macOS App Sandbox](https://developer.apple.com/documentation/security/accessing-files-from-the-macos-app-sandbox)
- [Apple: Creating an App Store provisioning profile](https://developer.apple.com/help/account/provisioning-profiles/create-an-app-store-provisioning-profile)

## License

EasyAlias is licensed under the MIT License. See `LICENSE`.
