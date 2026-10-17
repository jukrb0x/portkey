# Platform references

The desktop shares a Rust controller and hardware core. macOS uses Swift/AppKit linked to a Rust static library through `native/portkey.h`; Windows uses Win32 controls and `Shell_NotifyIcon` through `windows-sys`. The native event loops poll the controller while settings are hidden. No egui, eframe, tray-icon or webview remains. The Cargo lockfile fixes the resolved dependency graph. See [the interface contract](plans/native-ui.md).

macOS display UUID discovery explicitly links ColorSync, which exports `CGDisplayCreateUUIDFromDisplayID` in the installed SDK. The packaging verifier checks AppKit linkage, deployment targets, helper provenance and signatures.

Windows DDC uses the [low-level monitor configuration functions](https://learn.microsoft.com/en-us/windows/win32/monitor/using-the-low-level-monitor-configuration-functions), including [SetVCPFeature](https://learn.microsoft.com/en-us/windows/win32/api/lowlevelmonitorconfigurationapi/nf-lowlevelmonitorconfigurationapi-setvcpfeature). A physical monitor handle is transient, resolved from the enrolled display on each command; it is never saved as identity. Hardware and drivers determine whether DDC/CI is available while another input is selected.

The existing Mac helper's license and exact upstream revision remain in [ThirdParty/m1ddc](../ThirdParty/m1ddc/UPSTREAM.md). Local patches validate Get-VCP replies and their request checksum; see provenance and `scripts/test-ddc.sh`. Successful Set transport remains distinct from visible picture change.
