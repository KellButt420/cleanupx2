# Phase 0 feasibility report

This is the original Phase 0 snapshot. Signed installation and the native application
UI were subsequently implemented. See [the current report](implementation-report.md)
for current behavior and validation; the blockers below describe the earlier run.

Date: 2026-10-20. **Status: implemented proof, acceptance incomplete.**
Do not advance to Phase 1 until the signed-install and interactive elevation gates
below have been validated in a disposable Windows environment.

## Implementation

- Native WinUI 3 XAML diagnostic window with explicit scan, cancel, probes, and
  fixture-only removal after confirmation. This is intentionally a feasibility UI.
- Rust workspace with `core`, `discovery`, `uninstall`, and `ffi` crates. Future crates
  are not stubbed out or represented as finished features.
- Win32 discovery reads HKLM 32/64-bit uninstall views and shared HKCU entries.
  MSI discovery uses `MsiEnumProductsExW` and `MsiGetProductInfoExW`; overlap is merged
  only by product identity, never by display name. MSIX discovery uses PackageManager
  for the current user's eligible packages and excludes system/framework/resource packages.
- ABI 1, UTF-8 JSON batches, opaque operation handles, source-generated P/Invoke,
  Rust buffer release, cancellation, typed errors, and panic containment.
- Supported official execution strategies: validated registered `.exe` uninstallers,
  system `msiexec /x {product-code} /passive /norestart`, and PackageManager removal.
  Normal processes are started suspended and assigned to a job before resuming,
  allowing the fixture's temporary child uninstaller to finish before verification.
  MSI uses its documented command interface for this proof; direct MSI API execution
  is not implemented. Registry strings never go to a shell.
- Signed x64 development MSIX build; package-local Rust DLL, .NET runtime payload,
  Windows App Runtime framework dependency, package LocalState persistence.
- Disposable Win32 (user and optional machine), per-user MSI, and loose MSIX fixtures;
  independent verification checks actual fixture registration and file removal.

The main paths are `src/frontend`, `src/rust`, `tests/fixtures`, `scripts`,
`packaging`, and `docs`. [ABI details](ffi-contract.md) and
[packaging decision](adr/0002-msix-packaging-and-signing.md) describe the contracts.

## Toolchain and compatibility

Validated build environment:

- Windows x64, build 26200 (Windows 11; the legacy product string reports Windows 10).
- .NET SDK 10.0.402 installed in the user's `.dotnet` folder; runtime 10.0.13 ships
  self-contained. The machine-wide SDK initially only had .NET 6/8.
- Rust 1.99.0, MSVC ABI; Visual Studio Build Tools 2022 and Windows SDK 10.0.26100.0.
- Windows App SDK stable release 2.5.1. Component references: WinUI 2.3.9,
  InteractiveExperiences 2.1.9, Runtime 2.5.1. These components are the SDK's
  packaging layout; the application uses native WinUI 3, with no web interface.
- Minimum manifest OS: Windows 11 build 22000. Only build 26200 has been exercised.
  ARM64 is not implemented or tested.

The WinUI component requests an unpublished InteractiveExperiences 2.1.8 minimum;
2.1.9 is explicitly pinned, matching the official SDK metapackage. Selecting the
required components avoids pulling in unrelated AI, ML, Search, and Widgets packages.

The user-local .NET 10 runtime could not open outbound network sockets during
bootstrap (`WSAEACCES`), while PowerShell could reach nuget.org. No firewall or security
setting was changed. `restore-via-local-feed.ps1` downloads dependencies using PowerShell
and lets NuGet perform the actual restore. Its resolver is limited to this dependency
graph, and offline mode disables NuGet vulnerability auditing; it is not proof that
dependencies are vulnerability-free. Normal connected release builds must use NuGet
directly and perform their dependency/security checks.

## Executed checks and evidence

Latest complete integration run:
`artifacts/phase0/20261020-031614/{run-a.json,run-b.json,summary.json}`.
The files contain machine paths and installed application metadata, so artifacts are
ignored by source control. They are local evidence, not redistributable sample inventories.

1. Release MSIX: **build passed, 0 warnings, 0 errors**. Output:
   `artifacts/packages/Cleanup_0.0.1.0_x64_Test/Cleanup_0.0.1.0_x64.msix`.
