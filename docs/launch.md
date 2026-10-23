# Launch and updates

Public links live in `release.json`, shared by the app and website:
[GitHub](https://github.com/wiredincorp/cleanup),
[website](https://cleanup-wiredin.vercel.app), and
[Discord](https://discord.gg/5kRtCcNWU6). This file never contains credentials.

## Desktop downloads

The primary download is a single portable **`Cleanup.exe`**. Download it and run
it: no installer, administrator access, MSIX registration, Developer Mode,
certificate import, or separately installed .NET, Windows App SDK, or Visual C++
runtime is required. It is a compressed .NET single-file bundle. The .NET runtime
is linked into the executable; on first launch the WinUI / Windows App SDK native
files, resources, and Rust core (about 490 files, 156 MB) are extracted to
`%TEMP%\.net\Cleanup\<bundle id>` and reused by later launches. The download
folder stays clean, but this is not a no-extraction single file. Each version
extracts to a new bundle-id directory; on launch the app removes older ones in the
background, skipping any that a running copy still uses. Settings and history live in
`%LOCALAPPDATA%\Cleanup`, not beside the EXE, so moving or replacing `Cleanup.exe`
keeps them. The app is installation-free, not USB-roaming.

The fallback is a self-contained per-user **setup EXE** built with Inno Setup. It
installs under `%LOCALAPPDATA%\Programs\cleanup`, adds a Start menu shortcut, and
appears in Windows Installed apps. Use it where policy blocks executables extracted
under `%TEMP%`, or when a Start menu entry and uninstaller are preferred. Settings
and history remain after uninstall.

Both downloads include .NET and Windows App SDK / WinUI. The Rust core links the
MSVC C runtime statically, and Windows App SDK binaries import only the Universal
CRT built into Windows 10 and 11, so no Visual C++ redistributable is shipped or
needed. Packaging fails if any shipped binary imports it again.

The target is **Windows 11 x64**. ARM64 through x64 emulation is allowed by setup
but untested. There are no macOS, Linux, Windows 10, or 32-bit builds. Compatibility
with every operating system or company policy is not promised. The preview is
unsigned, so Windows may show SmartScreen / publisher warnings; managed devices
may disallow unsigned software. Trusted signing is deferred.

## Build and verify

On the toolchain described in [development](development.md):

```powershell
./scripts/verify-core.ps1
./scripts/package-desktop.ps1 -Version 0.1.2
./scripts/verify-portable.ps1 -Version 0.1.2
./scripts/verify-desktop-installer.ps1 -Version 0.1.2
```

The compiler helper downloads pinned Inno Setup 7.1.0 from its official GitHub
release and checks its SHA-256 before installing it into ignored `artifacts/tools`.
An existing compiler can be supplied with `-CompilerPath`. NuGet and Cargo use
locked dependencies. Each publish uses a fresh payload directory.

Publish only `Cleanup.exe`, the setup EXE, `Cleanup-Notices.zip`, and
`SHA256SUMS.txt` from `artifacts/desktop/<version>`. The notices archive (also
installed by Setup under `Notices/`) holds the exact restored NuGet, .NET runtime,
Inno Setup, and Cargo license/notice files. Do not publish diagnostic reports,
build paths, logs, or test fixtures. The Desktop installer CI workflow builds
these artifacts without publishing a release or accessing signing credentials.

Portable verification runs `Cleanup.exe` with an empty extraction directory and
checks first-launch extraction, unpackaged WinUI startup, native ABI, inventory
scanning, disposable file moves with undo, AppData state, and the absence of any
Visual C++ redistributable dependency. Installer verification covers install, the
same app checks, persistence, uninstall, and retaining app data. Neither is a
clean-machine or ARM64 validation result.
Complete the remaining [release checks](release-preparation.md) before a stable release.

## Updates and existing installs

Automatic updates are **not enabled** for the desktop preview. Settings → Check for
updates asks the GitHub releases API for a newer published tag, only when clicked;
the app makes no background network requests. For the portable app, close cleanup and replace `Cleanup.exe` with the
newer download. For Setup, close cleanup and run a newer setup EXE over the existing installation;
the stable installer AppId and install location allow replacement while settings
and history remain in `%LOCALAPPDATA%\Cleanup`. Uninstall also keeps that data.

The desktop app has its own state and identity. It does not migrate data from
`Cleanup.Dev` or the old MSIX candidate. Existing MSIX installations can remain
installed while trying the desktop app.

The earlier MSIX packaging scripts and Signed Windows release workflow remain
optional. That path requires trusted signing and validation of two releases
before enabling App Installer updates. It is not the primary download.
Keep `appInstallerUrl` empty for the desktop preview. A secure desktop auto-updater
remains future work.

## Publish and deploy

Use an immutable GitHub prerelease tag such as `v0.1.1-rc.1`; do not overwrite
released assets. The **Desktop release draft** workflow builds, verifies, attests
provenance, and attaches the four release files to a draft prerelease; review and
publish it by hand. Then set `downloadUrl` (the `Cleanup.exe` asset, the website's
main button) and `setupUrl` (the setup asset) in `release.json` to version-specific
asset URLs. Never point them at assets that are not yet published.
Keep WinGet disabled until a matching package has been accepted upstream.

Vercel builds `website` with Bun and source files outside the root enabled so it
can read `release.json`. `website/vercel.json` defines build, output, caching, and
security headers. Website CI independently checks types, tests, and build.
Production deployments can be created explicitly from GitHub `main`.
Persistent automatic Git deployment still needs the repository connected under
the existing Vercel project's Settings → Git; the connector's linking attempt
returned a project-name conflict.

SEO is generated at build time: canonical URLs, absolute social images, software
metadata, robots.txt, and sitemap.xml. Preview builds use noindex. `siteUrl` must
use the stable production domain.
