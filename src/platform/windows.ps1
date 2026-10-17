$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
try {
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class PortKeyNative {
    [StructLayout(LayoutKind.Sequential)] struct RECT { public int l,t,r,b; }
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] struct INFO {
        public uint cb; public RECT monitor,work; public uint flags;
        [MarshalAs(UnmanagedType.ByValTStr,SizeConst=32)] public string device;
    }
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] struct DEVICE {
        public uint cb;
        [MarshalAs(UnmanagedType.ByValTStr,SizeConst=32)] public string name;
        [MarshalAs(UnmanagedType.ByValTStr,SizeConst=128)] public string description;
        public uint flags;
        [MarshalAs(UnmanagedType.ByValTStr,SizeConst=128)] public string id;
        [MarshalAs(UnmanagedType.ByValTStr,SizeConst=128)] public string key;
    }
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] struct PHYSICAL {
        public IntPtr handle;
        [MarshalAs(UnmanagedType.ByValTStr,SizeConst=128)] public string description;
    }
    delegate bool Callback(IntPtr monitor,IntPtr dc,ref RECT rect,IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumDisplayMonitors(IntPtr dc,IntPtr clip,Callback callback,IntPtr data);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern bool GetMonitorInfo(IntPtr monitor,ref INFO info);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern bool EnumDisplayDevices(string device,uint index,ref DEVICE result,uint flags);
    [DllImport("dxva2.dll")] static extern bool GetNumberOfPhysicalMonitorsFromHMONITOR(IntPtr monitor,out uint count);
    [DllImport("dxva2.dll")] static extern bool GetPhysicalMonitorsFromHMONITOR(IntPtr monitor,uint count,[Out] PHYSICAL[] monitors);
    [DllImport("dxva2.dll")] static extern bool DestroyPhysicalMonitors(uint count,PHYSICAL[] monitors);
    [DllImport("dxva2.dll")] static extern bool SetVCPFeature(IntPtr monitor,byte code,uint value);
    [DllImport("dxva2.dll")] static extern bool GetVCPFeatureAndVCPFeatureReply(IntPtr monitor,byte code,out uint kind,out uint current,out uint maximum);
    public class Display { public string id; public string label; internal IntPtr handle; }
    public static Display[] Displays() {
        var result=new List<Display>(); string error=null;
        Callback callback=delegate(IntPtr monitor,IntPtr dc,ref RECT rect,IntPtr data) {
            INFO info=new INFO(); info.cb=(uint)Marshal.SizeOf(typeof(INFO));
            if(!GetMonitorInfo(monitor,ref info)){error="Monitor identity read failed";return false;}
            var matches=new List<DEVICE>();
            for(uint i=0;i<64;i++) {
                DEVICE d=new DEVICE(); d.cb=(uint)Marshal.SizeOf(typeof(DEVICE));
                if(!EnumDisplayDevices(info.device,i,ref d,1))break;
                if((d.flags & 1)!=0 && !String.IsNullOrEmpty(d.id)) matches.Add(d);
            }
            if(matches.Count!=1){error="Ambiguous or unavailable monitor interface";return false;}
            result.Add(new Display{id=matches[0].id.ToLowerInvariant(),label=matches[0].description,handle=monitor});
            return true;
        };
        if(!EnumDisplayMonitors(IntPtr.Zero,IntPtr.Zero,callback,IntPtr.Zero)) throw new Exception(error ?? "Monitor enumeration failed");
        if(error!=null)throw new Exception(error);
        return result.ToArray();
    }
    static string Access(string id,uint input,bool write) {
        var matches=new List<Display>();
        foreach(var d in Displays()) if(String.Equals(d.id,id,StringComparison.Ordinal))matches.Add(d);
        if(matches.Count!=1)throw new Exception("Enrolled display missing or ambiguous; no command sent");
        uint count;
        if(!GetNumberOfPhysicalMonitorsFromHMONITOR(matches[0].handle,out count)||count!=1)
            throw new Exception("Display does not map to exactly one physical monitor; no command sent");
        var physical=new PHYSICAL[count];
        if(!GetPhysicalMonitorsFromHMONITOR(matches[0].handle,count,physical))throw new Exception("Cannot open enrolled physical monitor");
        try {
            if(!write) {
                uint readKind,readCurrent,readMaximum;
                if(!GetVCPFeatureAndVCPFeatureReply(physical[0].handle,0x60,out readKind,out readCurrent,out readMaximum)
                    || readCurrent==0 || readCurrent>255)throw new Exception("DDC input readback unavailable or invalid");
                return readCurrent.ToString(System.Globalization.CultureInfo.InvariantCulture);
            }
            if(!SetVCPFeature(physical[0].handle,0x60,input))throw new Exception("DDC write failed; monitor outcome unknown");
            System.Threading.Thread.Sleep(1500);
            uint kind,current,maximum;
            return GetVCPFeatureAndVCPFeatureReply(physical[0].handle,0x60,out kind,out current,out maximum) && current==input ? "confirmed" : "sent";
        } finally { DestroyPhysicalMonitors(count,physical); }
    }
    public static string Read(string id) { return Access(id,0,false); }
    public static string Write(string id,uint input) {
        if(input==0 || input>255)throw new Exception("Invalid display input");
        return Access(id,input,true);
    }
}
'@
if ($env:PORTKEY_MODE -eq 'write') {
    [PortKeyNative]::Write($env:PORTKEY_DISPLAY, [uint32]$env:PORTKEY_INPUT)
} elseif ($env:PORTKEY_MODE -eq 'read') {
    [PortKeyNative]::Read($env:PORTKEY_DISPLAY)
} elseif ($env:PORTKEY_MODE -eq 'inventory') {
    $hubs = @()
    $devices = @(Get-PnpDevice -PresentOnly -Class USB -ErrorAction Stop)
    if ($devices.Count -eq 0) { throw 'USB inventory is empty; automation paused' }
    foreach ($device in $devices) {
        $service = (Get-PnpDeviceProperty -InstanceId $device.InstanceId -KeyName 'DEVPKEY_Device_Service' -ErrorAction Stop).Data
        if ($service -match '^USBHUB(3)?$') {
            if ($device.Status -ne 'OK') { throw 'USB hub status is uncertain; automation paused' }
            $paths = @((Get-PnpDeviceProperty -InstanceId $device.InstanceId -KeyName 'DEVPKEY_Device_LocationPaths' -ErrorAction Stop).Data | Sort-Object)
            if ($paths.Count -eq 0 -or [string]::IsNullOrWhiteSpace($paths[0])) { throw 'USB hub lacks a stable location path' }
            $hubs += @{ id = 'windows:' + $paths[0].ToLowerInvariant(); label = $device.FriendlyName + ' @ ' + $paths[0] }
        }
    }
    if ($hubs.Count -eq 0) { throw 'No USB root hubs could be enumerated; automation paused' }
    # WMI names are cosmetic only; join the entire PnP instance, never a model/VID prefix.
    $monitorNames = @(Get-CimInstance -Namespace root/wmi -ClassName WmiMonitorID -ErrorAction SilentlyContinue)
    $displays = @([PortKeyNative]::Displays() | ForEach-Object {
        $display = $_
        $label = $display.label
        $parts = $display.id -split '#'
        if ($parts.Count -eq 4 -and $parts[0] -ieq '\\?\display') {
            $instance = 'DISPLAY\' + $parts[1] + '\' + $parts[2]
            $names = @($monitorNames | Where-Object { ($_.InstanceName -replace '_[0-9]+$', '') -ieq $instance })
            if ($names.Count -eq 1) {
                $name = -join @($names[0].UserFriendlyName | Where-Object { $_ -ne 0 } | ForEach-Object { [char]$_ })
                $manufacturer = -join @($names[0].ManufacturerName | Where-Object { $_ -ne 0 } | ForEach-Object { [char]$_ })
                if (-not [string]::IsNullOrWhiteSpace($name)) { $label = ($manufacturer + ' ' + $name).Trim() }
            }
        }
        @{id=$display.id;label=$label + ' · ' + $display.id}
    })
    @{ displays = $displays; hubs = @($hubs) } | ConvertTo-Json -Depth 5 -Compress
} else { throw 'Invalid hardware helper mode' }
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
