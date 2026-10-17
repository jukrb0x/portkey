# Home/work display profiles — 2026-10-17

## Approved intent
The user wants the same app to work when plugging into different displays at home and work. Continue native AppKit interface acceptance and fix problems directly. Architecture and routine implementation choices are delegated.

## Implementation
Keep Rust Config v1 as each profile's hardware enrollment. Store a versioned profiles-rust.json with named profiles keyed by exact display identity. Read the previous config-rust.json as one profile if no profile store exists; preserve the original file. Each profile keeps its own hub topology, input mapping and automation setting. UI language is shared.

On reliable inventory, activate only exactly one saved connected display identity. Unknown displays require explicit enrollment; missing or ambiguous saved displays cannot write. A profile transition cancels queued work and resets ownership, so absence in a new location never implies remote ownership. Editing freezes activation and pauses work. New setup leaves prior profiles intact; cancel restores discovery-based selection. Saving upserts only the explicitly chosen display. No guessed input mapping or USB topology.

Native settings expose profile name, current/saved profile summary and New setup. Drafts survive polling/language changes. An active-profile change reseeds controls only outside editing. Existing display and paired-hub enrollment validation remains.

## Validation
Deterministic profile migration/persistence, home/work transitions, ambiguous/no display, edit/new/cancel and stale work tests. Build AppKit bundle and inspect native UI through accessibility and screenshots. Windows cross-target lint only; distinguish it from runtime tests. Keep diagnostic identities out of Git. No actual sleep or display writes in this UI acceptance pass. A current environment without shared hubs cannot complete automatic enrollment.
