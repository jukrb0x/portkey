# Explicit per-user prerequisite installation. Never downloads or installs from the app's startup path.
$ErrorActionPreference='Stop'
$url='https://aka.ms/windowsappsdk/2.5/2.5.1/windowsappruntimeinstall-x64.exe'
$destination=Join-Path ([IO.Path]::GetTempPath()) ('PortKey-WindowsRuntime-'+[Guid]::NewGuid().ToString('N')+'.exe')
try {
    Invoke-WebRequest -Uri $url -OutFile $destination
    $signature=Get-AuthenticodeSignature $destination
    if($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notlike '*Microsoft Corporation*'){throw 'Microsoft runtime installer signature validation failed'}
    Get-FileHash $destination -Algorithm SHA256 | Format-List
    $process=Start-Process -FilePath $destination -ArgumentList '--quiet' -PassThru
    if(-not $process.WaitForExit(120000)){Stop-Process -Id $process.Id;throw 'Runtime installer exceeded 120 seconds'}
    if($process.ExitCode -ne 0){throw "Runtime installer failed: $($process.ExitCode)"}
    $framework=Get-AppxPackage 'Microsoft.WindowsAppRuntime.2' | Where-Object {$_.Architecture -eq 'X64' -and $_.Version -ge [version]'2.5.1.0'}
    $ddlm=Get-AppxPackage 'Microsoft.WinAppRuntime.DDLM.2.5.1.0-x6'
    if(-not $framework -or -not $ddlm){throw 'Required x64 framework/DDLM packages are missing after installation'}
    $framework,$ddlm | Select-Object Name,Version,Architecture | Format-Table
} finally {Remove-Item $destination -Force -ErrorAction SilentlyContinue}
