# Two-host automatic switching — 2026-10-17

## Approved scope

Run PortKey on both awake computers using the shared Rust controller, native AppKit on macOS and native Win32 on Windows. Each host explicitly enrolls its own stable display identity and the shared one/two-hub switch as visible in that host's topology. Keep the Swift fallback. No network listener, peer protocol or remote command transport is required.

Inputs are relative to the host. For the current home setup, Mac local = 5 and remote = 7; Windows local = 7 and remote = 5. Keep these values in local enrollment, never application defaults. Home/work profiles retain separate mappings and identities.

## Policy and races

The existing policy already expresses both ends. When USB moves from Mac to Windows, Mac's settled departure requests remote 7 and Windows's settled arrival requests local 7. The return trip requests 5 from both sides. Either active video source can therefore perform the handoff on displays that reject writes from an inactive source.

Arrival requires all enrolled branches continuously present for 300ms. Departure requires all continuously absent for 700ms after prior settled local ownership. Partial inventory invalidates automatic requests and restarts settling once a complete state returns. Unknown inventory clears established ownership. Startup/wake absence never implies remote ownership; startup presence may claim local after settling. No matching by VID/PID alone.

Each host retains serialized, bounded operations and generation cancellation. A rapid reversal invalidates its stale pending requests; settled new states generate corrections toward the same new input on both ends. Independent hosts do not share a lock or clock. An already executing write can finish after cancellation, so physical convergence still depends on controller cancellation/correction and monitor behavior. Deterministic policy agreement does not establish hardware reliability or cross-host execution ordering.

Missing or ambiguous display identity must remain fail-closed. Manual override remains local to the app where it was requested; there is no cross-host manual synchronization. Whole-switch disconnection remains indistinguishable from departure after established ownership. Both hosts awake and exact enrollment are required for the supported workflow.

## Evidence and validation

The user reported two visually successful Mac-active writes of input 7, two unsuccessful Mac-inactive attempts at input 5, and a visually successful Windows-active write of input 5. This motivates handing off from the active side; it does not establish arbitrary monitor compatibility or qualify automatic switching. Raw hardware diagnostics and personal identifiers remain outside Git.

Deterministic core tests cover both mirrored handoff directions, staggered USB2/USB3 arrival, startup unknown/absent/partial on either platform, lost inventory, and rapid reversal with generation invalidation. Controller tests cover stale worker completion and platform/runtime tests separately qualify enumeration and transport. Host-side simulations, Windows cross-compilation, successful command transport/readback, and user-confirmed visual switching are separate evidence categories.

Physical acceptance remains a foreground coordinated round trip with both apps enrolled and awake. Do not infer success from process exit alone. Coordinate any actual Mac sleep with the user; automated policy tests do not qualify real wake recovery.
