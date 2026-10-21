# Confirmed registry cleanup through the Windows registry tool

Status: accepted for the development build, 2026-10-20. Release certification pending.

## Context

In the installed, signed MSIX, deleting a disposable application's key through
`RegDeleteTreeW` changed the packaged process's apparent view while the host key
remained. A successful return and an in-process existence check were insufficient.
Microsoft documents registry virtualization for packaged desktop apps; the precise
behavior depends on runtime and manifest settings. This decision follows the observed
signed-package fixture behavior on Windows 11 build 26200, not an assumption about
every MSIX application.

## Decision

Rust uses the fixed Windows System32 `reg.exe` executable for `export`, `delete`,
and subsequent `query` of explicitly reviewed application-specific SOFTWARE keys.
Arguments are separate native process arguments. No shell, metadata-provided
executable, script, elevation, service, or privileged helper is involved. Registry
views are selected explicitly. The in-process traversal remains a completeness
and registry-link guard. The real key is exported and the backup flushed before
deletion. Cleanup requires the review's explicit selection and second confirmation.
Protected/shared structural keys are refused; ordinary token permissions still apply.

This is a narrow registry operation owned by Rust, not a general command execution
interface or a generic registry cleaner. The application remains unelevated.

## Consequences and evidence

The installed-package UI removed a fresh publisher-specific disposable HKCU key.
An unpackaged PowerShell check confirmed that the host key was gone, and the `.reg`
backup existed in package LocalState. The earlier in-process-only check failed that
same class of host verification and was replaced. Future tests must check the host
view, including 32-bit keys and access-denied outcomes on a clean VM. Exporting and
deleting are separate operations; concurrent third-party registry changes remain a
race to investigate during hardening. A backup does not promise uninstall rollback.

## References

- [Packaged desktop runtime behavior](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes)
- [Windows reg export](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/reg-export)
- [Windows reg delete](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/reg-delete)
