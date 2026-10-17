param([string]$BinaryDirectory = (Join-Path $PSScriptRoot '../dist/PortKey-Windows'), [string]$OutputDirectory)
$ErrorActionPreference='Stop'
$BinaryDirectory=(Resolve-Path $BinaryDirectory).Path
if (-not $OutputDirectory) { $OutputDirectory=Join-Path ([IO.Path]::GetTempPath()) ('PortKey-native-qa-'+[Guid]::NewGuid().ToString('N')) }
$OutputDirectory=[IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path $OutputDirectory) { throw 'QA output directory must be fresh; refusing any pre-existing data' }
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
$configuration=Join-Path $OutputDirectory 'isolated-config'
New-Item -ItemType Directory -Path $configuration | Out-Null
$previousDirectory=$env:PORTKEY_CONFIG_DIR
$previousResult=$env:PORTKEY_SMOKE_RESULT
$previousTiming=$env:PORTKEY_TIMING_LOG
try {
    $env:PORTKEY_TIMING_LOG=Join-Path $OutputDirectory 'qa-timing.jsonl'
    $env:PORTKEY_CONFIG_DIR=$configuration
    $env:PORTKEY_SMOKE_RESULT=Join-Path $OutputDirectory 'result.json'
    $process=Start-Process -FilePath (Join-Path $BinaryDirectory 'PortKey.exe') -ArgumentList '--smoke-test' -WorkingDirectory $BinaryDirectory -PassThru
    $mainResult=$env:PORTKEY_SMOKE_RESULT
    $ready=$false
    for($i=0;$i -lt 30;$i++){if(Test-Path "$mainResult.trace"){$ready=@(Get-Content "$mainResult.trace" -ErrorAction SilentlyContinue) -contains 'controller_created';if($ready){break}};Start-Sleep -Milliseconds 100}
    if(-not $ready){throw 'Primary QA controller did not reach its isolated readiness stage'}
    $env:PORTKEY_SMOKE_RESULT=Join-Path $OutputDirectory 'blocked-startup.json'
    $blocked=Start-Process -FilePath (Join-Path $BinaryDirectory 'PortKey.exe') -ArgumentList '--blocked-smoke-test' -WorkingDirectory $BinaryDirectory -PassThru
    if(-not $blocked.WaitForExit(10000)){Stop-Process -Id $blocked.Id;throw 'Read-only blocked startup exceeded 10 seconds'}
    if($blocked.ExitCode -ne 0 -or -not(Test-Path $env:PORTKEY_SMOKE_RESULT)){throw 'Blocked startup did not produce a diagnostic result'}
    $blockedResult=Get-Content $env:PORTKEY_SMOKE_RESULT -Raw | ConvertFrom-Json
    if(-not $blockedResult.blocked_startup_diagnostics){throw 'Blocked startup did not retain read-only diagnostics and disabled setup/hardware controls'}
    Write-Output (Get-Content $env:PORTKEY_SMOKE_RESULT -Raw)
    $env:PORTKEY_SMOKE_RESULT=$mainResult
    if(-not $process.WaitForExit(25000)){Stop-Process -Id $process.Id;throw 'Native startup smoke exceeded 25-second bound'}
    if($process.ExitCode -ne 0){throw "Native app exited with code $($process.ExitCode)"}
    if(-not(Test-Path $env:PORTKEY_SMOKE_RESULT)){throw 'Native smoke did not produce evidence'}
    $result=Get-Content $env:PORTKEY_SMOKE_RESULT -Raw | ConvertFrom-Json
    foreach($key in 'config_null','lifecycle_ok','usb_notifications_ready','continued_ticks_while_hidden','automatic_off','unsaved_draft_preserved_across_language','chinese_labels_rendered','native_xaml_capture_complete','stale_attestations_cleared','invalid_draft_retained','mode_edit_orchestration','async_error_recovery','new_setup_reset','tray_registered','tray_icon_loaded','shared_diagnostics_preview','qualified_finish_orchestration','independent_profile_actions','collapsed_sidebar_settings') {if(-not $result.$key){throw "Native smoke failed: $key"}}
    $profileFile=Join-Path $configuration 'profiles-rust.json'
    if(Test-Path $profileFile){$saved=Get-Content $profileFile -Raw | ConvertFrom-Json;if(@($saved.profiles).Count -gt 0){throw 'QA unexpectedly created enrollment'}}
    Write-Output (Get-Content $env:PORTKEY_SMOKE_RESULT -Raw)
    $env:PORTKEY_SMOKE_RESULT=Join-Path $OutputDirectory 'reconstruction.json'
    $restarted=Start-Process -FilePath (Join-Path $BinaryDirectory 'PortKey.exe') -ArgumentList '--smoke-test --restore-editor' -WorkingDirectory $BinaryDirectory -PassThru
    if(-not $restarted.WaitForExit(15000)){Stop-Process -Id $restarted.Id;throw 'Draft reconstruction smoke exceeded 15 seconds'}
    if($restarted.ExitCode -ne 0 -or -not(Test-Path $env:PORTKEY_SMOKE_RESULT)){throw 'Draft reconstruction process did not complete'}
    $reconstructed=Get-Content $env:PORTKEY_SMOKE_RESULT -Raw | ConvertFrom-Json
    foreach($key in 'config_null','automatic_off','process_reconstructed_incomplete_draft','new_setup_reset','tray_registered','tray_icon_loaded'){if(-not $reconstructed.$key){throw "Draft reconstruction failed: $key"}}
    if(Test-Path (Join-Path $configuration 'windows-setup-editor.json')){throw 'Discard did not clear native editor state'}
    Write-Output (Get-Content $env:PORTKEY_SMOKE_RESULT -Raw)
    $transport=@(Get-Content (Join-Path $OutputDirectory 'qa-timing.jsonl') | ForEach-Object {try{$_ | ConvertFrom-Json}catch{}} | Where-Object {$_.event -eq 'helper_start' -and $_.role -in 'read','write'})
    if($transport.Count -gt 0){throw 'Isolated QA unexpectedly invoked display transport'}
    Write-Output 'No display-read/write helper transport was invoked in isolated QA.'
    if(-not(Test-Path (Join-Path $OutputDirectory 'native-settings-collapsed.png'))){throw 'Missing collapsed sidebar settings screenshot'}
    Write-Output "Native XAML settings screenshot: $(Join-Path $OutputDirectory 'native-settings-collapsed.png')"
    Write-Output "Native XAML screenshot: $(Join-Path $OutputDirectory 'native-window.png')"
} finally {
    $env:PORTKEY_CONFIG_DIR=$previousDirectory
    $env:PORTKEY_SMOKE_RESULT=$previousResult
    $env:PORTKEY_TIMING_LOG=$previousTiming
}
