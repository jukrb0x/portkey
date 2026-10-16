# Platform references

The Rust desktop uses [eframe 0.29.1](https://docs.rs/eframe/0.29.1/eframe/), a Rust desktop window and rendering integration without a webview. The Cargo lockfile fixes the full resolved dependency graph.

Windows DDC uses the [low-level monitor configuration functions](https://learn.microsoft.com/en-us/windows/win32/monitor/using-the-low-level-monitor-configuration-functions), including [SetVCPFeature](https://learn.microsoft.com/en-us/windows/win32/api/lowlevelmonitorconfigurationapi/nf-lowlevelmonitorconfigurationapi-setvcpfeature). A physical monitor handle is transient, resolved from the enrolled display on each command; it is never saved as identity. Hardware and drivers determine whether DDC/CI is available while another input is selected.

The existing Mac helper's license and exact upstream revision remain in [ThirdParty/m1ddc](../ThirdParty/m1ddc/UPSTREAM.md). Rust does not change the helper's reply parser or turn a successful command into evidence of a visible picture change.
