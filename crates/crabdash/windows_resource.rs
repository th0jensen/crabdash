//! Windows executable identity, derived from the same Cargo version as the app.

pub fn render(icon: &str, version: &str) -> Result<String, &'static str> {
    let mut numbers = version.split('.');
    let mut numeric = [0_u16; 4];
    for number in &mut numeric[..3] {
        *number = numbers
            .next()
            .ok_or("Windows resource version needs major.minor.patch")?
            .parse()
            .map_err(|_| "Windows resource version components must fit u16")?;
    }
    if numbers.next().is_some() {
        return Err("Windows resource version needs major.minor.patch");
    }
    // RC does not understand Win32 verbatim prefixes. Forward slashes also
    // avoid backslash escapes in paths with Unicode or whitespace.
    let icon = if let Some(path) = icon.strip_prefix(r"\\?\UNC\") {
        format!("//{path}")
    } else {
        icon.strip_prefix(r"\\?\").unwrap_or(icon).to_owned()
    };
    let icon = icon.replace('\\', "/").replace('"', "\"\"");
    let [major, minor, patch, revision] = numeric;
    Ok(format!(
        r#"#pragma code_page(65001)
1 ICON "{icon}"
1 VERSIONINFO
 FILEVERSION {major},{minor},{patch},{revision}
 PRODUCTVERSION {major},{minor},{patch},{revision}
 FILEFLAGSMASK 0x3fL
 FILEFLAGS 0x0L
 FILEOS 0x40004L
 FILETYPE 0x1L
 FILESUBTYPE 0x0L
BEGIN
 BLOCK "StringFileInfo"
 BEGIN
  BLOCK "040904B0"
  BEGIN
   VALUE "CompanyName", "th0jensen\0"
   VALUE "FileDescription", "Crabdash\0"
   VALUE "FileVersion", "{version}\0"
   VALUE "InternalName", "crabdash\0"
   VALUE "OriginalFilename", "crabdash.exe\0"
   VALUE "ProductName", "Crabdash\0"
   VALUE "ProductVersion", "{version}\0"
  END
 END
 BLOCK "VarFileInfo"
 BEGIN
  VALUE "Translation", 0x0409, 1200
 END
END
"#
    ))
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn numeric_and_display_versions_follow_cargo_version() {
        for version in ["0.1.5", "12.34.567"] {
            let resource = render("C:/Crabdash/AppIcon.ico", version).unwrap();
            let numeric = format!("{},0", version.replace('.', ","));
            assert!(resource.contains(&format!("FILEVERSION {numeric}")));
            assert!(resource.contains(&format!("PRODUCTVERSION {numeric}")));
            assert!(resource.contains(&format!(r#"VALUE "FileVersion", "{version}\0""#)));
            assert!(resource.contains(&format!(r#"VALUE "ProductVersion", "{version}\0""#)));
        }
    }

    #[test]
    fn rc_paths_keep_unicode_spaces_and_support_verbatim_local_and_unc_paths() {
        let local = render(r"\\?\C:\Thomas's 工作\AppIcon.ico", "0.1.5").unwrap();
        assert!(local.contains("1 ICON \"C:/Thomas's 工作/AppIcon.ico\""));
        let network = render(r"\\?\UNC\server\Crabdash folder\AppIcon.ico", "0.1.5").unwrap();
        assert!(network.contains("1 ICON \"//server/Crabdash folder/AppIcon.ico\""));
    }

    #[test]
    fn unrepresentable_versions_fail_instead_of_shipping_incorrect_metadata() {
        for version in ["0.1", "0.1.5.0", "0.1.5-beta", "65536.1.5", "a.1.5"] {
            assert!(render("icon.ico", version).is_err(), "{version}");
        }
    }
}
