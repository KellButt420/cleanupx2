# Third-party components

cleanup's own source is licensed under [MIT](LICENSE). Dependency and platform
licenses remain in effect; the project's license does not replace them.

- Windows App SDK / WinUI: Microsoft package license and notices.
- .NET runtime and SDK: the licenses and third-party notices distributed by .NET.
- CommunityToolkit.Mvvm: its package license and notices.
- Rust dependencies: licenses identified by each exact package in Cargo.lock.
- Fluent icons and Segoe fonts are provided by Windows; font files are not copied
  into this repository.
- `@resvg/resvg-js` is used only to regenerate Windows icon assets. Its MPL-2.0
  package license applies to that development tool; Node and the tool are not
  bundled into Cleanup.

The project logos and credits artwork were supplied for this project. The original
SVG/WebP inputs are retained with their derived Windows assets. No branding or
proprietary assets from other uninstaller utilities are included.

Before public binary distribution, include the notices delivered with the exact
runtime and dependency versions in the package. Keep any notices already included
by self-contained .NET / Windows App SDK packaging. The release procedure records
this remaining binary-distribution check explicitly.
