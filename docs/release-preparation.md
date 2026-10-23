# Release preparation

cleanup is MIT licensed. Dependency and runtime licenses still apply. Desktop
payloads include exact restored NuGet and Cargo license/notice files in `Notices/`.
See [launch and updates](launch.md) for packaging, links, and publishing.

## Preview distribution

The unsigned desktop preview ships as a portable `Cleanup.exe`, with a per-user
setup EXE as fallback. Neither requires the MSIX development certificate.
Code signing is deferred; do not describe this preview as signed or trusted.
Unsigned downloads carry Mark of the Web, so SmartScreen shows "Windows protected
your PC" until the user chooses More info → Run anyway. Smart App Control, when
on (common on fresh Windows 11 installs), blocks unsigned apps with no override,
and managed device policies can block them too; neither download can work around
this. Trusted signing is required to identify the publisher and pass Smart App
Control, but it cannot guarantee that SmartScreen stops warning for a new file
before reputation builds.

The unpackaged WinUI publish output has hundreds of support files and language
resources. Never prune WinUI or Windows App SDK resources to make a download look
smaller. The portable EXE keeps all of them inside a compressed .NET single-file
bundle and extracts them under `%TEMP%\.net\Cleanup` on first launch; Setup keeps
them in the install directory.

Before publishing each preview:

1. Run Rust formatting, strict Clippy, and tests; build using locked dependencies.
2. Run `verify-portable.ps1` and `verify-desktop-installer.ps1` in a disposable
   Windows user or environment. The installer check refuses to replace an existing
   desktop installation.
3. Review license/notice coverage, public copy, and final SHA-256 checksums.
4. Upload only `Cleanup.exe`, setup EXE, `Cleanup-Notices.zip`, and checksums to an immutable prerelease tag. Keep
   `artifacts/`, private certificates, local settings, diagnostic reports, and
   fixtures out of source and release assets. Reports can contain app names/paths.
5. Verify published download links before deploying the website.

Local checks have covered install, launch, native loading, inventory, disposable
file moves, persistence, uninstall, and retaining data. A development-machine pass
does not prove a clean Windows installation works without preinstalled runtimes.

## Before a stable launch

- Validate the portable EXE and setup on a clean Windows 11 x64 VM with no developer
  tools, .NET, Windows App SDK, or Visual C++ redistributable preinstalled, using a
  standard user account, a real browser download (Mark of the Web), and Smart App
  Control in its default state. `scripts/test-in-sandbox.ps1` automates the portable
  and setup self-tests in Windows Sandbox (free on Windows 11 Pro; enable
  `Containers-DisposableClientVM` once). Sandbox runs as an administrator, so it
  does not replace a standard-user check.
- Test upgrading between two desktop releases with settings/history retained,
  uninstall/reinstall, existing MSIX coexistence, offline launch, paths with spaces,
  and interrupted installs. Only data preservation on uninstall is verified so far.
- Choose a trusted signing method, sign installer and binaries, and verify signatures.
  Keep credentials in protected secret storage outside source control. The no-cost
  route for this MIT project is the SignPath Foundation open-source program
  (application and review required; signing then runs from GitHub Actions).
  Sign `cleanup_native.dll` and the app before bundling, then the bundled
  `Cleanup.exe`, setup, and uninstaller.
- Exercise uninstall fixtures, including human-approved UAC consent/refusal and
  MSI busy/restart outcomes. Never target real installed applications for tests.
- Complete filesystem failure and accessibility checks, and measure startup/memory
  on named hardware. Existing observations do not justify the original 120 MB claim.
- Test ARM64 separately before advertising compatibility. macOS/Linux and 32-bit
  Windows are outside this native WinUI build.

Automatic desktop updating remains future work. The optional signed MSIX workflow
and App Installer feed must be validated independently if that channel is revived.

## CI

Official GitHub actions are pinned to commit hashes. Desktop installer CI builds the
portable EXE, setup EXE, notices, and checksums on Windows x64. The existing Windows workflow checks the Rust core
and builds a separate development MSIX; Website checks types, tests, and static
build. A local build is not proof that a hosted runner job has passed.
