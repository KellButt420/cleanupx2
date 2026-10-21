# cleanup

[Website](https://cleanup-wiredin.vercel.app) · [GitHub releases](https://github.com/wiredincorp/cleanup/releases/latest) · [Discord support](https://discord.gg/5kRtCcNWU6)

A native Windows 11 application for removing applications and organizing folders.
This development build uses C#/.NET 10, WinUI 3, CommunityToolkit.Mvvm, and a Rust DLL
through a versioned C ABI. It has a native application list, confirmed sequential
uninstall, explicit leftover review, folder previews and moves, saved rules, persistent
History, and conflict-aware undo for eligible file moves.

The title bar, navigation, and workspace share Windows' Acrylic, Mica, or solid
background. Dark, light, Windows theme, system accent, and a custom accent apply live.
The palette button in the title bar opens appearance settings, including acrylic
tint presets, custom colors, tint strength and background visibility.
Application selection details and batch actions stay in the left sidebar above
Settings. The supplied green and violet SVGs live in `src/frontend/Assets`; the
window logo can be changed in Appearance. See the [latest layout update](docs/ui-layout-update.md).
The selection strip shows up to six icons and a `+N` count for additional apps.
Settings includes About information; its **luh geek** button opens the project credits
on a centered canvas without the sidebar. Back or Esc returns to Settings.

**This is a development build, not a production release.** Signed MSIX installation
and fixture workflows pass on the development machine; memory remains above target,
and clean-machine release certification is pending. See the
[latest appearance and performance audit](docs/performance-audit.md) and
[implementation report](docs/implementation-report.md).
The [original Phase 0 report](docs/phase-0-report.md) preserves the earlier snapshot.

Production packaging, signed GitHub releases, website deployment, and Windows-managed
automatic updates are documented in [launch and updates](docs/launch.md).

**Core** is the home page and opens first. It lists the drives Windows shows you (local,
removable, optical, and mapped network drives, with each network share's path). A round
chart shows the combined used and free space across ready drives, and Refresh re-reads
them; nothing watches in the background. Photos and Videos sit under Organize in the
sidebar; both are coming-soon placeholders.

Applications loads from the sidebar. Search by name or publisher; select one
application for details or several for a removal queue. Confirmation lists the apps
before their official uninstallers run. Cancelling the wait does not kill installers.
Leftovers are always a separate, initially unselected review.

Right-click an app, or use **Open** in the details panel, to start it. Store apps start
through their package; desktop apps start from the program their installer registered
(uninstallers are skipped), or from a Start Menu shortcut into their folder.

**Search in browser**, in the same menu and in the details panel, opens your default
browser on a Bing search for the app's name and publisher.

Games are listed in Applications too, under the **Games** filter. Steam and Epic Games
libraries are found automatically. Emulator games are found in the folders you add under
**Settings → Games and emulators**: each folder is searched for the emulators it contains
and for the game files they open, and PS3 titles installed in RPCS3 are read from its
dev_hdd0 folder. Games open with their launcher or emulator, and they are removed in that
launcher (or by deleting the game's files), not by cleanup.

In Organize, choose a folder, review the preview, then confirm the moves. The default
groups are **Doc, Media, Binary, Others**. Downloads, Desktop, Documents, a native
folder picker and folder drop are supported. Options in the preview provide detailed
file types, extension/date grouping, saved rules and name-conflict choices.
Optional rules match a category/extension and choose a different destination.
Preview before every execution, deselect
files to exclude them, and confirm the exact moves. Subfolders, protected folders,
installed-application locations, links, hidden/system files, partial downloads, and
online-only cloud files are preserved. History provides search, type/status filters
and a details drawer for results, interrupted batches and undo for unchanged files
whose original names remain available.

## Build on Windows x64

Install .NET SDK 10.0.402 or a compatible later .NET 10 SDK, Rust 1.99.0 MSVC,
Visual Studio 2022 C++ Build Tools, and Windows SDK 10.0.26100.0.
PowerShell 7 is required for the repository scripts.

```powershell
. .\scripts\env.ps1
.\scripts\verify-core.ps1
.\scripts\package-dev.ps1
```

If NuGet connections from .NET are blocked on the development machine, use
`scripts\package-dev.ps1 -UseLocalFeed`. It downloads packages from nuget.org
with PowerShell and restores through a local feed. It is a development fallback,
not a general NuGet resolver or release build workflow.

## Install and validate

For a real signed development MSIX install, first run this one-time setup in an
**elevated** PowerShell. It trusts the public development signer for all machine
users; the private key remains in the current user's store.

```powershell
.\packaging\Trust-DevCertificate.ps1
```

Then return to a **normal, unelevated** PowerShell:

```powershell
.\scripts\install-dev.ps1 -Mode Msix
.\scripts\phase0-feasibility.ps1 -RequireSignedPackage
```

The fixture harness installs and uninstalls its own per-user Win32, MSI, and MSIX
fixtures. Optional `-IncludeElevated` adds a per-machine fixture and requires human
UAC approval during setup, uninstall, and cleanup. Use disposable Windows environments
for this elevated integration run. The harness never approves UAC automatically.

Developer Mode provides a separate diagnostic path:

```powershell
.\scripts\install-dev.ps1 -Mode Register
.\scripts\phase0-feasibility.ps1
cleanup-dev.exe
```

Registration explicitly skips signed-install validation and uses a writable layout.
Switching from registration to a signed install removes the old development package
and its development state. Ordinary signed package updates preserve state.

When development trust is no longer needed, run `packaging\Trust-DevCertificate.ps1 -Remove`
from elevated PowerShell. The public certificate is under `artifacts\packaging`;
no private certificate is exported. Do not distribute development builds as releases.

## Repository

- `src/frontend`: WinUI application pages, view models, settings, interop, and a separate feasibility runner.
- `src/rust/crates/{core,discovery,uninstall,operations,organizer,leftovers,ffi}`: native operations and versioned ABI.
- `tests/fixtures`: disposable Win32, MSI, and MSIX applications.
- `scripts`, `packaging`: build, signing setup, install, and fixture harness.
- `docs`: decisions, ABI contract, and measured feasibility report.
- `website`: product site (Bun, Vite, Tailwind) and WinGet manifest generator. See [its README](website/README.md).
- `artifacts` (ignored): build output and machine-specific diagnostic evidence.

## Contributing and license

Licensed under [MIT](LICENSE). See [Contributing](CONTRIBUTING.md),
[Security](SECURITY.md) and [third-party notices](THIRD_PARTY_NOTICES.md).
The Windows CI workflow checks Rust and builds a development-signed MSIX with
SHA-256 checksums; it does not publish releases or validate UAC on a clean machine.
Both Cargo and NuGet dependency locks are included.

See [release preparation](docs/release-preparation.md) for the remaining public
binary release gates and [beta notes](docs/beta-notes.md) for current limitations.
