# Implementation brief — 2026-10-16

## Follow-up Mac departure failure — 2026-10-20

The user reports a new failed single-Mac departure after the v1.0.3 fix. A live private trace at approximately 20:28:15 local time shows an inventory error during USB removal, then repeated fresh complete-absence observations without any Mac remote-input dispatch; USB return at 20:28:32 led to a local command. The trace has no iterator-invalidation retry event, so the earlier recovery for one exact error string did not cover this failure. Treat any Mac inventory error as a bounded loss of observation only when local ownership was previously established and the same qualified profile remains active; require a fresh successful absence, settled policy request and exact display prewrite before switching. Keep Windows, startup, sleep and missing/ambiguous-display behavior fail closed. Do not claim visual acceptance from deterministic replay or command transport. Do not request repeated manual trials.

## Single-Mac departure reliability — 2026-10-20

The user reports repeated USB departures from Mac toward Windows failing while returns to Mac succeed. At this stage, the latest incidents had no new trace. Improve the documented Mac IOKit enumeration-invalidation path with bounded recovery from previously established ownership, requiring a fresh complete absent inventory and the existing settled, exact-display write gates. Other discovery errors were initially left fail closed; the follow-up above supersedes that error classification for Mac. Startup, profile, and sleep cases remain fail closed. Build and run locally without asking the user for repeated switching trials; distinguish simulation and transport from visual acceptance. See `docs/RUST-VERIFICATION.md` for implementation evidence.

## Current-format configuration only — authorized 2026-10-18

The user requests removal of historical Swift import and old configuration migration rather than further compatibility work. Remove the import UI/parser/action, old single-file Rust fallback, sibling-app preference lookup and grandfathered automation eligibility. Current-format qualified profiles remain supported. The user approved backing up and clearing existing local profiles and drafts, then configuring anew through the current guide. Complete configuration backups were verified before reset on both platforms; automation starts off. An old/unqualified record is not automatically approved. This supersedes the migration-preservation requirements in earlier delivery sections below. Keep workflow design documents locally under ignored directories; never commit them.

## Native product delivery — authorized 2026-10-18

The user now authorizes implementation and local deployment of a complete native product, prioritizing design quality over speed. Retain the repaired Rust runtime; replace presentation with SwiftUI plus AppKit integration on macOS and C++/WinRT + XAML WinUI 3 on Windows. Support two named computers, single/dual installation semantics, per-display workspaces and resumable setup with honest qualification provenance. Existing Rust automation preferences survive metadata migration; new/legacy Swift imports remain off. Controls and peer status remain explicitly local/unknown without a network protocol. No new physical switching or sleep tests. Retire historical Swift business logic only after replacement and migration safety. This supersedes historical frontend choices below.

## Native Windows DDC subprocess and lower-level USB investigation — 2026-10-18

Native subprocess deployed and its desktop timing session verified; configuration unchanged. Read-only comparison measured118ms native vs389ms PowerShell with identical unavailable readback outcomes. This is transport overhead evidence only. Static parent-port diagnostics distinguish connected/disconnected controls, but earlier transition detection is unproven and is not used for ownership. See latest verification record.

Latest ordinary-use events confirm fast native USB polling and specific interface notifications are running, yet Windows still reports owned then partial for seconds after Mac arrival. Investigate parent-hub physical port status read-only before changing ownership policy; never treat missing or failed diagnostic mapping as absence. Replace only Windows DDC read/write PowerShell transport with a native child process to remove measured process/compilation overhead. Preserve exact enrolled interface resolution, unique physical-monitor requirement, parent timeout/output bounds, post-write wait/readback semantics and hardware-result uncertainty. Cosmetic UI discovery may remain PowerShell. No additional physical switching trial is required for implementation; deploy only after native subprocess read-only smoke and regression checks.

## Automated display comparison result — 2026-10-18

Two bounded automated handoff arms ran with both apps stopped and verifiedWindows-input7 baselines. Windows-only set5 produced1.455s WindowServer hotplug-out→in; Mac-first thenWindows set5 produced1.415s. Both ended with Mac readback5 and same display mode. B's pre-Windows Mac read was null, so it does not isolate an already-selected-input duplicate; extra diagnostic read and one sample per arm limit inference. The logged reconfiguration occurs without cross-host duplicate writes in A. Both latest apps restored with configuration hashes unchanged. Do not claim all duplicate-command effects excluded, or change display modes/firmware settings speculatively.

## Remaining departure and display-link latency — approved 2026-10-18

Deployed Windows USBEvents: both interface subscriptions verified; live fallback cadence approximately262ms including scan/scheduling. Mac event-driven build remains running unchanged. Configurations preserved; no fresh physical switching acceptance claimed.

