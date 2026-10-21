# Local performance checks

Run from the repository root on Windows x64. Build/install a signed development
MSIX first. Leave its window untouched during UI automation.

```powershell
. .\scripts\env.ps1
cargo bench --locked --manifest-path src/rust/Cargo.toml -p cleanup-operations --bench history
.\scripts\audit-startup.ps1 -Runs 3
.\scripts\audit-navigation.ps1 -Label current -Cycles 12 -CreditsFrames
```

The Rust benchmark uses 1,000 and 10,000 synthetic informational events entirely in
memory. It never installs or removes anything. Each reported summary time averages
five runs and excludes journal parsing, disk I/O, FFI serialization and rendering.

The startup script closes the app normally and opens the **normal UI**, using the
explicit `--performance-audit` switch. That switch writes only local timing/count
data under the app's LocalState. It adds no timer, background monitoring or network
access. The script samples process memory and five-second idle CPU without further
UIA traversal. Results describe warm local launches, not a controlled cold boot or
the first painted frame. No process is killed or working set trimmed.

Each run measures two states: the Core landing page right after launch, then
Applications once its inventory has loaded. The second state is reached through UI
Automation before its sampling window starts.

The navigation script visits Settings, History, Organize, Photos, Videos, Applications and Core, samples
process memory/handles/threads and optionally captures timed credits frames. Those
captures include UI Automation and compositor overhead. Compare equivalent workload
states; initial resource/JIT warmup is expected. Keep Windows animation effects on
when assessing the fade; disabling them should show the page immediately.

Evidence stays under ignored `artifacts/performance`. Do not upload unreviewed
screenshots or machine-specific journal data. See the current performance report
for measured results and remaining targets.

With Python and Pillow installed, check the captured workload for regressions:

```powershell
python benchmarks/check_audit.py artifacts/performance/current
```

This requires the navigation run with `-CreditsFrames`. Its local guards check
private-memory growth after warmup and the early credits frame; passing them does
not establish the application's overall memory or startup targets. See the
[measured audit](../docs/performance-audit.md) for the baseline and final results.
