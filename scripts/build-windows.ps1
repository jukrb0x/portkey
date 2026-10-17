param([switch]$SkipRust, [switch]$SkipArchive, [string]$PlatformToolset)
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
if (-not $SkipRust) {
    cargo build --locked --release --lib --bin portkey-helper
    if ($LASTEXITCODE -ne 0) { throw 'Rust controller/helper build failed' }
}
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw 'Install Visual Studio C++ desktop tools and Windows SDK 26100' }
if(-not $PlatformToolset) {$version=& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationVersion;$PlatformToolset=if([version]$version -ge [version]'18.0'){'v145'}else{'v143'}}
$msbuild = Join-Path $vs 'MSBuild\Current\Bin\MSBuild.exe'
& $msbuild native/windows/PortKey.vcxproj /restore /m /p:Configuration=Release /p:Platform=x64 "/p:PlatformToolset=$PlatformToolset" /verbosity:minimal
if ($LASTEXITCODE -ne 0) { throw 'WinUI 3 native build failed' }
$destination = 'dist/PortKey-Windows'
New-Item -ItemType Directory -Force $destination | Out-Null
Copy-Item target/release/portkey.dll "$destination/portkey.dll" -Force
Copy-Item target/release/portkey-helper.exe "$destination/PortKeyHelper.exe" -Force
Copy-Item native/windows/MainWindow.xaml "$destination/MainWindow.xaml" -Force
Copy-Item README.md "$destination/README.md" -Force
if (Test-Path THIRD_PARTY_NOTICES.md) { Copy-Item THIRD_PARTY_NOTICES.md $destination -Force }
Copy-Item native/windows/README.md "$destination/Windows-README.md" -Force
Copy-Item scripts/install-windows-runtime.ps1 "$destination/Install-WindowsRuntime.ps1" -Force
$licenses=Join-Path $destination 'Licenses'
New-Item -ItemType Directory -Force $licenses | Out-Null
Copy-Item ThirdParty/m1ddc/LICENSE "$licenses/m1ddc-LICENSE.txt" -Force
Copy-Item ThirdParty/m1ddc/UPSTREAM.md "$licenses/m1ddc-UPSTREAM.md" -Force
$nugetRoot=if($env:NUGET_PACKAGES){$env:NUGET_PACKAGES}else{Join-Path $env:USERPROFILE '.nuget/packages'}
Copy-Item (Join-Path $nugetRoot 'microsoft.windowsappsdk.winui/2.3.9/license.txt') "$licenses/WindowsAppSDK-license.txt" -Force
# These transitive SDK build artifacts are unused: the app constructs no WebView.
Remove-Item "$destination/Microsoft.Web.WebView2.Core.dll","$destination/Microsoft.Web.WebView2.Core.winmd","$destination/PortKey.pdb" -Force -ErrorAction SilentlyContinue
& (Join-Path $PSScriptRoot 'test-windows-package.ps1') -BinaryDirectory $destination
if (-not $SkipArchive) { Compress-Archive -Path "$destination/*" -DestinationPath dist/PortKey-Windows.zip -Force }
Write-Output "Built native C++/WinRT WinUI 3 app in $destination"
