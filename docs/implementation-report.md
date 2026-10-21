# Development implementation and verification

Latest appearance, History and performance results: [0.0.34 audit](performance-audit.md).
Previous UI and release preparation: [0.0.28 verification](ui-release-update.md).
Earlier UI changes: [layout and logo update](ui-layout-update.md).
The measurements and artifact identity below preserve the 0.0.14 snapshot.

Date: 2026-10-20. Installed x64 development package: **0.0.14.0**.

The original Phase 0 diagnostic build is documented separately in
[phase-0-report.md](phase-0-report.md). Following the request to replace its
diagnostic presentation and complete the functional pages, the normal application
now opens Applications. Diagnostic controls are available only through `--selftest`.
This is a functional development build; the release and hardening gates below remain.

## What changed

| Area | Implementation |
| --- | --- |
| Applications | Automatic real inventory, search/filter/sort, virtualized rows, lazy icons, details, single and sequential multiple removal. Final installed launch displayed 483 applications. Unknown metadata remains empty. |
| Official removal | Registered validated executable, system Windows Installer command interface for MSI, PackageManager for current-user MSIX. Completion checks inventory as well as process results. Cancelling a wait leaves the official installer running and stops remaining queue items. |
| Leftovers | Separate itemized candidates with exact paths, association reasons and confidence. Initially nothing selected; a second confirmation lists selected targets. Recycle where supported; registry backup precedes permanent key removal. No automatic cleanup. |
| Organize | Downloads/Desktop resolution, native custom-folder picker, category/extension/date groups, custom destination/matching, persisted named rules, cancellable preview, individual exclusions, explicit move confirmation. |
| History | Persistent move/uninstall/cleanup records, partial/interrupted results, eligible move undo with file-identity and destination-conflict checks. Uninstall records are informational. |
| Appearance | Native Acrylic/Mica or solid across title bar, navigation and content; dark/light/system theme, live system accent and custom override. Title-bar palette button opens Settings. Advanced mode changes presentation immediately. |

The title drag region excludes the palette button; caption controls use transparent
backgrounds and matching theme foregrounds with DPI-aware right spacing. WinUI owns
native backdrop lifecycle, transparency and accessibility behavior. An earlier custom
backdrop transition crash was fixed by using the built-in backdrop implementations.
An unsupported accessibility-event subscription that caused launch failure was
removed. Windows color changes trigger appearance updates.

Custom accent changes mutate the existing native brush instances, including accent
fill/text resources in theme dictionaries. This fixes the observed case where the
textbox changed color but switches and navigation retained the old Windows accent.
Dark and light controls were visually checked. High-contrast dictionaries are excluded
from the custom palette; actual OS high-contrast transitions still need validation.

## Architecture and safety decisions

- C# owns views, confirmations and asynchronous UI state. Rust owns discovery,
  official execution, preview planning, file operations, leftovers and the journal.
- ABI **2** uses length-delimited UTF-8 JSON batches, opaque operation handles,
  explicit buffer release, panic containment and structured errors. All managed
  operation entry paths check compatibility. Long work runs outside the UI thread;
  preview/apply support cooperative cancellation between files.
- The package runs as invoker with full-trust desktop identity. The Rust DLL ships
  beside the executable; writable settings/rules/history use package LocalState.
  No elevated main window, persistent helper/service, filesystem watcher or network
  backend was added. Official uninstallers retain responsibility for their own UAC.
- Organizer accepts plain local absolute paths, canonicalizes existing folders,
  excludes protected/installed-application locations, and refuses reparse ancestors.
  It scans only the selected folder's immediate files, with a 20,000-entry limit.
  Hidden/system, linked, online-only, shortcut and partial-download files are skipped.
  Potential permission/read failures are reported.
- Approved destinations are fixed at preview. Apply fails on new collisions rather
  than silently changing the approved path. Native moves never overwrite, support
  long-path prefixes, and detect a cross-volume copy that leaves the source behind.
  Directory handles prevent ancestor deletion/replacement while used; file handles
  deny writes during identity verification and the move. These mitigate races without
  claiming to eliminate all filesystem/cloud interactions.
- Identity includes volume/file index, length and modification time. Undo refuses
  changed destinations and occupied original names, preserves failed items, and only
  removes directories created by the batch when empty. Identity is not a content hash.
