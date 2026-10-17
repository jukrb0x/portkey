param([string]$BinaryDirectory=(Join-Path $PSScriptRoot '../dist/PortKey-Windows'))
$ErrorActionPreference='Stop'
$BinaryDirectory=(Resolve-Path $BinaryDirectory).Path
foreach($name in 'PortKey.exe','portkey.dll','PortKeyHelper.exe','MainWindow.xaml','PortKey.pri','Microsoft.WindowsAppRuntime.Bootstrap.dll','Install-WindowsRuntime.ps1') {
    if(-not(Test-Path (Join-Path $BinaryDirectory $name))){throw "Missing package component: $name"}
}
foreach($name in 'PortKey.exe','portkey.dll','PortKeyHelper.exe') {
    $bytes=[IO.File]::ReadAllBytes((Join-Path $BinaryDirectory $name))
    $pe=[BitConverter]::ToInt32($bytes,0x3c)
    if([BitConverter]::ToUInt16($bytes,$pe+4) -ne 0x8664){throw "$name is not x64"}
    $optional=$pe+24
    if([BitConverter]::ToUInt16($bytes,$optional) -ne 0x20b){throw "$name is not PE32+"}
    $clr=$optional+112+14*8
    if([BitConverter]::ToUInt64($bytes,$clr) -ne 0){throw "$name unexpectedly contains a CLR runtime descriptor"}
}
# Incorrect helper invocation has no controller/discovery path and must never open a UI.
$helper=Start-Process -FilePath (Join-Path $BinaryDirectory 'PortKeyHelper.exe') -ArgumentList '--package-no-operation-test' -PassThru
if(-not $helper.WaitForExit(3000)){Stop-Process -Id $helper.Id;throw 'Dedicated helper launched a process that did not terminate'}
if($helper.ExitCode -ne 2){throw "Dedicated helper unexpectedly accepted a non-helper invocation: $($helper.ExitCode)"}
if(Test-Path (Join-Path $BinaryDirectory 'Microsoft.Web.WebView2.Core.dll')){throw 'Unused WebView binary was included'}
Get-FileHash (Join-Path $BinaryDirectory 'PortKey.exe'),(Join-Path $BinaryDirectory 'portkey.dll'),(Join-Path $BinaryDirectory 'PortKeyHelper.exe') -Algorithm SHA256 | Select-Object @{N='Name';E={Split-Path $_.Path -Leaf}},Hash | ConvertTo-Json
Write-Output 'Windows x64 native package checks passed; no CLR descriptor; dedicated helper rejects UI startup.'
