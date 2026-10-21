<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="website/public/logo-dark.svg">
    <img src="website/public/logo.svg" alt="" width="64" height="64">
  </picture>
  <h1>cleanup</h1>
  <p>A little less clutter. A lot more room.</p>
  <p>A native Windows 11 app for removing applications and organizing folders.</p>
  <p>
    <a href="https://cleanup-wiredin.vercel.app">Website</a> ·
    <a href="https://github.com/wiredincorp/cleanup/releases">Releases</a> ·
    <a href="https://discord.gg/5kRtCcNWU6">Discord</a>
  </p>
</div>

## A place for everything

- **See your space.** View used and free storage across your drives.
- **Let apps go.** Find applications, review your selection, and run their official uninstallers. Review leftovers separately.
- **Sort the pile.** Preview folder organization, choose what moves, and save rules for next time.
- **Keep a clear history.** Review results and undo eligible file moves.

Made with WinUI 3 and Rust, with light and dark themes that feel at home on Windows.

## Try the preview

[0.1.0-rc.1 is available on GitHub](https://github.com/wiredincorp/cleanup/releases/tag/v0.1.0.0-rc.1).
The MSIX is **unsigned** and cannot be installed through the normal trusted installation flow.
Production signing and automatic updates are still being prepared.

To run cleanup now, [build a development version](docs/development.md).
You'll need Windows 11 x64, PowerShell 7, .NET 10, Rust MSVC, Visual Studio C++ Build Tools, and the Windows SDK.

## Build it

```powershell
. .\scripts\env.ps1
.\scripts\verify-core.ps1
.\scripts\package-dev.ps1
```

See the [development guide](docs/development.md) for pinned toolchain versions, installation, and validation.

## Around the project

[Contributing](CONTRIBUTING.md) · [Launch & updates](docs/launch.md) · [Known limitations](docs/beta-notes.md) · [Security](SECURITY.md)

Licensed under [MIT](LICENSE). [Third-party notices](THIRD_PARTY_NOTICES.md).

©(wired-in) community
