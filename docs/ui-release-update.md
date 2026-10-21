# UI and open-source preparation — 0.0.28

This preserves the earlier snapshot. See the [0.0.34 appearance, History and
performance audit](performance-audit.md) for current results.

Date: 2026-10-20. Signed development MSIX **0.0.28.0**, native ABI **2**.

## Implemented

The organizer starts with a flat, minimal folder selection screen. A native picker,
known-folder shortcuts and folder drop lead to a selectable preview. Doc, Media,
Binary and Others are the default; additional grouping, extension/category filters,
destinations and saved rules live in Options. Rules record their source and use an
explicit schema version to preserve older collision-policy meanings.

Execution still uses the existing guarded Rust planner, file operations and journal.
The UI confirms exact source/destination paths, supports preview/move cancellation,
disables conflicting commands while busy, and preserves the undo action when some
files could not be restored. Name clashes never silently overwrite existing files.

Logos have transparent backgrounds, including regenerated MSIX/ICO assets. Native
navigation supplies one accent indicator. Selection fades in place, new icons pop
in and counts animate without sliding into Settings. Motion follows Windows
animation-effects changes. About has consistent inset spacing and a separate button
row. Credits remain centered without navigation, with Back in the title bar.

MIT was selected by the owner. The repository includes the license, package/crate
metadata, contribution/security guidance, third-party notice summary, NuGet and
Cargo locks, beta notes and a Windows CI definition with pinned official actions.
No public repository or release was created.

## Changed surfaces

- `Views/OrganizePage.*`, `Services/RulesService.cs`, `Services/SettingsService.cs`:
  guided flow, busy/cancellation/confirmation handling and saved rules.
- Rust `organizer`/known-folder contracts: simple grouping and Documents shortcut.
- `Controls/AppSelectionPanel.*`, `Services/Motion.cs`, `Views/SettingsPage.xaml`,
  shell/appearance resources and `Assets`: selection, navigation, logos and About.
- `Cleanup.csproj`, `Package.appxmanifest`, crate manifests, `packages.lock.json`:
  package 0.0.28, dependency lock and MIT metadata/content.
- Root release documentation, `.github/workflows/build.yml`, `.gitignore`, asset
  generation and verification scripts.

## Verification executed

| Check | Actual result |
| --- | --- |
| `scripts/package-dev.ps1 -UseLocalFeed` | Passed; 0 warnings, 0 errors, 19.31 s build stage. |
| Signed installation | Passed; WindowsApps install, `IsDevelopmentMode=False`, deployment status Ok, signature Valid. |
| Rust format / strict Clippy / locked tests | Passed; **82 tests**, 0 failures. |
| Locked NuGet restore through local feed | Passed with `RestoreLockedMode=true`. |
| `verify-about-ui.ps1` | Passed: compact switches, centered credits, title-bar Back, return to Settings, +1/+2/+3 selection counts. |
| `verify-sidebar-layout.ps1`, normal and `-Compact` | Passed; navigation positions unchanged across pages with single/multiple/empty selections. |
| Updated `verify-cross-volume-ui.ps1` | Passed through the installed native UI: OS folder picker, all four groups, Unicode, cancellation at both approval dialogs, collision preservation, C:→D: moves and undo. |
| Asset/package content | Transparent PNG alpha verified; LICENSE and notice summary present in the installed package. |
| Screenshots | Start, Settings/About, Applications and credits captured and visually inspected. |

The UI scripts were updated for the new flow. Initial attempts stopped before
approval because of modal initialization, asynchronous provider-tree updates and
incorrect UIA/polling assumptions. Those script issues were corrected; the full
fixture check then passed. Restored namespaced fixtures were retained; no personal
files or real applications were removed.

The computer-use native transport returned `failed to connect native pipe` (OS
error 2). Verification used the existing scoped Windows UI Automation helper and
owned-window captures. The OS folder picker's native controls were validated as
children of the modal window owned by Cleanup before interaction.

Local evidence is ignored under `artifacts/`: `resume-build-002.log`,
`resume-core-tests.log`, `resume-about-028.log`, sidebar logs/JSON,
`resume-organizer-028.log`, `ui-fixtures/cross-volume-result.json` and
`screenshots/v028`. These are machine-specific and may contain private paths.

Artifact: `artifacts/packages/Cleanup_0.0.28.0_x64_Test/Cleanup_0.0.28.0_x64.msix`
(approximately 45.6 MiB), SHA-256:

```text
e7ec9537fa3d8a2dc5fb2e2136c26cc0b83fe8fccda757deb47d029046315bd3
```

## Performance and remaining work

Hardware: Windows 11 x64, Ryzen 7 5800X3D, 64 GB RAM, existing SSD development
environment. The normal UI after page/fixture/UIA checks sampled **296.8 MiB working
set / 226.5 MiB private memory**, **0.0% machine-wide CPU over 5.010 s**. A fresh
launch on Applications, before further UIA traversal, sampled **201.1 / 133.8 MiB**,
**0.0% over 5.008 s**. These are different workload states, not a controlled
before/after benchmark. No working-set trimming was used. The 120 MB target is not
met; cold-start targets and clean-machine performance remain unverified.

There is no C# unit-test project yet. Live reduced-motion/high-contrast changes,
Narrator, redirected/cloud folders, interruption/race/failure coverage and the
elevated disposable uninstall matrix still need dedicated verification. Existing
official uninstall strategies were not changed or rerun in this UI update.

GitHub CI has not run; a local YAML parser was unavailable, and the hosted workflow
must be verified after push. PowerShell files parsed locally. No production signing
or clean-machine certification is claimed. Follow [release preparation](release-preparation.md)
for the exact remaining binary-release gates.
