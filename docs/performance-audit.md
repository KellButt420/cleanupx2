# cleanup — appearance, History and performance audit

2026-10-20. Final signed development package: **0.0.34.0**, x64, native ABI 2.

## 0.0.40 update: Core landing, leaks, icons, and selection

Package **0.0.40.0**. The app opens on **Core**, which lists local, removable, optical,
and mapped network drives and shows combined used and free space. Applications loads
its inventory on the first visit, so launch no longer builds the 469-app list.

Same machine as below. Measured with `scripts/audit-startup.ps1` (three launches per
state) and `scripts/audit-navigation.ps1`:

| State | Working set | Private | Managed heap |
| --- | --- | --- | --- |
| 0.0.34, same session, Applications loaded | 201.9–208.9 MiB | 133.8–140.1 MiB | not recorded |
| 0.0.40, Core landing, no inventory | 167.7–168.2 MiB | 108.3–109.0 MiB | 1.6 MiB |
| 0.0.40, Applications with 469 apps | 212.8–214.3 MiB | 139.3–139.9 MiB | 4.1 MiB |

Launch is about 35 MiB lower in working set. Visiting Applications is about 10 MiB higher
in working set and 5 MiB higher in private memory than 0.0.34. The Core page's XAML is
loaded at startup and stays loaded.

Navigation, eight cycles of Settings → History → Organize → Applications → Core, driven
through UI Automation. Private memory from cycle 2 to cycle 8:

- 0.0.35, before the leak fixes: 172.4 → 185.7 MiB, about 2.2 MiB per cycle.
- 0.0.36: 168.6 → 173.3 MiB, about 0.8 MiB per cycle.
- 0.0.40: 165.6 → 175.3 MiB, about 1.6 MiB per cycle.

Growth is lower but not zero. Part of it may come from the UI Automation driver. That
was not isolated.

Full scroll of the 469-row Applications list, 0.0.40: private memory rose 5.9 MiB at the
bottom and kept 4.2 MiB after returning to the top. On 0.0.36 the same scroll added
13.9 MiB and kept 15 MiB. Repeated passes on 0.0.37 stayed flat.

Leaks and lifetime fixes:

- `OrganizePage` subscribed to the static `OperationUpdates.UndoCompleted` event in its
  constructor and unsubscribed only when the window closed, so every visit left a page
  pinned. It now subscribes on Loaded and unsubscribes on Unloaded.
- `SettingsPage` added a `MainWindow.Closed` lambda on every construction. The lambda is
  gone. A pending tint save now flushes when the page unloads and when the window closes.
- `Motion.ShowHide` kept a weak reference per call indefinitely. Dead entries are now
  purged on each call.

Other changes:

- Icons: rows keep their icons only for the 160 most recently shown. Selected rows keep
  theirs. The shared icon cache is removed. After each 48 evictions a collection runs,
  because a bitmap's native pixels are released only when its managed wrapper is
  collected. The scroll numbers above show the effect; the mechanism is not isolated.
- Applications: eight shared avatar brushes instead of one per app. The install-folder
  check runs once per row instead of on every binding evaluation, which previously ran
  on the UI thread.
- Applications restores the list selection when the page loads. Before, the sidebar said
  "N apps selected" while the list showed none. Re-publishing the selection on load hid
  the sidebar panel's animated parts, so the restore updates only the list unless a
  selected app has gone.
- Drives (Core): each drive is probed on its own thread with a limit of 4 seconds for
  network drives and 15 seconds otherwise. A mapped share is counted once when two letters
  point at it.
- Diagnostics: `performance-audit.json` (opt-in `--performance-audit`) now records managed
  heap, GC-committed bytes, working set, private bytes, handles, threads, and Gen2
  collections.

Tested and rejected:

- Frame cache off (`CacheSize = 0`, cache mode removed): private memory rose from 108 to
  266 MiB over eight cycles, about 30 MiB per cycle. The cache stays.
- Backdrop, Applications state, two runs each: Acrylic 214–215 MiB working set, Solid
  212–213 MiB, Mica 218–227 MiB. The default stays.
- Garbage-collection tuning: the managed heap is about 4 MiB, so the collector is not the
  lever. No GC settings were changed.

Checked and left alone:

- Native JSON copies and History's data cost. The journal is 43 KB on this machine, and
  History's first-visit cost is XAML construction and JIT, not data.
- `x:Bind` on the shared view model. The generated code tracks through weak references,
  so it does not pin pages.
- The Credits logo is 313 px, so it is not a decode-size problem.
- Search rebuilds the list on each keystroke, and each row has a flyout. Both cost CPU,
  not much memory. Organize's plan and undo state are still lost when the page is left.

