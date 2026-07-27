# Microsoft Store Release

This source tree creates the Win32 NSIS installer submitted through the
Microsoft Store's **EXE or MSI app** workflow. It deliberately does not create
an MSIX package: EasyAlias needs normal access to the user's profile and user
`PATH` so generated `.cmd` aliases remain available to `cmd.exe`.

## Product Identity

The current Partner Center product uses:

| Property | Value |
| --- | --- |
| Product | EasyAlias |
| Store ID | `9PLV8CXDWW75` |
| Publisher display name | `Hannes Gnann` |

The MSIX package identity shown in Partner Center is public metadata. It is not
a code-signing key and is not required by this Win32 NSIS build.

## Build Requirements

Build the release on Windows with:

- Node.js and npm
- Rust with the stable MSVC toolchain
- Microsoft C++ Build Tools
- internet access during the build so Tauri can obtain the WebView2 offline
  installer

The resulting EasyAlias installer itself is offline and contains everything
needed for installation.

## Build

Run in PowerShell from `windows_src_store`:

```powershell
npm ci
npm run store:build
```

The unsigned NSIS installer is written below:

```text
src-tauri\target\release\bundle\nsis\
```

Its expected name follows this pattern:

```text
EasyAlias_1.0.0_x64-setup.exe
```

## Test The Installer

Microsoft Store certification requires a silent installation. Tauri's NSIS
installer uses the uppercase `/S` argument:

```powershell
.\EasyAlias_1.0.0_x64-setup.exe /S
```

After installation:

1. Start EasyAlias once.
2. Create a harmless test alias such as `easystoretest`.
3. Open a new `cmd.exe` window.
4. Run `where easystoretest`.
5. Run the alias and verify its output.

Also test silent uninstall using the uninstaller created in the EasyAlias
installation directory:

```powershell
& "$env:LOCALAPPDATA\EasyAlias\uninstall.exe" /S
```

Confirm the exact installation directory on the build machine before entering
the uninstall command in Partner Center.

## Signing

Before submission, the installer and application executable must be signed
with an Authenticode code-signing certificate trusted by Windows. The public
Partner Center product identity is not such a certificate.

After signing, verify both signatures:

```powershell
Get-AuthenticodeSignature .\EasyAlias_1.0.0_x64-setup.exe |
  Format-List Status, StatusMessage, SignerCertificate
```

The final `Status` must be `Valid`.

## Hosting And Submission

Upload the signed installer to an immutable, versioned HTTPS URL. Never replace
the file behind a URL already submitted to Microsoft. A suitable layout is:

```text
https://easyalias.org/downloads/windows/1.0.0/EasyAlias_1.0.0_x64-setup.exe
```

In Partner Center, submit EasyAlias as an **EXE or MSI app** and enter:

| Field | Value |
| --- | --- |
| Installer URL | versioned HTTPS URL |
| Silent install parameter | `/S` |
| Architecture | `x64` |
| Installer type | `.exe` |

Complete the listing, age rating, privacy/support URLs, screenshots, and
certification notes before publishing.

## Release Checklist

- [ ] `npm ci` succeeds on Windows
- [ ] `npm run store:build` creates the offline NSIS installer
- [ ] installer and bundled PE files are Authenticode-signed
- [ ] signature status is `Valid`
- [ ] `/S` installs without an installer UI
- [ ] EasyAlias starts and creates `.cmd` aliases
- [ ] a new `cmd.exe` resolves generated aliases
- [ ] `/S` uninstalls without an uninstaller UI
- [ ] signed installer is hosted at an immutable HTTPS URL
- [ ] Partner Center listing and certification information are complete
