# Guided native setup and two-host acceptance

## Approved intent

Users configure a physical USB switch without understanding registry identities, USB 2/3 companion hubs, or VCP input numbers. Keep Rust shared behavior and native AppKit/Win32 UI. Continue the existing branch without approval checkpoints. Hardware writes remain explicitly targeted and bounded.

## Design decisions

- Default setup selects a friendly display name and environment name, then learns the physical switch from stable disappearance and return of one or two topology-qualified hubs. Technical fields remain available under Advanced.
- Do not equate logical hub count with physical switch count. Failed enumeration is unknown; partial pairs and ambiguous candidate groups do not establish ownership. Enrollment never matches only VID/PID.
- Read the selected display's current input while that computer is visible. Display capabilities and standard port labels are insufficient proof of a firmware's actual values.
- Each host shows a short offline setup code for its measured local input. Enter the other host's code to supply the remote input. This transfers calibration only; it is not network communication or display identity matching. The user explicitly selects the display on each host. Explain that codes must come from the same physical monitor.
- Save completed setup with automation off, then perform directional tests and enable automation. Keep home/work profiles independent. Cancel and lifecycle changes invalidate pending probes and preserve saved profiles.
- Two mirrored policies request the same target on a normal handoff. No server/client protocol is added. Already-issued monitor commands cannot be revoked; rapid-reversal physical acceptance remains necessary.

## Work and acceptance

1. Shared guided-state machine, code validation, one/two-hub configuration, asynchronous exact-display reads, meaningful deterministic tests.
2. Native setup on both platforms, friendly device labels, advanced technical settings retained, singular physical-switch terminology.
3. Windows executable build and deployment to the existing authorized machine. Desktop-session execution is required for DDC; SSH is transport and diagnostics only.
4. Save measured home mappings on both hosts using their own explicit display identity and topology. Preserve historical work configuration.
5. Verify both native builds, core/helper tests, independent code review and UI flow. Then observe a USB round trip and bounded rapid reversal with the user. Record simulated, transport and visually verified results separately.

## Progress

- Mirrored policy simulations added; existing policy needed no behavioral change.
- Guided core, native frontends and platform readbacks implemented and independently reviewed. Native Mac/Windows builds and deterministic tests pass; physical learning and automatic acceptance remain in progress.
- Both directional manual DDC commands have user-confirmed visual success when sent by the computer currently displayed. Inactive-source Mac requests failed; raw identities and trial logs stay outside Git.
