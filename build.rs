//! Build script: embed the Windows exe icon resource.
//! `winresource` is a Windows-host build-dependency, so the call is gated
//! on the host as well as the target (Linux builds skip it entirely).

fn main() {
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/icon.ico")
            .set("ProductName", "Cairnwatch")
            .set("FileDescription", "Cairnwatch - release tracker with VirusTotal download gate")
            .compile()
            .expect("embed icon resource");
    }
}
