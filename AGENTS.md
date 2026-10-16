# Project working agreement

- Work only in this standalone repository; do not edit the user's chezmoi dotfiles.
- The user approved implementation and architecture choices; keep progressing without routine approval checkpoints.
- Use native Swift and macOS APIs with a small menu-bar UI. Prefer a small, understandable design over extra frameworks.
- Read docs/IMPLEMENTATION-BRIEF.md first, then the current spec and plan. The brief records the latest scope and supersedes historical proposal text.
- Local hardware diagnostics stay outside Git; recorded facts are measured evidence; distinguish simulated tests, command transport success and real visual switching.
- Device identity includes USB topology, because identical hub VID/PIDs coexist. Do not match by VID/PID alone or persist registry entry IDs.
- All display writes target the enrolled Philips explicitly. Never write to arbitrary displays or infer Windows ownership from startup absence.
- Add meaningful deterministic tests for policy, races, helper failures, configuration and sleep/wake recovery. Keep hardware tests opt-in/foreground and bounded.
- Retain third-party licenses and the pinned m1ddc provenance. No credentials, user keystrokes or mouse content are needed.
- Local build/run and the requested hardware tests are authorized. Coordinate actual Mac sleep with the user so they can wake the machine and observe the result.
