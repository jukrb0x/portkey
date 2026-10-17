# Native Windows USB observation

## Scope

Remove repeated PowerShell/PnP cmdlet enumeration from the USB observation path. Keep saved topology identities, native AppKit/Win32 UI and independent two-host policy. No network coordinator, automatic enrollment migration or physical trials.

## Implementation

1. Enumerate present USB devices with SetupAPI and read typed service/location properties. Validate hub status with Configuration Manager. Preserve existing full Windows location-path identity selection, including paired USB2/USB3 branches and identical model devices at different paths. Incomplete enumeration, malformed properties, ambiguous identity and device errors produce unknown/error, never absence.
2. Add an independent Windows USB observer, awakened by coalesced WM_DEVICECHANGE hints with periodic fallback. Notifications alone do not establish ownership. Keep startup absent unknown, partial branches non-actionable and policy debounce unchanged.
3. Refresh display/profile inventory separately every ten seconds and on explicit refresh. Full discovery remains required before writes, retries and probes. Keep lifecycle/revision guards; stale combined snapshots must not overwrite newer USB evidence. Repeated cached USB samples must not count as independent setup observations. Sleep/config/profile transitions must reset evidence safely.
4. Keep the PowerShell display/DDC bridge and its timeout/readback behavior. Add a display-only mode so native USB enumeration is not duplicated by the bridge. Historical inventory mode remains for read-only comparison.

## Verification

Deterministic tests cover typed native properties, exact location identities, malformed/incomplete snapshots, independent USB progress while display work blocks, rapid reversal, stale work cancellation, startup absence, partial branches and lifecycle recovery. Run host and native Windows suites, strict cross-target checks, existing mocked bridge tests and release builds. Compare native and old USB identities read-only on the current unchanged Windows snapshot if available. Keep machine identifiers and output outside Git. This verifies implementation and component costs, not visual switch time.
