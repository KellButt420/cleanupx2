# Development

Build and test cleanup on Windows 11 x64. Run the commands below from the repository root.

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
