//! Gives the Windows executable its icon and the name Explorer and Task
//! Manager show. Other targets need nothing here.

fn main() -> std::io::Result<()> {
    println!("cargo::rerun-if-changed=../../packaging/windows/icon.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }

    winresource::WindowsResource::new()
        .set_icon("../../packaging/windows/icon.ico")
        .set("ProductName", "ServerRemote")
        .set("FileDescription", "ServerRemote")
        .set("LegalCopyright", "Raresh Nistor. MIT OR Apache-2.0.")
        .compile()
}
