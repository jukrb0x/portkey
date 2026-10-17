#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    // Dedicated transport only: a missing/incorrect argument can never start a UI.
    if std::env::args().skip(1).collect::<Vec<_>>() != ["--ddc-helper"] {
        eprintln!("PortKeyHelper is a private bounded display transport.");
        std::process::exit(2);
    }
    #[cfg(target_os = "windows")]
    match portkey::platform::ddc_helper() {
        Ok(response) => println!("{response}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
    #[cfg(not(target_os = "windows"))]
    std::process::exit(2);
}