Open: the 120 MB target is not met. Launch is about 168 MiB and the Applications state
about 213 MiB. The managed heap is about 4 MiB, so most of the memory is WinUI, the runtime,
and native allocations. Clean-VM cold starts, a native memory profile, and a navigation run
without UI Automation are still needed to settle the remaining growth.

## Implemented

- Lowercase **cleanup** in the window, About, package display name, executable
  product metadata and user-facing messages. Technical package/assembly identifiers
  remain stable so development updates keep their existing state.
- A 700 ms credits fade with a gentler curve. Timed captures verify that the
  artwork appears progressively, in place. Windows animation preferences still
  control whether a new transition animates.
- Native desktop acrylic tint choices: Windows default, Windows accent, Graphite,
  Violet, Ocean, Forest, Rose and Custom. Custom colors can use a native color
  picker or hex input. Tint strength and background visibility update live and
  persist. The same material covers the title bar and workspace. Mica and Solid
  remain available; acrylic controls appear only when relevant.
- A full-width History list with search, type/status filters, concise outcomes and
  an opaque details drawer. Recorded items are virtualized; metadata, long messages
  and paths scroll with them. Close and eligible Undo remain reachable. Technical
  identifiers follow Advanced mode. Undo refreshes the drawer and the organizer's
  cached completion state.

## Code cleanup guided by measurements

The sidebar clears its unused navigation history after each move. Its destination
pages keep `NavigationCacheMode="Required"`. The 0.0.34 text said pages are reused;
that was not verified. The 0.0.39 test below shows the frame cache is what keeps memory
flat, while whether a visit reuses or rebuilds a page was not checked. Logo image sources
are reused when their URI is unchanged.
Tint slider previews update the existing native controller; settings writes are
debounced during a drag. No background monitor, service or network request was added.

Native History previously searched the complete event log once per batch. It now
groups borrowed events once and folds each group, preserving event order and stable
ordering for equal timestamps. No database or new Rust dependency was introduced.
A regression test covers interleaved, orphan, interrupted and restored records.

The UI tests also exposed two defects that were fixed before completion:

- A base backdrop configuration-change callback threw `ArgumentException: target`
  during theme changes. The custom material now updates its connected controller
  from the live XAML configuration without that target lookup, and disconnects and
  disposes its controller when replaced. Repeated tint/theme/material checks pass.
- The old move preview could briefly remain actionable while an Options dialog
  finished closing. Moves stay disabled until the new preview is ready. Verification
  now checks the proposed destination before it approves a fixture move.

## Hardware and method

Windows 11 Pro **10.0.26200**, Ryzen 7 5800X3D, 16 logical processors, **63.9 GiB RAM**.
Toolchains: .NET SDK 10.0.402, Rust 1.99.0, Windows SDK 26100; Release x64.

All UI measurements use a real installed MSIX in WindowsApps. Startup measurements
use the normal UI with the explicit `--performance-audit` switch, which records local
process-to-Loaded and inventory timings. It has no timer or telemetry. The script
waits for inventory, allows three seconds to settle, then samples process CPU for
five seconds. These local launches include a first launch after package update;
they are **not disk-cache-controlled cold starts, click-to-paint timings or clean-VM
measurements**. No forced GC or working-set trimming was used.

### Launch and memory — final 0.0.34

| Run | Window Loaded | Inventory, 469 apps | Working set | Private memory | Five-second process CPU, across machine |
| --- | --- | --- | --- | --- | --- |
| 1 | 1,420.6 ms | 587.0 ms | 201.3 MiB | 134.5 MiB | 0.000% |
| 2 | 801.8 ms | 552.0 ms | 202.1 MiB | 133.3 MiB | 0.020% |
| 3 | 769.8 ms | 558.7 ms | 218.7 MiB | 150.8 MiB | 1.190% |

The higher third CPU sample is included; its transient cause was not isolated.
After navigation and credits checks, a separate settled 5.019-second sample was
**0.000% CPU, 233.41 MiB working set, 161.21 MiB private memory**. The original
approximately 120 MB working-set target is **not met**. Further allocation and native
resource profiling is needed; these numbers do not justify claiming near-zero CPU
throughout every post-launch interval.

### Repeated navigation and fade

The same script visited Settings → History → Organize → Applications twelve times.
Private memory growth from cycle 2 to cycle 12 was **54.90 MiB** on the 0.0.28
baseline and **17.35 MiB** on final 0.0.34. This is an observed reduction in workload
growth, not proof that every retained object or leak has been eliminated. Resource
warmup, UI Automation, compositor activity and collection timing affect the samples.

