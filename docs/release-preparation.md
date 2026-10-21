# Release preparation

cleanup is MIT licensed. Dependency and runtime licenses still apply. Desktop
payloads include exact restored NuGet and Cargo license/notice files in `Notices/`.
See [launch and updates](launch.md) for packaging, links, and publishing.

## Preview distribution

The unsigned desktop preview ships as a per-user setup EXE and a portable ZIP.
It does not require the MSIX development certificate. Windows may warn about an
unknown publisher, and managed device policies can block unsigned binaries.
Code signing is deferred; do not describe this preview as signed or trusted.

Before publishing each preview:

1. Run Rust formatting, strict Clippy, and tests; build using locked dependencies.
2. Run `verify-desktop-installer.ps1` in a disposable Windows user or environment.
   It refuses to replace an existing desktop installation. Confirm normal portable
   launch and bundled runtime loading as well as the installed-app self-test.
3. Review license/notice coverage, public copy, and final SHA-256 checksums.
4. Upload only EXE, ZIP, and checksums to an immutable prerelease tag. Keep
   `artifacts/`, private certificates, local settings, diagnostic reports, and
   fixtures out of source and release assets. Reports can contain app names/paths.
5. Verify published download links before deploying the website.

Local checks have covered install, launch, native loading, inventory, disposable
file moves, persistence, uninstall, and retaining data. A development-machine pass
does not prove a clean Windows installation works without preinstalled runtimes.

## Before a stable launch

- Validate setup and portable builds on a clean Windows 11 x64 VM with no developer
  tools, .NET, or Windows App SDK preinstalled, using a standard user account.
- Test upgrading between two desktop releases with settings/history retained,
  uninstall/reinstall, existing MSIX coexistence, offline launch, paths with spaces,
  and interrupted installs. Only data preservation on uninstall is verified so far.
- Choose a trusted signing method, sign installer and binaries, and verify signatures.
  Keep credentials in protected secret storage outside source control.
- Exercise uninstall fixtures, including human-approved UAC consent/refusal and
  MSI busy/restart outcomes. Never target real installed applications for tests.
- Complete filesystem failure and accessibility checks, and measure startup/memory
  on named hardware. Existing observations do not justify the original 120 MB claim.
- Test ARM64 separately before advertising compatibility. macOS/Linux and 32-bit
  Windows are outside this native WinUI build.

Automatic desktop updating remains future work. The optional signed MSIX workflow
and App Installer feed must be validated independently if that channel is revived.

## CI

Official GitHub actions are pinned to commit hashes. Desktop installer builds the
EXE/ZIP/checksums on Windows x64. The existing Windows workflow checks the Rust core
and builds a separate development MSIX; Website checks types, tests, and static
build. A local build is not proof that a hosted runner job has passed.