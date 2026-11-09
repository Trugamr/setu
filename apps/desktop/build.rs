//! Embeds the app icon in the Windows executables.

fn main() {
    let icon = "../../design/brand/setu-dark.ico";
    println!("cargo:rerun-if-changed={icon}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon(icon)
            .compile()
            .expect("could not embed the app icon");
    }
}
