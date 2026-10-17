# Guided setup and two-host build verification — 2026-10-17

- The user visually confirmed two Mac-active switches to the Windows input and one Windows-active switch back to Mac. Two inactive-source Mac return attempts had no visual effect. This qualifies the measured directional commands, not automatic USB switching. The input mapping is kept in local enrollment; the monitor's advertised capability values were not reliable for this unit.
- Added native guided setup on AppKit/Win32: friendly monitor selection, learning one/two hubs from stable disappearance/return, exact-display current-input read, offline code exchange, and save with automation paused. Advanced retains technical enrollment. Codes carry input values only; users must select the same physical monitor on both computers.
- macOS: 63 Rust library tests pass; host and Windows cross-target strict Clippy pass. DDC synthetic checks: 26 protocol/name assertions and 8 simulated registry scenarios. AppKit release build, deployment/linkage, signature and provenance verification pass.
- Windows: native MSVC release linking and 61 native library tests pass on the user's Windows host. Dependencies were transferred from the locked local Cargo cache because the remote build environment could not resolve crates.io. The resulting executable and README were deployed locally; no public release was published.
- Independent review corrections: failed discovery clears cached setup baseline/freshness; selected display must remain unique; corrected setup errors no longer leave stale warning state; Windows friendly labels hide diagnostic IDs outside Advanced. Focused re-review found these addressed.
- Actual AppKit inspection confirms readable guided controls, friendly display name, optional DDC/CI instructions, pause during editing and cancel preserving saved home/work profiles. The physical USB learning round trip and Windows GUI acceptance are in progress.
- Windows SSH command/file transfer works, but its noninteractive session cannot enumerate physical monitors for DDC. A one-shot interactive scheduled-task trial did not produce output and was removed. Direct execution by the user in the logged-in desktop successfully switched the intended monitor. Product runtime must run in the logged-in desktop session; SSH is not a product dependency.
- One-hub support, race cancellation and stale probe/lifecycle behavior have deterministic coverage; single-hub hardware, rapid-reversal physical convergence, full automatic round trip, actual sleep and public signing/distribution remain separately unqualified.

# Exact display routing and active-input probes — 2026-10-17

- Replaced global traversal-order proxy selection with selected-controller, endpoint and common-parent matching. Multiple matching routes and unknown topology fail closed. Runtime registry handles are never persisted. This fixes a source-level targeting defect; it does not establish that routing caused the reported switching failure.
- 26 synthetic protocol/name checks and 8 simulated registry scenarios pass. The latter exercise the production selector against different parent identities, traversal orders, ambiguity, invalid iteration, service creation failure and bridge selection, with balanced references. All 49 Rust tests pass. The native AppKit release bundle builds and its signature/linkage/provenance verification passes.
- With the user selecting HDMI1 manually, checksummed Get replies report input 5 (maximum 14), and brightness reads succeed. The monitor's validated capabilities advertise inputs 17/18/15/16. This inconsistency remains unresolved; enrollment values have not been changed based on speculation.
- With automation paused and the app quit, bounded explicit-display Set requests for advertised DP1 (15) and HDMI1 (17) return transport success, but subsequent input reads remain 5. The user confirmed the picture did not change, then manually selected DP and confirmed Windows was visible. A subsequent Mac Get failed reply validation, consistent with the earlier inactive-HDMI null responses; it did not reveal the DP input number. A read-only Windows probe is prepared locally but has not been executed. No visible switching success is claimed.
- Raw identity, frames and hardware trial logs remain in ignored local diagnostics. No sleep or Windows runtime acceptance was performed in this investigation.

# Switching investigation and menu status — 2026-10-17

- User reported no visible input switch on the new display. Home automation was enabled and the UI observed both enrolled USB hubs absent; a manual request showed sent/unverified. Actual switching remains unresolved.
- Deterministic saved automatic profile owned→settled→absent test passes. No policy dispatch defect was reproduced. Added explicit waiting text when startup/resume absence has not established local ownership, retaining command feedback.
- Raw exact-display Get probes returned a valid null frame (`6e 80 be`), not a VCP value. The old parser read trailing bytes as an arbitrary number. Corrected Get checksum and added strict response validation; helper now exits nonzero on this response. Extended wait also returned null. No Set transport changes made.
- Added 18 synthetic helper checks and Mac CI execution. Rust tests total 49. Physical effect is not established by these tests.
- Menu bar changed to icon-only USB status, with distinct unknown/pause/error icons; actions name Mac/Windows directly. Reopening should show settings. Icon visibility was user-confirmed for the intermediate text version; final icon-only UI visual acceptance is pending because computer-use could not attach to the hidden app and screen capture failed for Activity Monitor.
- No new physical round trip or sleep acceptance is claimed. User confirmed Mac HDMI is direct, and could not find a DDC/CI option in the monitor OSD. A null reply does not establish whether this is firmware capability, a setting, or transport behavior.

# Home/work profile and AppKit acceptance — 2026-10-17

