PortKey Windows native frontend
==============================

Build on Windows x64 with Rust stable, Visual Studio C++ desktop tools (VS 2022 v143 or VS 2026 v145),
Windows SDK 10.0.26100 and network access for NuGet restore:

    powershell -ExecutionPolicy Bypass -File scripts/build-windows.ps1

This builds a native C++/WinRT + XAML WinUI 3 desktop application. No CLR,
WebView or browser is used. The pinned Microsoft Windows App SDK 2.5.1 runtime
(x64), including its DDLM package, must be installed from Microsoft.
Run the packaged Install-WindowsRuntime.ps1 explicitly to install the signed
Microsoft redistributable, or use the official download:
https://aka.ms/windowsappsdk/2.5/2.5.1/windowsappruntimeinstall-x64.exe
Startup uses an explicit bootstrap call without a hidden no-match modal; missing
runtime prerequisites produce an understandable launch error before creating
the controller. Existing runtime versions are retained. NuGet also restores upstream transitive
build dependencies; their presence does not mean the app uses WebView2.

Keep PortKey.exe, portkey.dll, PortKeyHelper.exe, MainWindow.xaml and generated
resources/bootstrap files together. Launch PortKey.exe. PortKeyHelper.exe is the
Rust bounded DDC subprocess and must not be used as the product launcher.
The controller DLL resolves that helper beside its own module and fails when it
is missing; it never falls back to launching the WinUI executable.

Closing the window hides it to the notification area. Open the tray menu for
named display destinations, local automatic switching, settings and Quit. The
200 ms UI-thread controller tick and USB observer remain active while hidden.
Windows suspend/resume is registered by the shared controller. The exact USB
interface notifications are discovery hints, never ownership evidence.

Setup and pause affect this app only. No peer liveness or global manual hold is
implied. New setups remain unenrolled and off; qualified setup evidence must be
explicitly recorded. Profiles follow exact enrolled displays/topologies.

An isolated startup smoke can be run with scripts/test-windows-startup.ps1.
It creates a fresh configuration directory, never copies live enrollment, and
exits after visible/hidden ticks. It performs inventory only and sends no DDC
commands. Run it in an interactive desktop session. The live PortKey controller
can remain running because the isolated QA configuration has its own lock.

The pinned SDK 2.5.1 composition uses native component packages WinUI 2.3.9,
Foundation 2.3.12, InteractiveExperiences 2.1.9 and Runtime 2.5.1, matching the
Microsoft.WindowsAppSDK 2.5.1 stable metapackage. Unused AI, ML, Search and Widget
SDK components are not restored. C++/WinRT is pinned at 2.0.250303.1.
The build script detects v143/v145; CI toolchain/runtime behavior requires its
own runner validation. Actual local Windows validation used VS 2026 v145.

Profiles use independent stable IDs: several HDMI/cable mappings can share one
exact enrolled display. Automatic recognition chooses the connected physical
display and its sole or explicitly remembered wiring profile. An unresolved
choice waits; PortKey never guesses cables from USB. The Profiles page separates
the active profile from the viewed profile, supports explicit local selection,
and can restore automatic recognition. An offline manual choice waits for that
exact display. Selection changes this computer only; choose matching wiring
profiles on the other computer independently. Browsing never activates a profile,
and profile settings edits are pinned to profile ID.
