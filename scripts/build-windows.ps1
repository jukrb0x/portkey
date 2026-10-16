$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
cargo build --locked --release
if ($LASTEXITCODE -ne 0) { throw 'Cargo build failed' }
New-Item -ItemType Directory -Force dist/PortKey-Windows | Out-Null
Copy-Item target/release/portkey.exe dist/PortKey-Windows/PortKey.exe
Copy-Item README.md dist/PortKey-Windows/README.md
Compress-Archive -Path dist/PortKey-Windows/* -DestinationPath dist/PortKey-Windows.zip -Force
Write-Output 'Built dist/PortKey-Windows.zip'
