# Portkey design

## Scope and evidence

This document records the native menu-bar product design. The [implementation brief](IMPLEMENTATION-BRIEF.md) records current scope, and the [implementation record](implementation-plan.md) tracks delivery. Machine-specific diagnostic records are excluded from Git; qualified behavior and pending cases are summarized below.

## Outcome

A native, unobtrusive menu-bar app keeps a Philips 27M2N5800 input aligned with a physical UGREEN four-port/two-host USB switch. Both computers awake is the initial supported workflow. One Mac process controls the screen; no Windows install or network service is required. The Mac must be awake. The existing project root is ~/dev/portkey, branch main, isolated from dotfiles.

## Architecture

- Swift, SwiftPM and macOS native APIs. Pure state/scheduling core with deterministic tests. AppKit NSStatusItem menu and a compact AppKit settings window.
- IOKit device lifecycle notifications plus complete inventory snapshots for reconciliation, NSWorkspace sleep/wake/display notifications, one serialized asynchronous DDC command worker.
- Vendored m1ddc revision 04d949794102eb8df01ad3681afff6464a3eede2 compiled into the app bundle, license retained. Private DDC APIs remain inside the helper. Use explicit monitor selector; no global helper installation.
- A reproducible script builds the app bundle in ignored dist/. Support normal Quit, single-instance behavior and a clear startup/backend error. The first release has no login item.
- JSON configuration and bounded diagnostic logs under the app's per-user Application Support/Logs directories. No keystroke capture, mouse-event observation, network listener or privileged component.

## Enrollment and input mapping

Use the explicitly enrolled Philips UUID; HDMI1=17, DisplayPort=15. Preserve a way to select/re-enroll if monitor identification changes; never target a different display silently.

Enroll both shared USB2/USB3 branches and their parent topology from the local inventory. The parent hubs stay present when switching. Duplicate VID/PIDs exist upstream, so include topology in identity. Runtime registry IDs are not persistent. A port/dock change must show enrollment needed rather than guess.

Both branches present => locally owned; settle for 300ms then HDMI1. Both absent after prior ownership => departure; settle for 700ms then DP. One branch missing => partial/unknown: issue no new action and cancel queued stale work. Preserve prior ownership history across a short partial transition so both-absent can be recognized; sleep/startup clears ownership. Peripheral-only changes never alter the hub state. USB3 and USB2 returned 600ms apart during the observed trial.

Startup or wake with both absent => unknown, no DP write. Wake with both present => reconcile and reclaim HDMI1 after settling. Sleep cancels retries/helper work, disarms departure, and releases stale monitor handles; wake re-enumerates. A monitor reconnection can reconcile known current local ownership but must not replay an old departure.

## Commands, manual control and failures

Use explicit desired inputs, not toggles. All automatic/manual writes share a serialized worker. Supersede queued work with a newer generation and check ownership before every retry. An in-flight old write cannot be revoked, so ensure the latest desired target gets the final corrective write. Bound helper lifetime, terminate and reap a hung helper, and drain pipes without deadlocks. Never run concurrent I2C operations.

At most three attempts for a request. Start immediately; retry after failure with 750ms and 1500ms delays. Distinguish transport-sent from readback-confirmed. The backend waits 4 seconds after a write before readback, based on measured transition latency; cancellation interrupts that wait. Each helper process has a 2.5-second deadline. Readback must be a known enrolled input; 0 or malformed reply is inconclusive. Readback failures must not cause an unbounded loop or steal focus. Selecting DP while Windows lacks a live video signal may revert to Mac; show a useful status and keep initial awake-host scope.

Manual input selection cancels stale automatic work and holds the manual choice until the next genuine hub-ownership transition. Toggling automation off cancels pending work; re-enabling reconciles current inventory and follows startup rules. Missing/ambiguous monitor, invalid config or missing helper disables writes and displays an actionable status. An empty inventory due to a read failure is not actual absence.

## Menu and settings

Menu contains a succinct current status, USB ownership status, automatic switching toggle, Switch to Mac (HDMI 1), Switch to Windows (DisplayPort), Settings, Show Logs, Quit. Status distinguishes unknown/partial/disconnected, paused, switching, sent/unverified, verified and error without flooding notifications. Provide a compact settings view to inspect detected hubs/monitor, save or re-enroll the pair, see the selected backend/input mapping, and revert an invalid edit. The app does not require a permanent Dock icon.

Default to English and provide an English / Chinese (Simplified) language submenu. Persist the selection in local preferences and immediately update the menu, settings window, and current status without restart. Controls and app-owned messages use the selected language; backend/system detail can remain in its original language. Keep the bundle identifier `local.usbfollowdisplay.app` and the existing `USBFollowDisplay` configuration/log directory names so installed preferences and enrollment remain compatible.

The [README](../README.md) documents build/run, settings, language selection, recovery, and uninstall. UI verification uses the actual packaged app.

## Tests and acceptance

Deterministic tests cover: paired topology matching, upstream duplicates, incomplete pair, device-interface duplication, keyboard/receiver unplug, failed enumeration, startup absence, delayed companion hub, rapid reversals, stale/in-flight writes, helper deadline/error, invalid readback, manual override, automation pause/re-enable, sleep/wake, and monitor reattachment. Use fake clocks/processes for these tests.

Hardware acceptance records only observed trials. The user confirmed one full physical round trip and several additional ad hoc switches, plus an HHKB-only unplug/reconnect. The user declined 20 repetitions, so there is no 20-cycle reliability claim. Four coordinated real sleep trials were recorded; the final Windows-awake trial confirmed eventual automatic recovery while the Mac was still locked, without Accessibility permission. Readback confirmation arrived about 23 seconds after wake in that run. Diagnostic GET traffic was also present, so this timing is not an isolated benchmark or a latency guarantee.

Startup or real wake with USB already switched away and monitor power-cycle behavior remain unqualified physical cases. Synthetic lifecycle checks are deterministic/integration evidence, not proof of actual sleep recovery. Any further physical switch or sleep tests require coordination with the user.

The release bundle, helper, deployment targets, plist, license, and ad-hoc signatures have recorded verification. The acceptance record distinguishes tests, transport/readback results, and user-visible picture confirmation. Delivery remains local in `~/dev/portkey` on `main`; the user pushes to GitHub themselves.

## Known limits

USB lifecycle cannot distinguish a host switch from the entire switch being unplugged or powered off. The app does not control the physical USB switch, wake arbitrary sleeping hosts, or run while the Mac sleeps/off. The single initial DDC round trip did not prove wake readiness or long-run reliability. Later sleep evidence qualifies only the conditions recorded above; pending cases remain explicit in the acceptance record. The vendored reply parser is unchanged: rejecting non-enrolled values does not validate a complete VCP packet.
