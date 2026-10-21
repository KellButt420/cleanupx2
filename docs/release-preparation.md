# Release preparation

## Source repository

The owner selected MIT. `LICENSE`, Cargo workspace/crate metadata and the managed
project identify that license. The packaged app includes the project's license and
third-party notice summary. Contributions remain subject to dependency licenses.

Before the first push, inspect the source tree and ignored outputs. `artifacts/`,
native/managed build outputs, local environment files, private signing keys and
diagnostic dumps must stay outside the public repository. UI verification evidence
may contain local application names or paths. Do not attach it without review.

The launch target is `wiredincorp/cleanup`. See [launch and updates](launch.md) for
the repository, website, support links, production packaging, and release workflow.

## CI

`.github/workflows/build.yml` uses Windows x64, the repository's .NET/Rust toolchain
pins and committed Cargo/NuGet lock files. Official GitHub actions are pinned to
commit hashes. The job runs formatting, strict Clippy and Rust tests, builds a
development-signed MSIX and uploads that package, x64 framework dependencies, the
public development certificate and SHA-256 checksums. It never exports a private
key, installs the app, approves UAC or publishes a release.

The workflow needs its first run on GitHub after the source is pushed. A local
package build is not evidence that the hosted runner job has passed.

## Public binary release gates

1. Choose a stable release identity, publisher and signing method. Keep production
   signing separate from `Cleanup.Dev` and the local development certificate.
   Use protected release credentials; do not add a private certificate to source.
2. Audit and include the exact dependency/runtime license notices required by the
   binary distribution. The root summary is not a substitute for those notices.
3. Build with locked dependencies, verify package signature and hashes, and test
   installation, first launch, dependencies, update, state retention and uninstall
   on a disposable clean Windows 11 x64 machine.
4. Run the fixture harness against the installed MSIX, including human-approved
   UAC consent/refusal. Verify MSI busy/restart outcomes and completion refresh.
   Never target the tester's real installed applications.
5. Finish the documented filesystem failure, accessibility and managed test matrix.
   Measure cold startup and memory on named hardware. Current memory observations
   exceed the original 120 MB target; no release performance claim is justified.
6. Review sanitized beta notes, produce final SHA-256 checksums, and manually
   approve publication of the signed release and required framework dependencies.

Development artifacts remain separate from the unsigned Release candidate.
The signed release workflow needs trusted signing credentials and clean-machine
validation before any installer is published. ARM64 and Microsoft Store submission
remain outside this beta preparation.
