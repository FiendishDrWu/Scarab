fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=assets/windows/scarab.ico");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/windows/scarab.ico");
        resource.compile()?;
    }

    Ok(())
}