The user requests continued diagnosis and direct deployment of evidenced fixes without repeated manual trials. Latest trace contains a 2.1s gap between a Windows owned USB snapshot and a partial snapshot before notification; native scan cost is only milliseconds. Shorten only the independent native Windows USB fallback to250ms with exact remaining-deadline waits; add specific USB interface arrival/removal hints while retaining full fresh topology/status validation. Do not infer absence from a notification. Display-link logs show a real hotplug-out/in interval rather than a PortKey sleep:1.495s between WindowServer hotplug states and roughly0.9s more until AppKit reports active. This does not identify firmware vs driver cause or establish visible-picture timing. Four bounded later same-input Mac writes produced no further logged reconfiguration, so unconditional extra DDC reads are not justified as a proven latency repair.

## Both-host event-driven deployment — approved 2026-10-18

Deployment completed: Mac native notifications report ready; Windows native pre-command validation is running, with two-display identity parity confirmed in a read-only desktop probe. Both saved configurations are unchanged and both new timing sessions verified. Previous comparison/staging holds below are historical. No new physical switching acceptance is claimed.


The user now explicitly requests Mac USB event observation and deployment of both latest builds, without more manual switching. This supersedes the Windows-only comparison hold. Register native IOKit arrival/removal hints on the main run loop, drain/release iterators, wake existing fresh discovery, retain two-second polling fallback and exact topology validation. Hints do not establish ownership. Deploy native Windows pre-command display validation plus 300ms established-departure settling; preserve configuration and rotating diagnostics. Verify actual running binaries and sessions, not just staged files. Both hosts remain independent bidirectional controllers; no network coordination or cross-host deduplication is introduced. Physical latency improvements require subsequent evidence and must not be inferred from passing tests.

## Windows-only comparison result and restoration — 2026-10-18

User confirms Windows-only could not switch picture from Mac to Windows. Windows logged fresh owned USB at 02:03:01.045, local write at 01.974 and unverified completion at 03.957. Therefore this was a dispatched but visually failed command, consistent with inactive-input DDC limitations; it does not prove a universal firmware restriction or settle duplicate-command latency. Mac optimized AppKit build was then restored with timing enabled and unchanged profile hash. Windows still runs TraceRotation; native prewrite optimization remains staged there.

## Current Windows-only comparison — 2026-10-18

The user reiterated that a Windows-only comparison was requested and should establish whether inactive-input DDC requires both hosts. Mac Rust was gracefully quit; legacy Swift is also stopped. Windows TraceRotation remains unchanged. Do not restart Mac or replace Windows during this comparison without preserving this experimental condition. Pre-command optimizations are built/staged (300ms departure debounce and native Windows fresh display validation), but not deployed; no measured improvement is claimed. Both-host operation currently means independent bidirectional controllers with no network coordination or cross-host deduplication; single Mac operation remains supported on qualified hardware.

## Current latency work and hardware-test boundary — 2026-10-18

The user has explicitly requested no more repeated manual switching trials. Continue with existing traces, read-only diagnostics and deterministic regression tests; do not request another foreground round trip unless the user independently reopens that work. They want causes established before an architecture/backend rewrite. Both automatic directions have been visually confirmed, but Windows-to-Mac latency remains unacceptable and the device-event trial was approximately 17 seconds. After the native USB update the user reported roughly 7–8 seconds; the native process was verified, but the prior timing file was already capped and did not capture this trial.

Existing traces identify redundant Windows Service queries (removed) and an internal scheduler defect: ownership-change command cancellation also invalidated newer in-flight discovery. Separate inventory freshness revisions from command generations; preserve full invalidation for setup/lifecycle/config changes. Do not claim a hardware speed improvement from tests alone. See docs/plans/switch-latency.md and docs/RUST-VERIFICATION.md. No server/client architecture change is approved by inference from the latency problem.

The follow-up audit also confirmed completed inventory was withheld behind DDC write/readback. Publish fresh scans before transport, preserving revision/lifecycle gates and command validation. A real worker barrier test reproduces the old delay without hardware. Full inventory scans and mandatory pre-write discovery remain measured costs; existing traces do not record physical button or picture timestamps. An offline analyzer in `scripts/analyze-timing.py` makes stage accounting reproducible.

The subsequent TraceRotation round trip is recorded: user estimates seven seconds outbound and two to three seconds returning. Notification-triggered USB scans took 2.3–4.2ms; native write return was bounded at 3.306–3.559s / 2.431–2.671s after the first Windows notification, with no queue bottleneck or retries. This excludes the post-write wait and cannot locate unmeasured button-to-notification or command-to-picture time. See the latest verification entry; do not keep attributing this trial to the two-second fallback.

