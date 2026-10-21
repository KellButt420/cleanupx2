# Third-party components

cleanup's own source is licensed under [MIT](LICENSE). Dependency and platform
licenses remain in effect; the project's license does not replace them.

- Windows App SDK / WinUI: Microsoft package license and notices.
- .NET runtime and SDK: the licenses and third-party notices distributed by .NET.
- CommunityToolkit.Mvvm: its package license and notices.
- Visual C++ app-local runtime files: Microsoft's Visual Studio redistribution
  license terms apply.
- Rust dependencies: licenses identified by each exact package in Cargo.lock.
- Fluent icons and Segoe fonts are provided by Windows; font files are not copied
  into this repository.
- `@resvg/resvg-js` is used only to regenerate Windows icon assets. Its MPL-2.0
  package license applies to that development tool; Node and the tool are not
  bundled into Cleanup.

The project logos and credits artwork were supplied for this project. The original
SVG/WebP inputs are retained with their derived Windows assets. No branding or
proprietary assets from other uninstaller utilities are included.

The desktop packaging script copies license/notice files from the exact restored
.NET, Windows App SDK, CommunityToolkit, and Rust dependency packages into
`Notices/` beside the app. Keep these files with the portable application.

The setup EXE is built with Inno Setup 7.1.0 under its
[license terms](https://jrsoftware.org/files/is/license.txt). The compiler itself
is not bundled with the app. Review redistribution terms when changing dependencies
or distributing a commercial version.
