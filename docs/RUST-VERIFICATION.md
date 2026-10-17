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