- 47 Rust tests pass, including migration, named upsert, home/work transition cancellation, absence/ambiguity/duplicate identity, editing/new/cancel and independent profile preferences. Host and Windows cross-target Clippy pass. 273 historical Swift assertions pass.
- Native AppKit release build and bundle verification pass. Actual UI inspection on the unlocked Mac confirms readable Chinese/English layout, device menus, invalid/incomplete enrollment rejection, draft preservation through refresh/language changes, close/reopen with Cmd+, and native Quit.
- Current display and USB2/USB3 hubs were rediscovered read-only. Home enrollment was saved through AppKit with the user's specified HDMI1/DP mapping and automation off. An initial empty USB snapshot was followed by valid discovery; no persistent enumeration failure was reproduced.
- New setup and cancel preserve saved profiles and restore the detected home profile. Restart visibly selects home and shows both home/work names. Work was restored from the historical Swift enrollment using the validated Rust import/store APIs, with automation off. Identifiers and the temporary import fixture stay local outside Git.
- The native file picker opened, but automated Go to Folder navigation repeatedly failed/reset and the computer-use connection failed once. File-picker completion is inconclusive; API import succeeded. No evidence yet isolates this as an application defect rather than automation behavior.
- No DDC writes or actual sleep were performed. Home/work physical travel and switching, work's current cabling, and Windows runtime remain unqualified. Three pure Windows UI helper tests executed on the host; native Win32 execution remains separate.
- Independent review found no remaining actionable profile correctness/safety issues.

The previous record below describes the earlier native replacement and its then-current limitations.

# Native Rust preview verification — 2026-10-16

Branch: `codex/rust-cross-platform`. Local delivery only; no remote push or release publication.

## Executed

- `cargo test --locked`: 41 tests passed. Covers ownership/settling/manual transitions, unknown inventory, sleep/wake reset, topology and root-controller identity, migration/persistence, pause-before-persistence, request dispatch/mailbox cancellation, exact display targeting, native empty enumeration, subprocess deadlines/inherited pipes, instance exclusion, lifecycle state, localized errors, native controller actions/snapshots, C ABI error/string handling, editing cancellation and first-run language persistence.
- `cargo clippy --locked --all-targets -- -D warnings`: passed on Apple Silicon macOS.
- `cargo clippy --locked --target x86_64-pc-windows-gnu --all-targets -- -D warnings`: passed. This compiles/type-checks Windows-target Rust including platform FFI declarations, but neither links a Windows executable nor executes its embedded PowerShell/C# bridge.
- `scripts/test.sh`: 273 historical Swift assertions passed.
- `scripts/build-rust-app.sh`: Rust static library and Swift/AppKit release bundle built. Native verifier passed: AppKit linkage, macOS 13 deployment targets for app/CLI/helper, plist, ad-hoc signatures, pinned helper license and provenance.
- Packaged `--diagnose`: completed successfully, returning zero external displays and zero matching USB devices in the execution environment. This validates empty discovery and fail-closed onboarding, not device enrollment or hardware switching.
- The prior egui preview was visually inspected, but that evidence does not qualify the replacement UI. Current AppKit visual/accessibility acceptance was blocked because the Mac was locked and the automation tool could not unlock it. Windows native UI has only cross-target static checks. Two pure Windows selection/input tests were extracted unchanged into an ignored standalone host harness and passed on macOS; no Win32 calls ran.

## Findings corrected

- Pinned m1ddc display-list operation crashed in this environment: use native CoreGraphics UUID discovery instead; helper writes use the exact raw UUID CLI syntax from the existing Swift backend.
- IOKit successful NULL iterator means no matches per the installed SDK; it is distinct from enumeration failure.
- Manual requests and persistent policy requests must dispatch once, not repeat per UI frame.
- Pause must cancel operational work before attempting persistence.
- Stale inventory/completions cannot override newer generations or resume state.
- Direct-attached hubs require verified controller ancestry, not guessed zero parent IDs.
- Native lifecycle generation and sleep bit use a single atomic snapshot.
- Helper inherited pipes and large outputs are bounded; startup errors are visible without a console.
- Quit cancels policy before returning. Native event loops continue ticking while settings are hidden; this is source-reviewed, not long-running acceptance.
- Removing the old UI dependencies exposed an implicit ColorSync link dependency; the Mac adapter now links the framework explicitly.
- First-run language preference restoration is covered across controller restarts.
- Independent source review found Windows refresh discarded offline draft choices; retained selections now survive disconnect/reconnect, with a deterministic merge regression.

## Still unqualified

No Rust physical display writes or actual sleep tests were performed. Prior Swift hardware acceptance does not qualify this runtime. Windows runtime, PowerShell bridge execution, native callback delivery, packaging on Windows, sleep/wake on either platform, hardware-specific input readback, and long-running hidden-window automation require foreground acceptance. CI is configured but has not been run remotely. The helper's incomplete DDC reply parser remains unchanged; Mac writes intentionally report sent/unverified.

Windows inventory launches an isolated PowerShell/native bridge, so enumeration can take seconds. It needs Windows performance measurement before release. DDC over another selected input remains hardware dependent. One controller host must own automatic switching. Single-hub switches are outside this preview.

Before public distribution: choose a project license and collect Rust dependency license notices, perform Windows and Mac hardware acceptance, then provide signed/notarized distribution as appropriate. No certificates, release credentials or publication are part of this change.
