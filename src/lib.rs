pub mod app;
pub mod core;
pub mod instance;
pub mod lifecycle;
pub mod messages;
pub mod platform;
pub mod runtime;
pub mod startup;
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub mod tray;
