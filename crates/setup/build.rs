fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut r = winresource::WindowsResource::new();
        r.set_icon("../../assets/linnea.ico").set_manifest_file("../../assets/app.manifest").set("ProductName", "Linnea").set("FileDescription", "Linnea Setup").set("CompanyName", "Linnea");
        if let Err(e) = r.compile() {
            println!("cargo:warning=icon not embedded: {e}");
        }
    }
}
