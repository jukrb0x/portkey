#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "windows")]
    if std::env::args().any(|arg| arg == "--ddc-helper") {
        match portkey::platform::ddc_helper() {
            Ok(response) => {
                println!("{response}");
                return;
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }
    if let Err(error) = run() {
        if std::env::args().any(|arg| arg == "--diagnose") {
            eprintln!("{error}");
        } else {
            portkey::startup::show_error(&error.to_string());
        }
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|arg| arg == "--diagnose") {
        let inventory = portkey::platform::inventory().map_err(std::io::Error::other)?;
        println!("Displays:");
        for display in inventory.displays {
            println!("{}\t{}", display.id, display.label);
        }
        println!("USB hubs:");
        for hub in inventory.hubs {
            println!("{}\t{}", hub.id, hub.label);
        }
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    portkey::native_windows::run().map_err(std::io::Error::other)?;
    #[cfg(not(target_os = "windows"))]
    return Err("Launch the native AppKit bundle built by scripts/build-rust-app.sh. Use --diagnose for read-only discovery.".into());
    #[cfg(target_os = "windows")]
    Ok(())
}