Mac unified logs subsequently recovered PortKey-attributed m1ddc activity and an external-display inactive→active reconfiguration interval. Conditional current-clock alignment puts Windows command return about 2.16–2.92 seconds before Mac reports the display active. Mac helper arguments/results were not logged; duplicate successful commands and visual frame timing remain unproven. The later Mac helper activity follows activation and cannot explain the earlier inactive interval. No restart or new switching trial was performed.

Latest detailed capture confirms local reassertions after transient display-profile disappearance, not evidence of opposite-target contention. A final user-observed failed handoff has Mac inventory error→unknown/absent with no departure write and an unverified Windows local write. A bounded fresh-snapshot retry for explicit IOKit iterator invalidation is staged and tested; it does not preserve stale ownership or weaken unknown-state handling. Historical Swift fallback was found running and gracefully quit; its logs contain no writes in the captured interval.

## Detailed Mac command logging — approved 2026-10-18

The user authorizes detailed Mac command/result logging and one restart to enable it, without repeated manual switching. Record each actual helper invocation, set/get operation, numeric input, local/remote target, UTC and monotonic timing, exit status and categorized failure. Preserve exact enrollment validation and transport-only success semantics; do not add readbacks or hardware writes for diagnostics. Keep device IDs and raw output outside the log. Maintain both hosts for the initial observation; a Windows-only comparison is a separate optional condition, not evidence already collected.

## Rotating latency logs — approved 2026-10-18

The user requests automatic log rotation so reaching the cap no longer stops recording, plus continued latency investigation without repeated manual switching. Keep an active 1MiB file and five backups, preserve record boundaries, recover from a full existing file, and make loss/retention limits visible in offline analysis. Do not erase existing evidence or claim the uncaptured 7–8 second trial has been reconstructed.

## Native Windows USB observation — approved 2026-10-18

The user approved replacing repeated PowerShell USB discovery with Windows native device enumeration, separating USB observation from display refresh, and verifying with automated tests without further manual switching. Keep the existing two-host policy and native UI. Device notifications wake an independent USB observer; fresh exact topology snapshots establish ownership. Full display/USB validation remains mandatory before writes, retries and input probes. No running app restart or physical switch trial is required for this implementation. See [native USB plan](plans/windows-native-usb.md).

## Two-host automatic switching (current approved scope)

Run the native Rust app on both Mac and Windows with independently enrolled display/hub identities and mirrored local/remote inputs. The current home mapping is Mac local 5 / remote 7 and Windows local 7 / remote 5; these are user-specific values, not product defaults. Both hosts stay awake. Settled USB arrival selects local; settled departure after established ownership selects remote. The two requests therefore agree on the destination input. This supersedes the single-controller restriction in the initial Rust plan. No network service is needed. See [dual-host plan](plans/dual-host.md) for evidence limits, races and validation.

## Home/work profiles (current approved scope)

The user wants automatic recognition of separately enrolled home/work displays. Add named per-display profiles over the shared Rust controller, preserving each environment's USB topology and input mapping. Unknown or ambiguous devices must not inherit another display's setup. Continue AppKit UI acceptance on the unlocked Mac; the currently attached display has changed and must be discovered again. See [profile plan](plans/location-profiles.md).

## Native UI architecture

The user explicitly requested keeping the Rust core and replacing egui with native UI on the same branch, without stepwise confirmations. macOS uses a thin Swift/AppKit menu-bar and settings frontend linked to a Rust static library. Windows uses native Win32 controls and system tray from Rust. Configuration, policy, hardware operations and lifecycle protection remain shared Rust. See [native UI plan](plans/native-ui.md); this supersedes the prior egui choice. Preserve the Rust configuration schema and explicit enrollment. No browser, egui, eframe or tray-icon UI runtime remains.

## Rust product rewrite

The user requested `codex/rust-cross-platform`: rewrite in Rust, generalize enrollment for other users, and add Windows support. This supersedes the Swift-only/Mac-only architecture below for this branch. See [Rust product plan](plans/rust-product.md). The historical Swift delivery remains a fallback and its hardware evidence does not qualify the new runtime. New installs must start unenrolled with automation off; never ship personal identifiers as Rust defaults. Explicit display targeting, topology identity, bounded serialization and fail-closed startup/sleep behavior remain mandatory. Windows support requires separate build and physical validation; do not claim universal monitor compatibility. Delivery remains local, with no push or release publication.

## Historical Swift delivery

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