2. Rust tests: **54 passed**, including registry normalization, product IDs, duplicates,
   command rejection, FFI buffer ownership, errors/panics, cancellation, process-tree
   waiting, diagnostic persistence, and no-overwrite fixture moves. Clippy with
   warnings denied and rustfmt checks passed.
3. Developer Mode registration: **passed**, identity
   `Cleanup.Dev_0.0.1.0_x64__kfjrmgg4yenmj`. Install location is the checkout's
   `artifacts/layout/Cleanup.Dev`, with `IsDevelopmentMode=True`.
4. Native window and P/Invoke: **passed**. The XAML root loaded, ABI reported 1,
   native module path was inside the registered package, and elevation was false.
   Earlier rendered-window evidence is in `artifacts/phase0/run1/window.png`.
5. Inventory: **passed**. Latest run returned 488 records in four batches,
   including all three fixtures. Reported source errors were empty.
6. Sequential official removal: **passed for fixtures only**. Per-user MSI, Win32,
   and MSIX registrations disappeared and the independent verifier confirmed removal.
   Both Win32 uninstaller stages reported no package identity and no elevation.
   The ordinary Win32 job observed two processes; primary-process exit alone was
   not reported as completion. The final inventory contained no fixtures.
7. Persistence: **passed across two process launches**, in package LocalState.
   This is a flushed diagnostic JSONL launch log, not the future transaction journal.
8. Fixture file operations: **passed** for a custom directory under artifacts.
   Unicode filename move, attempted collision without overwrite, preservation of
   both existing files, and move-back were checked. The test harness cleaned its
   installed fixture registrations and leftover data after verification.
9. Known-folder resolution: **passed** for Downloads and Desktop through
   `SHGetKnownFolderPath`. Native folder-picker selection, redirected/cloud folders,
   and moves inside Downloads/Desktop have **not** been validated.
10. Signed installation into WindowsApps: **blocked**. The self-signed development
    signer was not trusted machine-wide. Previous normal deployment failed with
    underlying `0x800B0109` (untrusted root). A fresh UAC request for the narrow
    TrustedPeople setup ended with **“The operation was canceled by the user.”**
    No machine-wide development trust was added. The app remains registered from
    a development layout; this does not satisfy signed-install acceptance.
11. Elevation: the explicit `runas` path, UAC cancellation mapping, and narrow
    machine fixture are implemented; successful elevated fixture uninstall and
    declined-consent behavior from the installed app are **not yet tested**.
    No privileged helper/service is present or currently justified.

The harness now defaults installation to signed MSIX and requires explicit `Register`
for the development alternative. `-RequireSignedPackage` rejects development layouts
before creating fixtures. It never clicks unrelated installer dialogs or approves UAC.
There is no C# unit-test project yet; managed interop was exercised by the packaged
feasibility runner. Clean-machine framework deployment, update, Store certification,
and release signing have not been validated.

## Measurements

Hardware: AMD Ryzen 7 5800X3D (16 logical processors), approximately 64 GB RAM.
The checkout and registered package were on a WDC WDS100T2B0A SATA SSD; the machine
also has WD_BLACK SN850X NVMe drives. These are development-machine observations,
not a controlled benchmark or cold-cache measurement.

- Process start to XAML root Loaded: **655 ms, 424 ms**. This excludes shell activation
  delay before process creation and does not prove the 2-second cold-start target.
- Inventory: **550 ms, 589 ms**, 488 records. Source timings are in each report.
- Idle after a 3-second settle, sampled for 5 seconds: **140.1 MiB working set**,
  **93.5 MiB private memory**, **0.000% CPU** at recorded precision.
  The 120 MB working-set target is **not met**. Do not claim near-zero CPU outside
  this short sample or extrapolate this proof to the eventual complete UI.
- Signed package size: **44.9 MiB**; unpacked layout approximately **115 MiB**, mostly
  the self-contained .NET runtime. Framework dependencies add a separate download
  when not already installed.
- MSI fixture removal: **7.57 s**. Win32 fixture removal: **48.62 s**, with the
  fixture's own directory deletion taking most of that time. The core completed
  **15 ms** after the fixture's final diagnostic timestamp. Isolating Win32 in the
  previous run took **1.66 s**. The filesystem delay following MSI removal remains
  unexplained; no security-product exclusions or system changes were attempted.

