# Launch and updates

Public links live in `release.json`, shared by the app and website:
[GitHub](https://github.com/wiredincorp/cleanup),
[website](https://cleanup-wiredin.vercel.app), and
[Discord](https://discord.gg/5kRtCcNWU6). This file never contains credentials.

## Desktop downloads

The primary download is a self-contained **setup EXE** built with Inno Setup.
It installs for the current user under `%LOCALAPPDATA%\Programs\cleanup`, adds a
Start menu shortcut, and appears in Windows Installed apps. Installation needs no
administrator access, MSIX registration, Developer Mode, or certificate import.
The app includes .NET, Windows App SDK / WinUI, and Visual C++ runtime files.

The **portable ZIP** contains the same app. Extract the entire ZIP and run
`Cleanup.exe`; keep the other files beside it. Settings and history still live in
`%LOCALAPPDATA%\Cleanup`, rather than inside the extracted directory.

The target is **Windows 11 x64**. ARM64 through x64 emulation is allowed by setup
but untested. There are no macOS, Linux, Windows 10, or 32-bit builds. Compatibility
with every operating system or company policy is not promised. The preview is
unsigned, so Windows may show SmartScreen / publisher warnings; managed devices
may disallow unsigned software. Trusted signing is deferred.

## Build and verify

On the toolchain described in [development](development.md):

```powershell
./scripts/verify-core.ps1
./scripts/package-desktop.ps1 -Version 0.1.1
./scripts/verify-desktop-installer.ps1 -Version 0.1.1
```

The compiler helper downloads pinned Inno Setup 7.1.0 from its official GitHub
release and checks its SHA-256 before installing it into ignored `artifacts/tools`.
An existing compiler can be supplied with `-CompilerPath`. NuGet and Cargo use
locked dependencies. Each publish uses a fresh payload directory.

Publish only the setup EXE, portable ZIP, and `SHA256SUMS.txt` from
`artifacts/desktop/<version>`. Payloads include dependency license/notice files in
`Notices/`. Do not publish diagnostic reports, build paths, logs, or test fixtures.
The Desktop installer CI workflow builds these three artifacts without publishing
a release or accessing signing credentials.

Local verification covers installation, unpackaged WinUI startup, native ABI,
inventory scanning, disposable file moves, persistence, uninstall, and retaining
app data. A normal portable launch also checks bundled runtimes load from the
extracted directory. This is not a clean-machine or ARM64 validation result.
Complete the remaining [release checks](release-preparation.md) before a stable release.

## Updates and existing installs

Automatic updates are **not enabled** for the desktop preview. About opens GitHub
Releases. Close cleanup and run a newer setup EXE over the existing installation;
the stable installer AppId and install location allow replacement while settings
and history remain in `%LOCALAPPDATA%\Cleanup`. Uninstall also keeps that data.
For portable builds, extract a new version into a separate directory.

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
released assets. Publish all three download files, then update `downloadUrl` and
`portableUrl` in `release.json` to version-specific asset URLs. Keep WinGet
disabled until a matching package has been accepted upstream.

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