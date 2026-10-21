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

[Download Setup EXE](https://github.com/wiredincorp/cleanup/releases/download/v0.1.1-rc.1/cleanup-0.1.1-windows-x64-setup.exe) · [Portable ZIP](https://github.com/wiredincorp/cleanup/releases/download/v0.1.1-rc.1/cleanup-0.1.1-windows-x64-portable.zip)

Run the installer, or extract the ZIP and open `Cleanup.exe`. The required runtimes are included—no MSIX setup, certificate import, or separate .NET installation.

**Windows 11 x64.** This preview is unsigned, so Windows may show a publisher or SmartScreen warning. ARM64 emulation is unverified; macOS/Linux builds are unavailable. Automatic updates are not enabled yet.

## Build it

```powershell
. .\scripts\env.ps1
.\scripts\verify-core.ps1
.\scripts\package-desktop.ps1 -Version 0.1.1
```

See the [development guide](docs/development.md) for pinned toolchain versions, installation, and validation.

## Around the project

[Contributing](CONTRIBUTING.md) · [Launch & updates](docs/launch.md) · [Known limitations](docs/beta-notes.md) · [Security](SECURITY.md)

Licensed under [MIT](LICENSE). [Third-party notices](THIRD_PARTY_NOTICES.md).

©(wired-in) community
