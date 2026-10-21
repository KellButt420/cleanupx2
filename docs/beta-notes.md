# Development beta notes

cleanup combines a native application inventory with official uninstaller
coordination and an on-demand folder organizer. It does not run a service or
automatically clean the system.

## This UI update

- Version 0.0.34 uses lowercase **cleanup** throughout displayed branding. Acrylic
  has eight tint choices, a native custom-color picker and live strength/background
  controls. The title bar shares the window material.
- History has search, type/status filters and a scrolling details drawer. Undo
  refreshes both History and the organizer's completion screen. Native summary
  grouping reduces repeated scans of the journal.
- A minimal folder selection screen leads to a selectable move preview. The
  default groups are Doc, Media, Binary and Others; extra sorting/rule choices
  stay in Options. Final approval lists exact source and destination paths.
- Preview and move operations support cancellation. Completed moves in a stopped
  batch remain inspectable in History. Undo checks file identity and refuses
  occupied original names; it is not a complete recovery guarantee.
- Selection stays in the sidebar, with six icons and an additional-count badge.
  Native navigation has one accent marker and stable positions across pages.
  Selection changes use restrained motion that respects Windows animation effects.
- Logos use transparent assets. About has an inset credits button; credits fade
  into a centered canvas with Back in the title bar.
- MIT licensing, contribution/security documentation, dependency locks and a
  pinned Windows build workflow are included.

## Limits

This is an x64 development build. Production signing, clean-machine certification,
the elevated fixture matrix and the complete accessibility/filesystem failure
matrix remain open. There is no managed unit-test project yet. Uninstaller exit
alone is not proof of removal; leftover review does not guarantee attribution.
Files changed after a move may not be eligible for undo.

On the development machine, final 0.0.34 sampled 201–219 MiB working set after
launch and 233 MiB in a settled sample after exercising pages, above the original
120 MB target. Five-second post-launch CPU samples ranged from 0.000% to 1.190%
across the machine; the separate settled sample was 0.000%. Controlled cold-start
release targets have not been established. See the [current performance
audit](performance-audit.md) for methods, test versions and remaining limits.
