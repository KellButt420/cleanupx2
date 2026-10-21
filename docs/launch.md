# Launch and updates

The source repository is `https://github.com/wiredincorp/cleanup`. The site is
configured for `https://cleanup-wiredin.vercel.app`. Support lives at
`https://discord.gg/5kRtCcNWU6`. `release.json` holds these public links and is
bundled into both the app and website. It never contains credentials.

## Current release candidate

`scripts/package-release.ps1` builds a Release MSIX using a separate production
manifest without changing `Cleanup.Dev`. A candidate can be built unsigned:

```powershell
./scripts/package-release.ps1 -Version 0.1.0.0 -IdentityName WiredIn.Cleanup `
  -Publisher 'CN=wiredincorp' -Unsigned
```

This candidate is not a public installer. Its publisher is provisional until a
trusted code-signing identity is selected. Keep that identity stable afterward;
changing publisher or package name creates a different app and data location.

## Signed releases

Select a publicly trusted Windows code-signing certificate or signing service.
The certificate subject must exactly match the release Publisher. Local release
packaging accepts a certificate thumbprint from CurrentUser/My. Self-signed
certificates are rejected. Hardware-backed or remote signing services need their
own signing step; the prepared GitHub workflow supports an exportable PFX.

Create a GitHub environment named `release`, require a reviewer, and configure
`CLEANUP_SIGNING_PFX_BASE64` and `CLEANUP_SIGNING_PASSWORD` as environment secrets.
Use GitHub's private secret entry, never commit a certificate or password.
Run **Signed Windows release** with the version, identity, and exact publisher.
The workflow verifies the Rust core, builds and signs the MSIX, packages the x64
framework dependencies, generates checksums and an App Installer feed, and creates
a draft release. Release tags are `v<four-part-version>`, such as `v0.1.0.0`.

Complete the clean-machine checks in [release preparation](release-preparation.md)
before publishing that draft. Include the MSIX, all generated x64 dependency
packages, `cleanup.appinstaller`, and `SHA256SUMS.txt` in the release assets.
Use a strictly increasing version ending in `.0`; do not overwrite prior assets.

## Automatic updates

Install through `cleanup.appinstaller` to register the Windows update feed.
Its stable URL is:

```
https://github.com/wiredincorp/cleanup/releases/latest/download/cleanup.appinstaller
```

The feed points to immutable, version-specific MSIX and framework dependency URLs.
It checks on launch, shows the Windows update prompt, permits launching without
updating, and enables Windows' background update task. Windows verifies the signed
package and preserves the package's app data. Cleanup never replaces its own
executable or terminates file operations to apply an update. About also offers a
manual check when the installed package has an App Installer association.

Direct MSIX and development installations do not automatically gain that association.
Updates cannot be verified end-to-end until two production-signed releases have
been installed on a clean Windows 11 machine.

See Microsoft's [App Installer update settings](https://learn.microsoft.com/en-us/windows/msix/app-installer/update-settings)
and [feed format](https://learn.microsoft.com/en-us/windows/msix/app-installer/how-to-create-appinstaller-file).

## Website deployment

Vercel builds the `website` directory with Bun, with source files outside the root
enabled so it can read `release.json`. `website/vercel.json` specifies build,
output, asset caching, and security headers. Git integration deploys changes from
`main`; GitHub's Website workflow independently runs type checks, tests, and build.

SEO is generated at build time: canonical URLs, absolute social images, software
metadata, robots.txt, and sitemap.xml. Preview builds use noindex. Set `siteUrl`
to the stable production domain, not a deployment-specific preview URL.

The latest-release link stays visible. Direct installation remains unavailable
until a signed release is published. Then set `appInstallerUrl` in `release.json`
to the feed URL and redeploy. The installer button changes to **Install for Windows**.
Configure WinGet only after the community package has actually been accepted.
