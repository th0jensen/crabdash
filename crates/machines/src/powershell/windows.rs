//! Pure Windows executable selection, also tested on non-Windows hosts.

/// Resolve only an existing bundled shell under an absolute Windows SystemRoot.
/// Injecting the existence check avoids host filesystem or environment changes
/// in tests. Preserve directory case, spaces and Unicode as one executable path.
pub(super) fn bundled_executable(
    system_root: Option<&str>,
    is_file: impl FnOnce(&str) -> bool,
) -> Option<String> {
    let root = system_root?;
    if root.trim().is_empty() || root.contains('\0') || !is_absolute_windows_path(root) {
        return None;
    }
    let root = root.trim_end_matches(['\\', '/']);
    let path = format!("{root}\\System32\\WindowsPowerShell\\v1.0\\powershell.exe");
    is_file(&path).then_some(path)
}

fn is_absolute_windows_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
    {
        return true;
    }
    // A UNC root must contain both server and share. Root-relative paths such
    // as \Windows or drive-relative paths such as C:Windows depend on cwd.
    if let Some(unc) = path.strip_prefix(r"\\") {
        let mut components = unc.split(['\\', '/']);
        return components.next().is_some_and(|server| !server.is_empty())
            && components.next().is_some_and(|share| !share.is_empty());
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_empty_or_relative_system_root_does_not_probe_cwd() {
        for root in [
            None,
            Some(""),
            Some("   "),
            Some("Windows"),
            Some(r"C:Windows"),
            Some(r"\Windows"),
            Some(r"\\server"),
            Some("C:\\bad\0root"),
        ] {
            assert_eq!(
                bundled_executable(root, |_| panic!("Invalid root must not probe filesystem")),
                None
            );
        }
    }

    #[test]
    fn existing_absolute_root_preserves_spaces_unicode_and_case() {
        let expected = r"c:\Windows Files\雪\System32\WindowsPowerShell\v1.0\powershell.exe";
        let selected = bundled_executable(Some("c:\\Windows Files\\雪\\"), |path| {
            assert_eq!(path, expected);
            true
        });
        assert_eq!(selected.as_deref(), Some(expected));
        assert_eq!(bundled_executable(Some(r"C:\Windows"), |_| false), None);
    }

    #[test]
    fn drive_and_unc_roots_resolve_without_host_path_semantics() {
        for root in [r"C:\Windows", "C:/Windows/", r"\\server\share\Windows"] {
            let expected = format!(
                "{}\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
                root.trim_end_matches(['\\', '/'])
            );
            assert_eq!(
                bundled_executable(Some(root), |path| path == expected),
                Some(expected)
            );
        }
    }
}
