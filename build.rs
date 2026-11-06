//! Build script: embed the Windows exe icon resource.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/icon.ico")
            .set("ProductName", "GiRustStatus")
            .set("FileDescription", "GiRustStatus - release tracker with VirusTotal download gate")
            .compile()
            .expect("embed icon resource");
    }
}