The baseline logo was already **81.8%** of its final purple signal at **123 ms**;
final 0.0.34 was **21.3% at 135 ms**. Captures use a fixed logo region and record
actual sample times. The local regression checker fails both the navigation-growth
and early-fade checks on the captured baseline, and passes on final 0.0.34.

### Native History benchmark

Five runs per size, optimized Rust, synthetic informational events entirely in memory:

| Records | Before | After |
| --- | --- | --- |
| 1,000 | 5.093 ms | 0.585 ms |
| 10,000 | 485.895 ms | 5.366 ms |

The 10,000-record core summary benchmark is about **90.5× faster**. It excludes disk
read/JSON parsing, FFI serialization, complex move batches and UI rendering. It is
not a 90× claim for the complete History page or the whole application.

## Verification actually run

- Final 0.0.34: signed package build passed with **0 warnings, 0 errors**; MSIX
  installation passed with `IsDevelopmentMode=False`, deployment Ok and valid signature.
  Final Rust formatting, strict Clippy and **83 tests** passed.
- Installed UI checks on 0.0.31/0.0.33 passed: eight tint choices, custom color/slider
  persistence, repeated dark/light/material switching, native color picker apply
  and cancel, History search/filters/details and restored-batch undo eligibility.
- 0.0.33 passed the complete disposable C:→D: fixture workflow: native folder picker,
  all four groups, cancellation at both approval dialogs, collision preservation,
  Unicode content and undo **through the new History drawer**. Restored fixture
  directories were retained for inspection. No real application was uninstalled.
- 0.0.33 passed the existing About/credits, six-icon overflow, compact switch and
  normal/compact stable-sidebar checks. Final 0.0.34 was launched three times,
  exercised by the navigation/fade audit and visually checked at normal and compact
  sizes. The final display-metadata check reports lowercase `cleanup`.
- Fifteen PowerShell scripts parsed locally. The captured-baseline regression
  checker failed as expected; the final captured regression checker passed.

Several initial automation attempts stopped before approval because of asynchronous
dialog/provider initialization, dropdown scrolling and stale preview assumptions.
The scripts now wait for readiness and exact fixture paths. The discovered product
defects above were fixed, then the workflow passed. An interrupted fixture check's
moves were restored through its exact named History entry before testing continued.

The computer-use transport returned `failed to connect native pipe` (OS error 2).
Native verification used the existing process-scoped UI Automation helper and
owned-window captures. Diagnostic evidence remains under ignored `artifacts`.

## Important files

- `src/frontend/App/MainWindow.*`, `Cleanup.csproj`, `Package.appxmanifest`:
  branding, navigation reuse, fade and packaging metadata.
- `Services/{TintedAcrylicBackdrop,PerformanceAudit,OperationUpdates,SettingsService}.cs`,
  `Views/SettingsPage.*`: native tint lifecycle, opt-in audit, UI updates and preferences.
- `Views/HistoryPage.*`, `ViewModels/HistoryItemViewModel.cs`, `Views/OrganizePage.xaml.cs`:
  History design, undo synchronization and preview gating.
- Rust `operations/src/lib.rs` and `operations/benches/history.rs`: grouped summaries,
  regression coverage and reproducible benchmark.
- `scripts/audit-*.ps1`, UI verification scripts, `benchmarks/check_audit.py` and this
  report: reproducible checks and measured limits.

## Reproduce and next work

The final development artifact is
`artifacts/packages/Cleanup_0.0.34.0_x64_Test/Cleanup_0.0.34.0_x64.msix`
(47,809,024 bytes). SHA-256:

```text
8bc1d7121bc2ddde7da343a77f163094b9b8c8b9379d682afeb2597e7580eb17
```

This uses the development signer; production signing and release certification
remain separate work. Package and framework hashes are in `artifacts/SHA256SUMS.txt`.

See [benchmark instructions](../benchmarks/README.md). Local evidence includes
`artifacts/performance/{baseline028,final034,startup}`, `history-benchmark-{before,after}.log`,
`appearance-history-*` logs and `screenshots/v034`. These can contain private paths
and inventory names; review before sharing.

Next: profile long-running managed/native allocations, establish real cold-start
measurements on a clean Windows VM, add managed unit tests, and finish Narrator,
high contrast, live reduced motion and redirected/cloud-folder coverage. The Store,
production signing, elevated fixture matrix and hosted CI remain unverified. Follow
[release preparation](release-preparation.md) before any public binary release.

Native implementation references: [SystemBackdrop configuration](https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.media.systembackdrop.getdefaultsystembackdropconfiguration),
[DesktopAcrylicController](https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.composition.systembackdrops.desktopacryliccontroller).
