# UI layout update

2026-10-20. Development MSIX x64, version 0.0.23; this update follows the functional-build report.

## Latest refinements

- Settings switches use the native compact 40-pixel layout, without On/Off text
  reserving empty trailing space. Both align with the right content edge of their
  cards and retain accessible names and native toggle state.
- Sidebar items explicitly use the modern WinUI ListViewItem style and a six-pixel
  radius for both selected and pointer states, including Settings.
- At most six selected icons are shown; seven/eight/nine selections display
  `+1`/`+2`/`+3` beside them. The complete selection still drives the batch action
  and exact removal confirmation. The icon strip no longer scrolls or stacks names.
- The **luh geek** button sits directly on the About card in Settings. It fades
  in a canvas spanning the whole window, with no sidebar or title-bar branding.
  The supplied transparent logo is centered above `forthescene`,
  `a (wired-in) project`, and the current calendar year. Back sits in the
  title-bar row where the app icon is on other pages and returns to Settings
  with the same fade; Escape and Alt+Left also go back. The fade is skipped
  when Windows animations are turned off.
- **Organize** defaults to four folders by extension: **Doc** (.pdf, .doc,
  .txt, …), **Media** (.jpeg, .mp3, .gif, …), **Binary** (.exe, .zip, .dll, …),
  and **Others**. Each shows its file count after a preview. Detailed types,
  extension, and date modes, date subfolders, collision handling, filters, and
  saved rules are under **More options**. Folders can be dropped onto the page;
  the last folder and mode are remembered; a finished move offers **Undo** and
  **Open folder** in place.
- The supplied WebP is retained as `Assets/ForthesceneLogo.webp`. A lossless PNG
  format conversion with identical RGBA pixels is packaged for native image loading;
  its transparency and artwork were preserved. No network content is involved.

## Changes

- Moved the supplied SVGs to `src/frontend/Assets/LogoGreen.svg` and
  `LogoViolet.svg`. Violet is the default. Appearance offers both window-logo
  variants; the package's static identity assets use violet.
- Generated scaled MSIX tile/store assets, shell target sizes and a multi-resolution
  `Cleanup.ico`. Both the executable and `AppWindow` use the new icon. The dark
  icon tile retains contrast in both light and dark Windows themes.
- Aligned the logo and navigation glyphs to the same horizontal center. Removed
  the extra hamburger row; aligned page padding, list headers and row columns.
- Added subtle native-theme separators between application rows.
- Moved single-application details and multiple-selection actions into a compact,
  scrolling sidebar section between navigation and Settings. A single horizontal
  icon strip replaces the Selection heading; multiple selected names are not
  stacked below the Remove button. Icon tooltips preserve names, with a `+N`
  count after six icons. The list remains
  visible; clear selection replaces closing an overlay. Technical details remain
  available in Advanced mode inside a collapsed expander.
- Kept explicit removal confirmation and official uninstall behavior. Selection
  state now comes directly from the complete selected-items collection, avoiding
  stale details when changing between one and multiple selected applications.

## Sidebar regression

The first footer-based implementation shifted navigation when switching pages:
`scripts/verify-sidebar-layout.ps1` failed on installed 0.0.16 with
`Applications shifted on Organize from 8,53 to 12,229` (coordinates relative to the
window). The user observed the same displaced rows and disappearing header.

The shell now uses native ListViews in fixed Grid rows: navigation at the top,
selection in the bounded middle row, Settings at the bottom, and the page Frame in
an independent column. Hiding or resizing details cannot change the navigation
rows. Only page content participates in page entrance transitions. Native controls,
keyboard selection, Windows accent indicators, accessible names and the continuous
window backdrop are retained.

The regression helper compares the actual UI Automation rectangles across all four
pages at normal and minimum window sizes. It drives the installed app and performs
no uninstall or filesystem operation. Its source is retained for future UI changes.

## Validation

- Release signed MSIX build completed with zero warnings and zero errors.
- Signed installation/update and activation tested on the development machine.
- Checked real inventory, single/multiple sidebar selection, clear selection,
  page changes and the 900 × 580 minimum layout.
- Confirmed that a sidebar removal action opens the existing itemized confirmation;
  cancelled the dialog. No real application uninstaller was launched during UI work.
- Verified the green logo and light theme visually on 0.0.15; final shell changes
  retain the same logo/theme implementation.
- The position regression first failed on 0.0.16, then passed on 0.0.17 at both
  1240 × 800 and 900 × 580. Final 0.0.21 validation adds single/multiple/empty
  selection checks; see the local `artifacts/sidebar-layout-*.json` outputs.
- The earlier scrolling strip was verified on 0.0.19 and then replaced by the
  requested six-icon cap. The installed 0.0.22 check verified `+1`, `+2`, `+3`,
  right-aligned switches, rounded navigation, and About → credits → About → Settings.
- Final signed 0.0.23: `scripts/verify-about-ui.ps1` passed switch alignment,
  full-window centered credits without a sidebar, Back/Appearance navigation,
  and `+1`/`+2`/`+3` counts. Both normal and compact sidebar regression runs passed
  again with single/multiple/empty selection. PowerShell parsing and the package
  signature check passed; PNG pixels were checked against the decoded source WebP.
- Rust behavior was unchanged in this update; its earlier 80-test result is
  documented in the functional report, not claimed as a new test execution here.

Native Computer Use transport remained unavailable (OS error 2); scoped Windows UI
Automation and own-window captures were used. Machine-specific builds, screenshots
and UI measurement output stay under ignored `artifacts`.

## Asset regeneration

The original vectors are the source of truth. PNG/ICO files are committed build
inputs, so ordinary application builds require only the documented .NET/Rust tools.
To regenerate assets after a logo revision:

```powershell
npm install --prefix artifacts/asset-tools --no-save --package-lock=false @resvg/resvg-js@2.6.2
node scripts/generate-brand-assets.mjs
```

Node and the SVG rasterizer are development tools only and are not bundled into
the native application. The existing memory target, clean-VM/UAC, accessibility and
release-certification limitations from the implementation report remain open.

## 0.0.26

- The selection panel keeps **Remove** at the bottom, directly above Settings,
  for one app and for several. With several apps the count and details come
  first, then the app icons, then Remove. With one app its icon sits beside its
  name and the details scroll above the pinned buttons.
- The Applications list header has its own shaded bar and a visible divider.
- The logo, navigation icons, and selection panel all sit 12 px from the left
  edge, matching the logo's 12 px from the top; labels align under "Cleanup".
- The About card in Settings shows a larger logo, the name with a
  "Development build" tag, the description, and the version and native core on
  one quiet line, with **luh geek** on the right.