- Flushed append-only JSONL records intent before moving and records each outcome.
  Torn lines are ignored and future events begin on a fresh line. A crash between
  the native move and its completed journal event still requires manual inspection;
  complete transactional recovery and uninstall rollback are not promised.
- Registry deletion from the packaged process was observed to affect only its
  virtualized view. Fixed Windows `reg.exe` export/delete/query operations now run
  with the current token and no shell. Actual host-key absence was checked.
  See [ADR 0003](adr/0003-registry-cleanup-under-package-identity.md).

## Main files changed

| Files | Responsibility |
| --- | --- |
| `src/frontend/App/{App.xaml.cs,MainWindow.xaml,MainWindow.xaml.cs}` | Normal activation, separate diagnostics, single instance, native title bar/navigation/backdrop |
| `src/frontend/Views/{ApplicationsPage,OrganizePage,HistoryPage,SettingsPage}.*`, `LeftoverReview.cs` | Actual task pages, confirmations, details and result states |
| `src/frontend/ViewModels/{ApplicationsViewModel,AppItemViewModel,HistoryItemViewModel}.cs` | Inventory/list state, queue, history presentation |
| `src/frontend/Services/{Appearance,SettingsService,RulesService,IconLoader}.cs` | Theme/accent, persistence, named rules, bounded lazy icon work |
| `src/frontend/Interop/{Features,NativeCore,NativeMethods}.cs` | Typed feature contracts, background calls, ownership and ABI checks |
| `src/rust/crates/{core,discovery,uninstall,operations,organizer,leftovers,ffi}` | Native implementation, safety checks, regression tests, ABI header |
| `scripts/{package-dev,install-dev,ui-automation,verify-cross-volume-ui}.ps1`, `packaging` | Signed builds/install, fixture-only native UI verification |
| `README.md`, `docs/ffi-contract.md`, this report and ADR 0003 | Usage, ABI, evidence, architecture and remaining gates |

## Builds and executed checks

Environment: Windows 11 x64 build 26200, Ryzen 7 5800X3D (16 logical processors),
64 GB RAM. SDKs used: .NET **10.0.402**, Rust **1.99.0** MSVC, Visual Studio 2022
Build Tools and Windows SDK **10.0.26100.0**. Frontend package references include
Windows App SDK WinUI 2.3.9, InteractiveExperiences 2.1.9, Runtime 2.5.1 and
CommunityToolkit.Mvvm 8.4.2. The package carries .NET runtime 10.0.13.

| Check | Actual result and scope |
| --- | --- |
| Signed Release MSIX build | Passed, zero warnings/errors. Local-feed restore used because this machine blocks .NET's direct NuGet connection; no security settings were changed. |
| Installation/update | 0.0.14.0 installed in `C:\Program Files\WindowsApps\Cleanup.Dev_0.0.14.0_x64__kfjrmgg4yenmj`; `IsDevelopmentMode=False`. Signature verified, development signer `CN=Cleanup Dev`. State survived signed updates. |
| Final normal launch | Passed on 0.0.14.0; real 483-row inventory rendered. Organizer preview produced three fixture moves, one adjusted collision name and one preserved subfolder; no execution in this final smoke check. |
| Rust unit/integration tests | **80 passed**, zero failed, rerun against final source: core 17, discovery 16, FFI 11, leftovers 5, operations 4, organizer 13, uninstall 14. Tests mutate dedicated temporary fixtures only. |
| Rust lint/format | `cargo clippy --all-targets -- -D warnings` and `cargo fmt --all --check` passed against final source. |
| PowerShell parsing | All scripts under `scripts` and `packaging` parsed without errors. |
| Signed feasibility harness | **11/11 checks passed on 0.0.10.0**, including Win32/MSI/MSIX fixture removal, Rust loading from WindowsApps, inventory, external Win32 uninstaller without package identity, persistence across restarts and fixture file moves. Not rerun wholesale after later appearance/path/ABI changes. |
| Native UI workflows | Actual application UI exercised search/filter/details, multiple selection, sequential Win32+MSIX fixture removal, explicit file/registry leftover review, rules save/load/delete, preview/collision/move/history/undo. Host filesystem/registry checks confirmed effects. |
| Cross-volume UI move/undo | Passed: Unicode text fixture in a dedicated C: Downloads directory moved to D: and restored through History, with content checked. |
| C# unit tests | No separate managed unit-test project or unit-test run yet. UI integration is not a substitute for those tests. |

