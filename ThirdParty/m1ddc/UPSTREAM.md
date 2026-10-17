# Upstream source

Repository: https://github.com/waydabber/m1ddc
Revision: 04d949794102eb8df01ad3681afff6464a3eede2

This revision was compiled and physically tested with the current M5 Pro HDMI / Philips 27M2N5800 setup. Original LICENSE is retained. No prebuilt executable is committed. Any changes to upstream source must be documented here.

## Local reply-validation patch — 2026-10-17

The upstream revision remains pinned above. `convertI2CtoDDC` now requires the
received buffer length and requested VCP feature, validates the DDC/CI reply
source (0x6e), length (0x88), Get-VCP opcode, success result, feature echo, value
type and XOR checksum (0x50 seed) before decoding the two big-endian values.
Malformed, unsupported, truncated and mismatched replies return the existing
invalid sentinel; the CLI reports failure instead of printing an arbitrary
number. Direct byte decoding replaces the incorrect `sizeof(2)` NSData copy
length expression. On the tested Foundation implementation that expression
was clamped to the source NSData length; stack corruption was not reproduced.

Run `scripts/test-ddc.sh` for deterministic packet fixtures with I2C functions
stubbed to abort. No physical switching or monitor compatibility is established
by these tests. Get-VCP request checksum now includes its source address (0x51 for ordinary
features, 0x50 for the alternate input feature), and repeated preparation is
idempotent. The CLI identifies valid null replies without decoding trailing
padding as a value. Transport routing, timing and Set-VCP behavior are unchanged
by this patch. Original copyright and license are retained.
