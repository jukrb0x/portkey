fn main() {
    println!("cargo:rerun-if-changed=native/windows.manifest");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let manifest = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("native/windows.manifest");
        println!("cargo:rustc-link-arg-bin=portkey=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bin=portkey=/MANIFESTINPUT:{}",
            manifest.display()
        );
    }
}
