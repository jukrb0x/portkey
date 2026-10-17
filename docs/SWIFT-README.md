# Swift migration and native build paths

The historical SwiftPM application has been retired from active source/build paths. PortKey now uses the shared Rust controller with SwiftUI/AppKit on macOS and C++/WinRT WinUI 3 on Windows. The Swift language is still used by the current Mac frontend in `native/macos`; it is the old business/hardware controller that was removed.

Use the [current README](../README.md) for requirements, setup, qualification, data locations, recovery and uninstall instructions. The canonical Mac commands are now:

```sh
./scripts/test.sh
./scripts/build-app.sh
./scripts/verify-bundle.sh dist/PortKey.app
open dist/PortKey.app
```

These scripts run/build/verify the native Rust-backed app. `scripts/build-rust-app.sh` is the direct native builder; no SwiftPM package or `swift run CoreTests` remains. Windows builds use `scripts/build-windows.ps1` and the full packaged directory described in [Windows instructions](../native/windows/README.md).

Historical Swift configuration can be explicitly imported on the Mac. The selected `config.json` is preserved; the imported setup requires review and stays unverified with automation off. Old Swift physical observations, preferences and logs are historical evidence rather than new-runtime qualification. Never launch a retained old binary beside the new controller.

During local retirement, the old source, tests, plist and launch/build instructions were archived in ignored `work/product-1.0/legacy-swift-source.tar.gz`, byte-verified before removal. Git history and the earlier source baseline also provide recovery. This is a local development archive, not a shipped dependency or supported parallel launcher. Existing `dist` fallback binaries are preserved until integration/deployment backup is complete.

The [historical design](design.md) and [implementation record](implementation-plan.md) retain earlier decisions and measured limits. Current behavior/evidence is recorded in the [implementation brief](IMPLEMENTATION-BRIEF.md) and [Rust/native verification](RUST-VERIFICATION.md). The pinned [m1ddc provenance and license](../ThirdParty/m1ddc/UPSTREAM.md) remain part of the native app bundle.
