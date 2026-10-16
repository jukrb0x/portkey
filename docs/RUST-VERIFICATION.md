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
