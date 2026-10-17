# Deterministic mocks only: never enumerates real devices or opens a display.
$ErrorActionPreference = 'Stop'
$source = Join-Path $PSScriptRoot '../src/platform/windows.ps1'
$tokens = $null; $errors = $null
$tree = [Management.Automation.Language.Parser]::ParseFile($source, [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw 'Bridge syntax is invalid' }
$function = $tree.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Get-PortKeyHubs' }, $true)
if ($null -eq $function) { throw 'Production USB function not found' }
Invoke-Expression $function.Extent.Text
function Device($id, $service, $status = 'OK') {
    [pscustomobject]@{InstanceId=$id; Status=$status; FriendlyName='Hub'; CimInstanceProperties=@{Service=[pscustomobject]@{Value=$service}}}
}
function Property($id, $paths) {
    [pscustomobject]@{InstanceId=$id; KeyName='DEVPKEY_Device_LocationPaths'; Data=$paths}
}
function Reset-Fixture {
    $script:devices = @((Device 'USB\HUB1' 'USBHUB3'), (Device 'USB\HUB2' 'USBHUB'), (Device 'USB\CONTROLLER' 'USBXHCI'))
    $script:locations = @{
        'USB\HUB1' = @(Property 'USB\HUB1' @('PCIROOT(0)#PCI(1)#USBROOT(0)#USB(2)', 'ACPI(_SB_)#ACPI(USB0)#ACPI(RHUB)#ACPI(PRT2)'))
        'USB\HUB2' = @(Property 'USB\HUB2' @('PCIROOT(0)#PCI(1)#USBROOT(0)#USB(3)'))
    }
    $script:failEnumeration = $false; $script:failProperty = $false; $script:queries = 0
}
function Get-PnpDevice {
    param([switch]$PresentOnly, $Class, $ErrorAction)
    if ($script:failEnumeration) { throw 'Injected enumeration failure' }
    return $script:devices
}
function Get-PnpDeviceProperty {
    param($InstanceId, $KeyName, $ErrorAction)
    if ($KeyName -ne 'DEVPKEY_Device_LocationPaths') { throw 'Unexpected repeated service query' }
    $script:queries++
    if ($script:failProperty) { throw 'Injected property failure' }
    return $script:locations[$InstanceId]
}
function Assert-Failure([scriptblock]$change) {
    Reset-Fixture
    & $change
    $failed = $false
    try { $null = @(Get-PortKeyHubs) } catch { $failed = $true }
    if (-not $failed) { throw 'Incomplete inventory was accepted' }
}
Reset-Fixture
$result = @(Get-PortKeyHubs)
if ($result.Count -ne 2 -or $script:queries -ne 2) { throw 'Incorrect hub filtering/query count' }
if ($result[0].id -cne 'windows:acpi(_sb_)#acpi(usb0)#acpi(rhub)#acpi(prt2)' -or $result[1].id -cne 'windows:pciroot(0)#pci(1)#usbroot(0)#usb(3)') { throw 'Full sorted topology changed' }
Assert-Failure { $script:devices[0].CimInstanceProperties.Remove('Service') }
foreach ($service in @($null, '')) {
    Reset-Fixture
    $script:devices[0].CimInstanceProperties.Service.Value = $service
    $result = @(Get-PortKeyHubs)
    if ($result.Count -ne 1 -or $script:queries -ne 1) { throw 'Empty service must remain a non-hub' }
}
Assert-Failure { $script:devices[0].CimInstanceProperties.Service.Value = 9 }
Assert-Failure { $script:devices += $script:devices[0] }
Assert-Failure { $script:devices[0].Status = 'Error' }
Assert-Failure { $script:devices = @() }
Assert-Failure { $script:devices = @((Device 'USB\CONTROLLER' 'USBXHCI')) }
Assert-Failure { $script:locations['USB\HUB1'] = @() }
Assert-Failure { $script:locations['USB\HUB1'] += $script:locations['USB\HUB1'][0] }
Assert-Failure { $script:locations['USB\HUB1'][0].InstanceId = 'USB\OTHER' }
Assert-Failure { $script:locations['USB\HUB1'][0].Data = @() }
Assert-Failure { $script:locations['USB\HUB1'][0].Data = @('') }
Assert-Failure { $script:failEnumeration = $true }
Assert-Failure { $script:failProperty = $true }
Write-Output 'Windows USB inventory: 16 deterministic cases passed (mocked; no hardware access).'
