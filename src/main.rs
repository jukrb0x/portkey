#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
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
    let config = portkey::core::Config::path().map_err(std::io::Error::other)?;
    let _instance =
        portkey::instance::acquire(config.parent().ok_or("Invalid configuration directory")?)
            .map_err(std::io::Error::other)?;
    portkey::app::run()?;
    Ok(())
}
