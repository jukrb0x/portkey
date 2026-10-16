# Approved implementation brief — 2026-10-16

PortKey is a native Mac-only menu-bar app in a standalone repository. The initial implementation includes paired-hub ownership detection, explicit monitor input selection, enrollment/settings, and bounded helper execution. Architecture choices were delegated; hardware claims must remain tied to recorded observations.

Current design: [design.md](design.md)

Implementation record: [implementation-plan.md](implementation-plan.md)

Machine-specific acceptance records remain local and are excluded from Git.

## Product name

The product name is **PortKey**. Swift package, target, executable, bundle name, menu, and documentation use this spelling. The bundle identifier is `local.portkey.app`; configuration and logs use `PortKey` directories. Migrate existing enrollment, logs, automation, and language preferences on first launch; never overwrite newer PortKey data or silently ignore migration failures. The checkout remains `~/dev/portkey`. Do not edit private Codex state.

## Working context

Repository: ~/dev/portkey
Delivery branch: main
The repository is separate from the user's chezmoi dotfiles.

Work in this repository and preserve compatibility with the existing local app data. The user handles pushing to GitHub; no remote publication is part of local delivery.

## Chosen architecture

Swift native macOS app, SwiftPM-managed testable core, IOKit USB lifecycle observation, NSWorkspace sleep/wake notifications, and a small AppKit menu-bar UI and settings window. Use the vendored, pinned m1ddc backend compiled into the .app bundle. No Windows client, server, network listener or external UI framework is needed for this first release.

Use a single serialized scheduler with bounded helper lifetime and latest-state cancellation. Separate pure ownership policy from USB enumeration, DDC process transport and UI. App UI should provide status, automatic switching on/off, manual Mac/Windows input selection, an understandable error/status indication, and a way to inspect/re-enroll the monitor/hub pair after cabling changes. Ship a .app bundle built by a reproducible script; preserve the original helper license. App should run from this repo's ignored dist directory for initial acceptance. The first release uses manual app launch and does not install a login item. Do not require administrator privileges or globally install the helper.

Manual input selection must not be instantly undone by a stale automatic action. Cancel pending actions and hold manual selection until the next genuine USB ownership transition.

## Measured hardware and commands

Mac M5 Pro, direct built-in HDMI to Philips 27M2N5800; Windows connected via DP. External display currently works at scaled 2560x1440 / 144Hz. The 5120x2880 system rendering framebuffer does not make the physical panel 5K.

Input values: HDMI1=17; DisplayPort=15. Qualified helper revision: 04d949794102eb8df01ad3681afff6464a3eede2.

A real trial with Windows awake confirmed that the Mac can reclaim HDMI1 while DP is selected. This qualifies the tested cabling only. Use the explicitly enrolled monitor UUID for every write; local identifiers and raw probe output are kept outside Git.

The upstream parser does not validate a full VCP response and returned 0 during transitions. Do not treat exit 0 or input 0 as verified switching. Add proper reply validation if touching that code, with a focused regression test; otherwise treat non-enrolled values as inconclusive. Keep selection/write success separate from visually verified hardware acceptance.

## USB ownership signal (physically tested)

Switch: UGREEN USB 3.0, four peripheral ports / two host terminals. Exact product SKU unknown and unnecessary for current topology enrollment.

The shared USB2/USB3 pair and its parent topology were enrolled from the local inventory. A keyboard and two receivers were attached beneath the shared USB2 branch. Exact local identifiers belong in the saved configuration, not public diagnostic records.

One switch round trip removed/restored both shared branches and all three peripherals. Unplugging HHKB alone removed only HHKB: both shared hubs and receivers stayed present. Upstream hub with the same VID/PID stays connected, so model-only matching is wrong. Runtime registry IDs changed after reconnect and cannot be saved as stable identity. USB3/USB2 branches came back about 600ms apart in that sample.

Both enrolled branches present and settled => Mac owns USB => HDMI1.
Both absent and settled after established ownership => proposed Windows handoff => DP.
Partial pair or missing keyboard/receiver alone => no new input selection.
Startup absent => unknown, not Windows. Sleep cancels pending work and clears prior ownership; wake re-enumerates. Wake with pair present can reclaim HDMI1; wake absent remains unknown without a departure write. Moving the dock/Mac port requires re-enrollment, not guessing another matching hub.

Unplugging/powering off the whole USB switch can mimic switching to Windows. This is a physical observability limit, not something keyboard activity fixes. Document it. Initial supported workflow keeps both hosts awake. Windows not being awake caused inconclusive black-screen/return-to-HDMI behavior before the successful DDC test; qualify wake readiness separately.

## Language and documentation

Project documentation is English. The app defaults to English and offers an English / Chinese (Simplified) submenu, with the selection saved in local preferences. Language changes update menu, settings, and current status immediately without restart. Controls and app-owned messages use the selected language; hardware/backend errors may include original system detail. Use the PortKey bundle identifier and data directories, with one-time legacy migration.

## Verification and remaining scope

The original native delivery passed the dependency-free assertion runner (75 checks), release build, actual deployment-target checks, plist/license verification, and ad-hoc signature checks. Those results predate the language-support change; current verification must be recorded separately.

The user confirmed one full physical round trip and several additional ad hoc switches, plus HHKB-only unplug/reconnect, and explicitly declined 20 repeated cycles. Four real sleep trials were recorded. The final Windows-awake trial confirmed eventual recovery while the Mac remained locked, without Accessibility permission. Readback arrived about 23 seconds after wake in that observed run; this is not an immediate-recovery guarantee.

Startup or real wake while USB remains switched to Windows and monitor power-cycle behavior remain unqualified physical cases. Keep simulated policy checks, helper transport success, DDC readback, and visually confirmed switching distinct. Further physical switching and real Mac sleep tests require coordination with the user. Do not repeat completed hardware trials merely to satisfy an obsolete test count.
