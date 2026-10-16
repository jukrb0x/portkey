# Approved implementation brief — 2026-10-16

The user explicitly asked to build the Mac-only app now, create an independent project under ~/dev/, initialize Git, keep advancing, add a small UI/menu bar, and test repeated switching and sleep recovery. They delegated architecture choices. They also authorized a new local Codex chat and a Codex project for this work.

The older design/plan remain research history; use the coherent files for implementation.

## Product name

The user requested a new name inspired by Harry Potter, Minecraft, London Underground or Sherlock. The selected product/project name is **Portkey** (Harry Potter’s transportation object, with a USB port association). Swift package, executable, bundle display name, menu and current build instructions use Portkey. Keep the existing `local.usbfollowdisplay.app` bundle identifier and Application Support/Logs paths to preserve local settings and enrollment. The checkout remains at its saved Codex project path `/Users/jabriel/dev/usb-follow-display`.

## Working context

Repository: /Users/jabriel/dev/usb-follow-display
Initial branch: codex/initial-app
No user code existed in this directory before initialization. It is already separate from chezmoi; do not create a nested worktree just to satisfy isolation ceremony.

Continue implementation in this repository. The user has approved the design direction and does not want another general approval round. The menu-bar addition and architecture choices below are delegated implementation decisions. Resolve ordinary choices and record them rather than stopping.

## Chosen architecture

Swift native macOS app, SwiftPM-managed testable core, IOKit USB lifecycle observation, NSWorkspace sleep/wake notifications, and a small AppKit menu-bar UI (SwiftUI can be used for a compact settings view if helpful). Use the vendored, pinned m1ddc backend compiled into the .app bundle. No Windows client, server, network listener or external UI framework is needed for this first release.

Use a single serialized scheduler with bounded helper lifetime and latest-state cancellation. Separate pure ownership policy from USB enumeration, DDC process transport and UI. App UI should provide status, automatic switching on/off, manual Mac/Windows input selection, an understandable error/status indication, and a way to inspect/re-enroll the monitor/hub pair after cabling changes. Ship a .app bundle built by a reproducible script; preserve the original helper license. App should run from this repo's ignored dist directory for initial acceptance. An Applications install or login-start option can be provided after the foreground app works; do not require administrator privileges or globally install the helper.

Manual input selection must not be instantly undone by a stale automatic action. Decide and document a simple policy, such as canceling pending actions and waiting for the next genuine USB ownership transition.

## Measured hardware and commands

Mac M5 Pro, direct built-in HDMI to Philips 27M2N5800; Windows connected via DP. External display currently works at scaled 2560x1440 / 144Hz. The 5120x2880 system rendering framebuffer does not make the physical panel 5K.

Philips system UUID: 8EBDEBE8-EAF7-4084-8064-0B59A5E48919
EDID UUID: 410C98C3-0000-0000-1824-0103803C2278
Input values: HDMI1=17; DisplayPort=15.
Qualified helper revision: 04d949794102eb8df01ad3681afff6464a3eede2

A real test read current DP=15 with Windows awake, sent HDMI1=17 FROM THE MAC WHILE DP WAS SELECTED, then read 17 twice. User confirmed the external screen returned to Mac. Thus inactive-HDMI reclamation works in this cabling. No other DDC app needs installation.

Source is in ThirdParty/m1ddc. Example command after building:
`m1ddc --verbose display 8EBDEBE8-EAF7-4084-8064-0B59A5E48919 set input 17`

The upstream parser does not validate a full VCP response and returned 0 during transitions. Do not treat exit 0 or input 0 as verified switching. Add proper reply validation if touching that code, with a focused regression test; otherwise treat non-enrolled values as inconclusive. Keep selection/write success separate from visually verified hardware acceptance.

## USB ownership signal (physically tested)

Switch: UGREEN USB 3.0, four peripheral ports / two host terminals. Exact product SKU unknown and unnecessary for current topology enrollment.

- Shared USB3 branch: VID/PID 05e3:0626, location 0x02210000, parent 05e3:0626 at 0x02200000.
- Shared USB2 branch: VID/PID 05e3:0610, location 0x02121000, parent 05e3:0610 at 0x02120000.
- HHKB 04fe:0021, Logitech receiver 046d:c52b and Keychron receiver 3434:d026 are children of the shared USB2 branch.

One switch round trip removed/restored both shared branches and all three peripherals. Unplugging HHKB alone removed only HHKB: both shared hubs and receivers stayed present. Upstream hub with the same VID/PID stays connected, so model-only matching is wrong. Runtime registry IDs changed after reconnect and cannot be saved as stable identity. USB3/USB2 branches came back about 600ms apart in that sample.

Both enrolled branches present and settled => Mac owns USB => HDMI1.
Both absent and settled after established ownership => proposed Windows handoff => DP.
Partial pair or missing keyboard/receiver alone => no new input selection.
Startup absent => unknown, not Windows. Sleep cancels pending work and clears prior ownership; wake re-enumerates. Wake with pair present can reclaim HDMI1; wake absent remains unknown without a departure write. Moving the dock/Mac port requires re-enrollment, not guessing another matching hub.

Unplugging/powering off the whole USB switch can mimic switching to Windows. This is a physical observability limit, not something keyboard activity fixes. Document it. Initial supported workflow keeps both hosts awake. Windows not being awake caused inconclusive black-screen/return-to-HDMI behavior before the successful DDC test; qualify wake readiness separately.

## Execution and verification

1. Update the existing plan for a menu-bar product and the now-selected paired-hub trigger; the historical proposal has superseding update blocks. Commit that coherent plan/spec before product implementation.
2. Write meaningful failing tests for the pure policy and scheduling behavior, implement and run them green. Cover paired hub matching, duplicate interfaces, partial arrival, keyboard unplug, startup absence, rapid reversals, old in-flight writes, helper deadline/failure, invalid readback, pause/manual override, and sleep/wake.
3. Integrate native USB events, vendored helper packaging, menu UI, configuration persistence and single-instance behavior. Build and launch a real .app, inspect UI with native automation if possible, and make startup/backend failures understandable.
4. Run bounded real-hardware acceptance. Request user cooperation for physical USB switching. Record actual observations separately from simulated tests. Coordinate real Mac sleep with the user (they must be able to wake it); do not claim actual sleep recovery from synthetic notifications alone. If user input is pending, keep doing independent work.
5. Fresh review, fixes and final verification; commit in logical units. Leave the useful app and local repository ready to continue. Do not push/publish without a request.

No need to create another chat for subtasks. This dedicated project chat is the user-authorized local session. Use agents only when applicable skill instructions require them. Never mark hardware acceptance done until real evidence exists.