Rust regression coverage includes parsing/normalization/duplicates, FFI errors and
memory ownership, cancellation, protected paths, destination links, planned collisions,
new collisions after preview, changed files, undo preconditions, journaling and torn
records. It does not cover the entire proposed release safety matrix.

The computer-use native transport was unavailable (`failed to connect native pipe`,
OS error 2). Verification therefore used Windows UI Automation scoped to Cleanup's
window, plus its own-window screenshots. This did not involve browser emulation.
Verified dark/light/custom-accent captures are under `artifacts/screenshots/v012`;
those captures predate the final ABI guard changes. Some later screen captures had
overlapping shell UI and are not visual acceptance evidence.

Local evidence (machine-specific, ignored build artifacts):

- `artifacts/phase0/20261020-114245/summary.json` and its run-a/run-b records.
- `artifacts/ui-fixtures/cross-volume-result.json`.
- `artifacts/ui-fixtures/final-metrics.json` for installed 0.0.14.0.
- Registry `.reg` backups and operation JSONL under package LocalState/history.

Final artifact: `artifacts/packages/Cleanup_0.0.14.0_x64_Test/Cleanup_0.0.14.0_x64.msix`
(47,528,554 bytes, approximately 45.3 MiB). SHA-256:

```text
B4CA1F969AAD68D81CAA02A653E457D690C6323894487922309E2D7AC8C46FF9
```

This identifies a locally signed development artifact, not a published release.
Release signing and identity must remain separate from the development certificate.

## Performance observations

The final normal UI, after loading inventory and exercising the organizer, sampled
**224.9 MiB working set**, **157.1 MiB private memory**, and **0.0% CPU across the
machine** over 5.003 seconds. An earlier settled normal-UI sample on 0.0.12.0 was
214.3/143.6 MiB and 0.0% CPU over 5.341 seconds. The approximately 120 MB working-set
target is **not met**; retained native icons/resources and the self-contained
WinUI/.NET baseline need profiling. No working-set trimming was used to lower results.

The 0.0.10 diagnostic harness measured 437/442 ms to window Loaded and 567/569 ms
for inventory; these were repeated local launches of the diagnostic window. They
do not establish cold-start time for the final normal UI or a clean machine. No
release-grade cold-start, disk-cache-controlled or thousands-of-record benchmark
has been completed.

## Limitations and exact next work

1. Profile the final normal UI's memory, icon cache/resource lifetime and activation
   using Windows performance tools. Establish repeatable cold-start and idle
   baselines before optimizing; the memory target remains a release concern.
2. Run the signed harness in a disposable clean Windows 11 VM, then run
   `scripts/phase0-feasibility.ps1 -RequireSignedPackage -IncludeElevated` with human
   UAC approval. Test both consent and refusal, machine MSI, reboot-required results,
   installer-busy outcomes, dependencies, update, uninstall and state lifecycle.
   No new successful elevated-fixture or clean-machine certification is claimed.
3. Add C# tests for view models, navigation/modes, settings failure, command enablement
   and cancellation. Repeat keyboard, Narrator, high contrast, transparency disabled,
   reduced motion, high DPI and narrow-window verification.
4. Extend disposable filesystem tests to redirected known folders, actual OneDrive
   placeholders/sync, long paths, permission denial, hard links, junction races,
   low disk space, process interruption between each journal stage and partial
   cross-volume failure. Current conservative exclusions may refuse valid locations.
5. Strengthen interrupted-operation reconciliation, journal schema/versioning and
   concurrent-process behavior. Review registry backup/delete races and verify
   32/64-bit host outcomes in the clean VM. Backups/history are not a restoration
   guarantee, nor is confidence a safety guarantee.
6. Complete production signing, release identity and public binary certification
   after the above gates. MIT licensing, a CI definition, development checksums
   and beta documentation were subsequently added in 0.0.28; hosted CI still needs
   its first GitHub run. ARM64 and Store
   certification remain untested. No post-v1 roadmap features were added.

Automatic approval review rejected a combined verification command with
`blocked by policy`; it included recursive fixture-directory cleanup under Downloads
and did not run. The narrowed move/undo check subsequently passed without deletion.
Those dedicated `CleanupVerification-*` Downloads folders remain for inspection;
their exact successful source is recorded in `cross-volume-result.json`. An earlier
disposable `HKCU\SOFTWARE\CleanupDisposableTestApp` test key may also remain from
the failed virtualized-delete test. No broad personal-folder or registry cleanup was
attempted to bypass that rejection.
