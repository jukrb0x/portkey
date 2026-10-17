# Native UI replacement plan — 2026-10-16

The user approved replacing egui in the current branch while retaining the Rust core. This supersedes the prior egui UI choice. No routine approval checkpoints, physical writes, sleep tests or publishing.

## Architecture
Rust owns configuration, topology, policy, serialized worker, lifecycle, persistence and user-action validation. A UI-neutral Controller exposes JSON snapshots/actions; a narrow C ABI provides owned strings and an opaque handle to AppKit. macOS uses Swift/AppKit NSStatusItem, NSMenu and native settings controls. Windows uses Rust Win32 controls, a message loop/timer and Shell_NotifyIcon. No egui/eframe/tray-icon or browser framework remains.

## Interface contract
`Controller::new() -> Self`, `snapshot(&mut self) -> serde_json::Value`, `action(&mut self, action: serde_json::Value) -> serde_json::Value`.
Snapshot: `config` (Config or null), `inventory` (object displays/hubs arrays, or null), `ownership` (unknown/owned/absent/partial), `status` (string), `error` (bool), `lifecycle_ok` (bool), `sleeping` (bool), `language` (en/zh-Hans), `config_path` (string), `editing` (bool).
Actions: `{"type":"refresh"}`, `{"type":"save","config":Config}`, `{"type":"manual","target":"local"|"remote"}`, `{"type":"automatic","enabled":bool}`, `{"type":"language","value":"en"|"zh-Hans"}`, `{"type":"edit","enabled":bool}`, `{"type":"import","path":string}`, `{"type":"quit"}`. Responses: `{"ok":true}` or `{"ok":false,"error":string}`; import success additionally returns `config` for UI review without saving. Frontends own unsaved form values. Snapshot ticks policy; call every 200ms from the native event loop even while settings are hidden. Hardware operations stay off the UI thread.
C ABI: `pk_create()->void*`, `pk_snapshot(void*)->char*`, `pk_action(void*,const char*)->char*`, `pk_string_free(char*)`, `pk_destroy(void*)`. Main-thread use only. Each returned string must be freed exactly once. No panic crosses the ABI.

## Tasks and ownership
1. Runtime worker: src/controller.rs and src/ffi.rs (core worker); extract existing safety logic from app.rs, preserve tests and add ABI/action validation regressions.
2. AppKit: native/macos/* (Mac worker); real menu/settings, explicit enroll/input mapping, language, imports, close-to-menu lifecycle; link Rust static library.
3. Win32: src/native_windows.rs (Windows worker); real controls/tray/message loop, same action contract, no hardware writes during tests.
4. Root: Cargo/main/lib/dependencies, Mac/Windows scripts, docs and integration. Remove egui app/tray only after replaced. Build, unit tests, lint mac/windows, inspect packaged Mac UI, independent review, commit locally.

## Verification boundaries
Retain all safety constraints and the config-rust.json schema/path. First-run remains unenrolled and paused. No automatic legacy import or arbitrary display fallback. Keep pinned m1ddc license and raw diagnostic captures outside Git. Windows cross compilation is not native Windows runtime qualification. Native UI is not license to claim physical switching or sleep/wake acceptance.

## Profile extension — 2026-10-17
See [home/work plan](location-profiles.md). Snapshot additionally exposes `profiles: [{name, display_id}]`, `active_display_id: string|null`, and `profile_name: string`. Save accepts optional `profile_name`; `new` starts a blank draft while retaining saved profiles. `edit` with false cancels and resumes exact-display matching. The active configuration remains a Config v1; persistence is now the versioned `profiles-rust.json`, with read-only migration from the previous single-config file.
