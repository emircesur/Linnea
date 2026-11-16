fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut r = winresource::WindowsResource::new();
        r.set_icon("../../assets/kesit.ico").set_manifest_file("../../assets/app.manifest").set("ProductName", "Kesit").set("FileDescription", "Kesit CLI").set("CompanyName", "Kesit");
        if let Err(e) = r.compile() {
            println!("cargo:warning=icon not embedded: {e}");
        }
    }
}
