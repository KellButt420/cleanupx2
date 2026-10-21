$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'env.ps1')
Push-Location $RepoRoot
try {
    cargo fmt --manifest-path src/rust/Cargo.toml --all --check
    if ($LASTEXITCODE -ne 0) { throw 'Rust format check failed.' }
    cargo clippy --locked --manifest-path src/rust/Cargo.toml --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Clippy failed.' }
    cargo test --locked --manifest-path src/rust/Cargo.toml
    if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed.' }
} finally { Pop-Location }
