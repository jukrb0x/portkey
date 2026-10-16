# Rust cross-platform product design and implementation plan

## Intent and scope
The user requested a new branch, Rust rewrite, broader onboarding and Windows support. This supersedes the previous Swift-only/Mac-only scope. Decisions below are implementation choices delegated to the builder. Keep the historical Swift delivery available while the Rust replacement is validated. No publishing or physical switching during development.

## Architecture
One Rust package, `portkey`, with a tested pure core, native platform adapters, serialized bounded command worker, and a small cross-platform desktop settings/control window. Use egui/eframe for a compact Rust UI without a browser runtime. No network service, administrator requirement or keyboard/mouse capture. Both hosts awake remains the supported workflow. Run automation on one controller host only to avoid competing writes.

## Product behavior
First run has no enrolled monitor or hubs and cannot write. Discover displays and USB hubs; explicitly select one display, two distinct topology-identified hubs and local/remote VCP inputs. Save validated versioned JSON atomically in per-user storage. Never embed the developer's hardware IDs. Import old Mac configuration explicitly without overwriting it. English and Simplified Chinese UI. Manual switching, automation pause, actionable errors and clear transport/readback statuses. Changing enrollment cancels pending work. Startup absence/read failure never becomes remote ownership. Unknown or partial inventory cancels work. Sleep/wake resets ownership. No arbitrary display fallback.

## Interfaces and file ownership
- `src/core.rs`: serde types `Config`, `Hub`, `Display`, `Ownership`, `Target`, `Request`, `Policy`. Configuration uses platform string, display ID, local_input, remote_input, hubs array of two topology identities; implementations coordinate exact API before dependent edits.
- `src/platform.rs`, `src/platform/*`: `inventory() -> Result<Inventory,String>` (displays, hubs), `set_input(config, input) -> Result<WriteOutcome,String>`, native macOS and Windows implementations. External/helper operations bounded; Windows DDC calls isolated in child process. Re-enumerate display identity before write.
- `src/app.rs`: setup/control UI, persistent language/settings, background serial worker and lifecycle invalidation.
- `src/main.rs`, `src/lib.rs`, `Cargo.toml`, scripts, CI, README: root integration responsibility.

## Implementation tasks
1. Core/configuration: write failing policy/config tests, implement generic topology matching, fail-closed validation, atomic persistence/legacy import and test transitions/manual/sleep/races.
2. Platform: write parser/selection/timeout tests, implement Mac inventory and pinned helper invocation and Windows SetupAPI/monitor APIs (or native PowerShell bridge where justified), ensure exact enrolled display and bounded isolated execution.
3. Product UI/runtime: implement discovery/enrollment, language, controls, serialized latest-generation requests, recovery and actionable first-run state. Write deterministic runtime tests.
4. Integration: build scripts for Mac/Windows, reproducible lockfile, CI for both systems, English docs, update brief; run formatting, cargo tests/check/build, old regression tests, independent review and fixes.

## Review focus and verification
Duplicate hub VID/PIDs must not collide. Missing or ambiguous displays must not write. Enumeration failure must not look like both hubs absent. In-flight old work cannot overlap new requests and final state must converge. Sleep or long event-loop gaps discard pending departures. Config changes invalidate queued work. Actual Windows build/runtime and physical switching are separate evidence from host tests; report unexecuted validation explicitly. Preserve pinned helper and license.

## Delivery record

The four implementation areas landed on this branch with parallel core, platform and desktop workers plus independent review. The Rust package, tray/setup UI, native lifecycle notifications, Mac adapter and Windows bridge, scripts, CI and docs are implemented. See [verification](../RUST-VERIFICATION.md) for executed checks and explicit remaining platform/hardware acceptance. No physical writes, actual sleep, remote publication or deletion of the Swift fallback occurred.
