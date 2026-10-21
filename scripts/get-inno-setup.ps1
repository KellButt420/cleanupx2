# Installs only the pinned build tool into ignored workspace artifacts. No admin rights.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$directory = Join-Path $root 'artifacts/tools/inno-7.1.0'
$compiler = Join-Path $directory 'ISCC.exe'
if (Test-Path $compiler) { return $compiler }
$download = Join-Path $root 'artifacts/tools/innosetup-7.1.0-x64.exe'
New-Item -ItemType Directory -Path (Split-Path $download) -Force | Out-Null
Invoke-WebRequest 'https://github.com/jrsoftware/issrc/releases/download/is-7_1_0/innosetup-7.1.0-x64.exe' -OutFile $download
$expected = '0362a383ed217d4c4239b5933866dd96d3eb2102737da92f80f6057a4b40df2f'
if ((Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { throw 'Inno Setup download hash mismatch.' }
$process = Start-Process -FilePath $download -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/CURRENTUSER',('/DIR="' + $directory + '"'),'/NOICONS') -WindowStyle Hidden -Wait -PassThru
if ($process.ExitCode -ne 0 -or -not (Test-Path $compiler)) { throw "Inno Setup tool installation failed: $($process.ExitCode)" }
$compiler