Package SHA-256:
`BC5D25D2AA3CBE380749503A2A6EC9EE57645454A96E31E80C38999DE8A852AA`.
This identifies the local development artifact, not a published release.

## Safety and packaging boundaries

The proof writes only diagnostic app data and namespaced fixtures. It does not
implement leftover deletion or general organization of user files. The diagnostic
UI permits removal only of the exact known fixture IDs and expected publisher.
Installed metadata remains untrusted; re-reading and validation mitigate stale plans
but do not eliminate filesystem races. The executable-host block list is conservative
validation, not a sandbox or guarantee of publisher trust. File/registry cleanup,
handle-based identity validation, cloud/offline handling, long-path operations, and
complete operation authorization require later safety design and tests.

Full-trust packaging grants package identity, not administrator rights. Registry and
AppData virtualization can affect writes, so enumeration success must not be used as
proof that future leftover deletion reaches the intended real registry/filesystem.
The fixture uninstallers demonstrated an external process without inherited package
identity in Developer Mode; the same assertion must be repeated after signed install.
Machine MSI servicing and elevation still require validation in a disposable VM.
Elevated processes may escape the job tracker, and source verification can correctly
return uncertain completion. No uninstall restoration is promised.

## Exact next steps

1. Provision a disposable clean Windows 11 x64 VM with snapshots. Do not use real
   applications or personal files for destructive integration testing.
2. Build with `scripts/package-dev.ps1` (use `-UseLocalFeed` only if necessary).
3. In an elevated PowerShell on the test machine, run
   `packaging/Trust-DevCertificate.ps1`. This trusts the public development signer in
   LocalMachine TrustedPeople only. Confirm the expected subject/thumbprint and
   approve UAC manually. Do not import it into Trusted Root.
4. Back in a normal unelevated shell, run `scripts/install-dev.ps1 -Mode Msix`.
   Check `Get-AppxPackage Cleanup.Dev`: `IsDevelopmentMode=False`, protected install
   location, signature valid, and the declared Windows App Runtime dependency present.
5. Run `scripts/phase0-feasibility.ps1 -RequireSignedPackage`, then repeat with
   `-IncludeElevated` and approve only the fixture's setup/removal/cleanup UAC prompts.
   Separately decline removal consent and verify a cancelled/uncertain result without
   falsely reporting removal. Check that Cleanup stays unelevated throughout.
6. Add and execute a minimal native folder-picker probe; validate Downloads, Desktop,
   and a selected custom/redirected folder with newly created fixture subdirectories.
   Confirm no pre-existing files change and repeat persistence after app restart.
7. Test dependency deployment on the clean VM, app updates preserving LocalState,
   package removal, and reboot-required MSI results. Record fresh screenshots and
   package activation evidence. Reproduce/profile the fixture deletion delay there.
8. Measure real cold activation and profile WinUI/.NET memory before attempting
   optimizations. Document the above-target working set even if unchanged.
9. Once these gates pass, begin Phase 1: native navigation, appearance, modes,
   settings, accessibility, and view models using CommunityToolkit.Mvvm.

Development trust can be removed through the same elevated setup script with
`-Remove`. If a feasibility run is interrupted, first ensure its uninstaller has
finished; `artifacts/target/fixtures/release/cleanup-fixture.exe status --scope user`
and `purge --scope user` inspect/clean only the dedicated Win32 fixture. Re-run the
harness for idempotent fixture setup and inspect its reports before proceeding.

## Primary references checked

- [Stable Windows App SDK downloads](https://learn.microsoft.com/windows/apps/windows-app-sdk/downloads)
  and [2.0 release notes](https://learn.microsoft.com/windows/apps/windows-app-sdk/release-notes/windows-app-sdk-2-0).
- [Packaged desktop app runtime behavior](https://learn.microsoft.com/windows/msix/desktop/desktop-to-uwp-behind-the-scenes).
- [Package-signing certificate trust](https://learn.microsoft.com/windows/uwp/packaging/create-certificate-package-signing).
- [Standard Windows Installer command options](https://learn.microsoft.com/windows/win32/msi/standard-installer-command-line-options).
