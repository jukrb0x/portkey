# PortKey

**Switch your USB. Let your display follow.**

PortKey connects a physical two-host USB switch to your monitor's input selection. Its shared Rust core runs behind native desktop interfaces: Swift/AppKit on macOS and Win32 controls on Windows, with a small menu-bar/system-tray control and an English / Simplified Chinese setup window. Everything stays on your computer: no account, server, network listener, administrator service, keyboard capture or mouse tracking.

## Rust preview · 0.2

This branch replaces the personal, fixed-hardware setup with explicit onboarding: choose your display, enroll the two shared USB hubs, and map the monitor inputs to **this computer** and **the other computer**. No personal hardware identifiers are shipped as Rust defaults. First run is unenrolled and automation is off.

The Rust app is a development preview. macOS builds and read-only discovery are checked locally; Windows has an implemented native API bridge and cross-target compilation checks, but needs Windows runtime and hardware acceptance. This is not a claim that every monitor, dock or USB switch works. The historical Swift app remains available as a fallback; its physical acceptance results do not qualify the Rust rewrite. See [legacy instructions](docs/SWIFT-README.md).

## Supported setup

- A monitor supporting DDC/CI input selection (VCP `0x60`), enabled in its on-screen menu.
- Two awake computers, each with its own video cable to the monitor.
- A two-host USB switch exposing **two shared hubs**, commonly USB2 and USB3. Single-hub switches are not supported in this preview.
- macOS 13+ on Apple Silicon for the bundled m1ddc backend, or Windows 10/11 with Windows PowerShell 5.1 and the PnpDevice module. Managed Windows policies may block the native bridge.

Run automatic switching on **only one controller computer**. Both platforms can act as controller; running them both automatically against one display can cause competing writes. Quit the legacy Swift app before enabling the Rust controller.

DDC availability when another video input is selected depends on the monitor, GPU, dock and cable. A disconnected monitor, duplicate identity, unsupported DDC or ambiguous physical-monitor mapping fails closed. There is no automatic fallback to another display.

## Build and run

Install stable Rust. On macOS install Apple Command Line Tools. The lockfile fixes dependency versions; no webview or Node runtime is used.

```sh
./scripts/test-rust.sh
./scripts/build-rust-app.sh
open dist/PortKey-Rust.app
```

The Mac script links the Rust static library into the AppKit executable, bundles a diagnostic CLI and the pinned m1ddc helper and its license, then ad-hoc signs `dist/PortKey-Rust.app`. This is a local development build, not a notarized public download. An optional repository-local Rust toolchain in ignored `.tools/` is supported without modifying shell dotfiles.

On Windows, install Rust with the MSVC toolchain and Visual Studio C++ build tools, then run PowerShell in the repository:

```powershell
cargo test --locked
./scripts/build-windows.ps1
./dist/PortKey-Windows/PortKey.exe
```

The script also creates `dist/PortKey-Windows.zip`. No elevated application privileges are needed. The Windows bridge uses system PowerShell to invoke native Win32 monitor APIs in a bounded child process; it never installs a script or changes execution policy. Windows CI builds and packages on `windows-latest`; macOS CI builds the app bundle. CI configuration is provided; local work does not publish a release.

## First-run setup

1. Connect the USB switch to this computer and keep both hosts awake.
2. Select the intended monitor. Inspect its stable identity in the device selector (macOS also provides tooltips).
3. Select the **two shared hubs**. Switch USB away and refresh to determine which pair disappears; switch back and select that pair. Identical models are distinguished by physical topology, including parent/controller path. A keyboard-only disconnect must not remove your selected hubs.
4. Set each host's VCP input value. Common values are DisplayPort `15`/`16` and HDMI `17`/`18`; check your display's documentation. Save the enrollment.
5. Use the manual buttons to test each direction while watching the display. Only then enable automatic switching.

A command being sent, a monitor reporting its input and a visible picture change are different evidence. The pinned Mac helper does not fully validate DDC replies, so the Rust app reports Mac writes as **sent, unverified**. Windows readback confirmation is still not proof of the visible picture.

Use the tray/menu-bar menu to open settings, pause or select a host. Closing the settings window keeps the controller in the tray; choose **Quit PortKey** to exit. Startup failures are reported rather than leaving a hidden controller without accessible controls.

## Home and work setups

Save a named setup for each display, with its own USB hub pair and host input mapping. Connecting exactly one enrolled display activates its saved setup automatically. Use **New setup** to enroll another environment without replacing the first. New/unknown displays require enrollment once; identical models are matched by identity, never by their displayed name alone.

No matching display or multiple connected enrolled displays pauses selection. Disconnect all but the intended enrolled display before switching. Moving USB ports/docks can change hub topology and require editing that environment's setup. Merely plugging in an HDMI/DP cable does not establish USB ownership: the physical USB switch remains required.

## Safe behavior and recovery

- Both enrolled hubs must be observed locally and settle before departure can select the other host. Startup absence never means the other computer owns USB.
- A partial pair or failed inventory cancels pending automatic work. Peripheral-only changes do not change ownership.
- Manual selection supersedes queued automation until a genuine USB ownership transition.
- Sleep/resume notifications invalidate pending work and clear ownership. Wake with absent hubs remains unknown. Enumeration and controller/display readiness may delay recovery.
- All operations use a serialized worker. At most three attempts with cancellation-aware delays. Helper processes have deadlines and capped, concurrently drained output. A write already inside a driver cannot be undone; a newer valid request runs afterward.
- Pause takes effect even if saving preferences fails. Invalid configuration cannot silently load an enrollment or enable writes.
- Moving a hub, dock or cable can change topology: edit the setup and enroll it again.

Unplugging or powering off the entire USB switch looks like switching to the other host. PortKey cannot distinguish these cases, wake arbitrary hosts, or control the physical USB switch.

## Data and migration

Rust settings are stored separately from the old Swift file:

| Platform | Configuration |
| --- | --- |
| macOS | `~/Library/Application Support/PortKey/profiles-rust.json` |
| Windows | `%APPDATA%\PortKey\profiles-rust.json` |

Existing single-display `config-rust.json` is read as one profile when no profile store exists. The old file is preserved; subsequent saves use `profiles-rust.json`.

Saved configuration contains profile names and only display/hub identities, input mapping, automation and language preferences. Language selection is also kept locally for first-run setup. Invalid JSON displays an error; save a reviewed replacement from setup to recover.

On macOS, **Import Swift setup…** reads the old `config.json`. It does not overwrite that file and imports with automation off. Review and save explicitly. Never share a raw configuration or diagnostic capture without inspecting its device identifiers.

For opt-in **read-only** discovery (no DDC writes):

```sh
dist/PortKey-Rust.app/Contents/MacOS/PortKey --diagnose
```

Quit and remove the app/folder to uninstall. Optionally remove the Rust configuration and local language preference. The app installs no login item, global helper or service.

## Development and evidence

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

Tests cover policy transitions, topology identity, configuration failure/migration, latest-request dispatch, manual overrides, pause, lifecycle invalidation, helper deadline/output handling and exact display selection. They do not send hardware writes. Physical switching and sleep acceptance require coordinated foreground tests.

See [the native UI architecture](docs/plans/native-ui.md), [the original Rust plan](docs/plans/rust-product.md), [platform references](docs/RUST-REFERENCE.md), and [current verification](docs/RUST-VERIFICATION.md). Preserve [m1ddc provenance and license](ThirdParty/m1ddc/UPSTREAM.md). Historical Swift tests remain available through `scripts/test.sh`.
