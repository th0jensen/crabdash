// Resource compilation follows the Windows target, including cross-host GNU builds.
mod windows_resource;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=windows_resource.rs");
    println!("cargo:rerun-if-changed=../../assets/icons/AppIcon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_windows_identity()?;
    }
    Ok(())
}

fn embed_windows_identity() -> Result<(), Box<dyn std::error::Error>> {
    use std::{env, fs, path::PathBuf};
    let icon = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/icons/AppIcon.ico")
        .canonicalize()?;
    let icon = icon
        .to_str()
        .ok_or("The Windows application icon path is not valid Unicode")?;
    let resource = windows_resource::render(icon, env!("CARGO_PKG_VERSION"))?;
    let resource_path = PathBuf::from(env::var("OUT_DIR")?).join("crabdash.rc");
    fs::write(&resource_path, resource)?;
    // GPUI looks up resource ID 1 for the window/taskbar icon. Keep its manifest
    // in the existing GPUI backend and embed only this application's identity.
    embed_resource::compile(&resource_path, embed_resource::NONE).manifest_required()?;
    Ok(())
}
