// Resource compilation follows the Windows target, including cross-host GNU builds.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/icons/AppIcon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_windows_icon()?;
    }
    Ok(())
}

fn embed_windows_icon() -> Result<(), Box<dyn std::error::Error>> {
    use std::{env, fs, path::PathBuf};
    let icon = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/icons/AppIcon.ico")
        .canonicalize()?;
    let icon = icon
        .to_str()
        .ok_or("The Windows application icon path is not valid Unicode")?;
    // canonicalize returns a Win32 verbatim prefix, which RC's filename parser
    // cannot interpret after slash normalization. Convert it to a normal path.
    let icon = match icon.strip_prefix(r"\\?\UNC\") {
        Some(path) => format!("//{path}"),
        None => match icon.strip_prefix(r"\\?\") {
            Some(path) => path.to_owned(),
            None => icon.to_owned(),
        },
    };
    // Forward slashes are accepted by RC and avoid backslash escape sequences;
    // an explicit code page also supports Unicode checkout directory names.
    let resource = format!(
        "#pragma code_page(65001)\n1 ICON \"{}\"\n",
        icon.replace('\\', "/")
    );
    let resource_path = PathBuf::from(env::var("OUT_DIR")?).join("crabdash-icon.rc");
    fs::write(&resource_path, resource)?;
    // GPUI looks up resource ID 1 for the window/taskbar icon. Keep its manifest
    // in the existing GPUI backend and embed only this application's identity.
    embed_resource::compile(&resource_path, embed_resource::NONE).manifest_required()?;
    Ok(())
}
