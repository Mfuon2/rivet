fn main() {
    println!("cargo:rerun-if-changed=../../assets/branding/Rivet.ico");
    #[cfg(target_os = "windows")]
    winresource::WindowsResource::new()
        .set_icon("../../assets/branding/Rivet.ico")
        .set("ProductName", "Rivet")
        .set("FileDescription", "Rivet Download Manager")
        .compile()
        .expect("compile Rivet Windows icon resource");
}
