# Portkey implementation record

This record preserves the implemented plan and the limits of its acceptance evidence. The [implementation brief](IMPLEMENTATION-BRIEF.md) records current scope; the [design](design.md) describes the product. Completed items below refer to the original native-app delivery, with later changes recorded separately.

**Goal:** Ship a usable native Mac menu-bar app that follows the enrolled USB hub pair and switches the qualified Philips between HDMI1 and DP.

**Architecture:** SwiftPM pure policy/scheduler core, IOKit snapshot/event adapter, one serialized helper executor, AppKit menu/settings UI, and reproducibly bundled vendored m1ddc. One Mac app; no Windows/network component.

**Tech stack:** Swift 6 / Foundation / IOKit / AppKit; a dependency-free SwiftPM executable assertion runner on this Command Line Tools installation; clang for the pinned Objective-C helper.

**Design:** [design.md](design.md)

## Global Constraints

- One Mac process, explicit Philips selector, HDMI1=17 and DP=15; both hosts awake for first supported workflow.
- Pair USB2/USB3 topology enrollment, not keyboard or VID/PID alone. Startup absence and failed enumeration must never infer Windows ownership.
- Both present settles 300ms; both absent after ownership settles 700ms; incomplete pair issues no new action.
- Maximum three attempts, 750ms/1500ms retry delays after failure, bounded/reaped helper process, no concurrent DDC writes.
- Manual selection supersedes pending automatic work until the next genuine ownership transition; sleep cancels and clears ownership, wake re-enumerates.
- Retain pinned helper license/provenance; no system-wide helper install, privileged process or network listener.

## Review Focus

- Duplicate parent hubs and a changed Mac/dock port must not match an unrelated shared hub.
- Rapid state changes and a late in-flight write must converge on the newest target.
- Sleep/wake and an inventory read error must not fabricate departure.
- A helper timeout, full output pipe or invalid readback must not hang the menu UI or imply verified selection.
- Manual override, automation toggles and monitor reconnection must cancel stale work consistently.

### Task 1: Tested core and configuration

**Files:** Package.swift; Sources/FollowCore/{Configuration,USBTopology,OwnershipPolicy,CommandScheduler}.swift; Tests/FollowCoreTests/*.swift.
**Interfaces:** USB inventory snapshots produce owned/absent/partial/unknown observations. Pure policy consumes observations, lifecycle/automation/manual events and monotonic time to produce explicit input requests with generation. Scheduler serializes requests through an injected backend and clock. The implementation separates observations, policy requests, and serialized backend work.

- [x] Write focused failing tests for every core/config/scheduler failure mode in the spec, including initial absence, matching upstream duplicates, delayed companion hub, keyboard-only removal, failed inventory, manual override, in-flight reversal and wake absence.
- [x] Run the tests and confirm meaningful red results before implementation.
- [x] Implement the smallest core with explicit ownership history and latest-generation work cancellation; add only the abstractions required for deterministic clocks/backend testing.
- [x] Run `scripts/test.sh` and inspect the tested core results. Standard `swift test` is unavailable in the installed Command Line Tools environment.

### Task 2: Native I/O, packaged helper and real menu-bar app

**Files:** Sources/Portkey/{AppDelegate,USBMonitor,SettingsWindow,AppState}.swift; Sources/FollowCore/{DisplayBackend,ProcessRunner,ConfigurationStore}.swift; scripts/build-app.sh; Resources/Info.plist; Tests/* for process/config adapters; README.md; ThirdParty/m1ddc changes only if needed with provenance.
**Interfaces:** IOKit adapter emits complete inventory snapshots or explicit read errors to the core. Backend targets the configured monitor via bundled helper and reports sent/confirmed/inconclusive/failed. App state observes lifecycle and drives the same scheduler for UI and automatic actions. Preferences edits validate before saving and atomically persist.

- [x] Write failing tests for backend arguments, helper failure/deadline/output handling, known input/readback parsing, persistence validation and the selected lifecycle adapter seams.
- [x] Implement native device notifications and re-enumeration, draining notification iterators correctly; use a topology snapshot rather than treating arbitrary termination as a hub departure.
- [x] Implement bounded helper execution with termination/reaping and pipe draining. Retain the vendored reply parser and reject non-enrolled readback values at the app boundary; full VCP packet integrity remains unverified.
- [x] Implement menu/status/settings, single-instance behavior, startup error handling, manual selection and automatic pause using the tested core.
- [x] Build the .app with its helper and license; run tests and release build, validate bundle/plist, launch the actual app and inspect its real menu/settings with native UI tools where available. Commit.

### Task 3: Hardware acceptance, sleep recovery and final delivery

**Files:** diagnostics/app-acceptance.md; README.md; fixes in corresponding source/tests.
**Interfaces:** The running packaged app uses real USB and monitor hardware. The test record separates deterministic tests, dry-run/integration observations and user-confirmed physical results.

- [x] Coordinate physical USB switching with the user and record the observed subset. Record rapid-reversal evidence separately from visually confirmed round trips; do not infer extra confirmations from log events.
- [x] Confirm HHKB-only unplug does not change inputs with the packaged app running.
- [x] Test lifecycle simulation in code and coordinate actual Mac sleep/wake with USB owned locally. Record the observed eventual recovery and leave wake with USB switched away pending.
- [x] Test foreground quit/restart, missing backend/config error and recovery. Document awake-host and complete-switch-unplug limits.
- [x] Obtain a fresh whole-branch review, fix important findings with regression tests, and run `scripts/test.sh`, the release bundle build, and relevant hardware retests.
- [x] Commit the deliverable, leave the .app available locally, and report what changed, where to launch it, evidence and remaining hardware limits. Do not push or publish.

## Native delivery evidence

Tasks 1/2 implemented and committed; independent review findings fixed with regressions. Installed Command Line Tools lacks XCTest/Testing: `scripts/test.sh` runs the dependency-free SwiftPM executable test runner (75 assertions). `scripts/verify-bundle.sh` validates actual deployment targets, license and signature. The user declined 20 repeated physical cycles and confirmed several ad hoc switches plus HHKB-only unplug/replug; the observed subset is recorded in [app acceptance](../diagnostics/app-acceptance.md). Four coordinated real sleep trials recorded; eventual automatic external recovery confirmed with Windows awake while Mac remains locked. Recovery has an observed delay, not an immediate-response claim. Optional physical wake-with-USB-away and monitor power-cycle cases remain unqualified.

## Product name, repository, and language support

The product, Swift package/target, executable, and app bundle are named Portkey. The checkout is `/Users/jabriel/dev/portkey`, and the delivery branch is `main`. The user handles pushing to GitHub. Keep `local.usbfollowdisplay.app`, `~/Library/Application Support/USBFollowDisplay`, and `~/Library/Logs/USBFollowDisplay` for compatibility.

The current language requirement is English by default with a persistent English / Chinese (Simplified) submenu. Selecting a language immediately updates the menu, settings, and current status without restarting. App-owned controls and messages use the selected language, while hardware/backend errors may retain original detail. Language verification belongs to the current change's tests and actual app inspection; the historical 75-check result above predates this addition.
