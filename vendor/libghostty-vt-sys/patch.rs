//! Stage native sources and apply the extension without modifying their checkout.
use std::{
    borrow::Cow,
    collections::hash_map::DefaultHasher,
    ffi::OsStr,
    fs::{self, OpenOptions},
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[cfg(not(windows))]
fn git_path(path: &Path) -> Cow<'_, OsStr> {
    Cow::Borrowed(path.as_os_str())
}

#[cfg(windows)]
fn git_path(path: &Path) -> Cow<'_, OsStr> {
    use std::{
        ffi::OsString,
        os::windows::ffi::{OsStrExt, OsStringExt},
        path::{Component, Prefix},
    };
    // Git for Windows rejects Rust's verbatim prefix. Preserve native paths for
    // filesystem calls and remove only recognized disk/UNC prefixes in argv.
    let Some(Component::Prefix(prefix)) = path.components().next() else {
        return Cow::Borrowed(path.as_os_str());
    };
    let (skip, unc) = match prefix.kind() {
        Prefix::VerbatimDisk(_) => (4, false),
        Prefix::VerbatimUNC(_, _) => (8, true),
        _ => return Cow::Borrowed(path.as_os_str()),
    };
    let mut wide = if unc { vec![92, 92] } else { Vec::new() };
    wide.extend(path.as_os_str().encode_wide().skip(skip));
    Cow::Owned(OsString::from_wide(&wide))
}

fn git(source: &Path, args: &[&str], patch: Option<&Path>) -> Result<Output, String> {
    let mut command = Command::new("git");
    #[cfg(windows)]
    command.args(["-c", "core.longpaths=true"]);
    command.arg("-C").arg(git_path(source)).args(args);
    if let Some(patch) = patch {
        command.arg(git_path(patch));
    }
    command
        .output()
        .map_err(|error| format!("run git: {error}"))
}

/// Copy the current working tree, including local edits, into a build-owned
/// checkout. Git metadata and ignored build/package caches stay in the source.
/// The destination has independent metadata, so restored Cargo caches never
/// borrow a worktree's .git path or its object database.
pub fn stage(
    source: &Path,
    destination: &Path,
    patches: &[PathBuf],
) -> Result<Vec<PathBuf>, String> {
    let source = source
        .canonicalize()
        .map_err(|error| format!("read source: {error}"))?;
    let parent = destination
        .parent()
        .ok_or("staging directory needs a parent")?;
    fs::create_dir_all(parent).map_err(|error| format!("create staging parent: {error}"))?;
    let destination = parent
        .canonicalize()
        .map_err(|error| format!("read staging parent: {error}"))?
        .join(
            destination
                .file_name()
                .ok_or("staging directory needs a name")?,
        );
    if destination.starts_with(&source) || source.starts_with(&destination) {
        return Err("source and staging directories must not overlap".to_owned());
    }
    // Each OUT_DIR owns this lock. Concurrent stages cannot remove or patch
    // each other's copy; no lock or other file is written into the source.
    let lock_path = destination.with_extension("stage.lock");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|error| format!("open staging lock: {error}"))?;
    lock.lock()
        .map_err(|error| format!("lock staged checkout: {error}"))?;

    let files = source_files(&source)?;
    let source_hash = fingerprint(&source, &files)?;
    let mut input = DefaultHasher::new();
    source_hash.hash(&mut input);
    for patch in patches {
        fs::read(patch)
            .map_err(|error| format!("read native patch: {error}"))?
            .hash(&mut input);
    }
    let input = input.finish();
    let stamp = destination.join(".git/crabdash-stage");
    // Retain source mtimes and Zig's native cache when the inputs and the
    // complete staged output match. A partially restored/edited cache rebuilds.
    let cached = fs::read_to_string(&stamp).ok().and_then(|value| {
        let mut fields = value.split_whitespace();
        Some((
            fields.next()?.parse::<u64>().ok()?,
            fields.next()?.parse::<u64>().ok()?,
        ))
    });
    if let Some((old_input, old_output)) = cached
        && old_input == input
        && let Ok(staged_files) = staged_files(&destination)
        && fingerprint(&destination, &staged_files)? == old_output
    {
        for patch in patches {
            ensure(&destination, patch)?;
        }
        return Ok(files.into_iter().map(|file| source.join(file)).collect());
    }

    if destination.exists() {
        fs::remove_dir_all(&destination)
            .map_err(|error| format!("remove old staged source: {error}"))?;
    }
    fs::create_dir_all(&destination).map_err(|error| format!("create staged source: {error}"))?;
    for file in &files {
        let target = destination.join(file);
        fs::create_dir_all(target.parent().unwrap())
            .map_err(|error| format!("create staged directory: {error}"))?;
        // Copy working-tree bytes, not HEAD: source overrides may carry local
        // edits, including an already-applied copy of this extension.
        fs::copy(source.join(file), &target)
            .map_err(|error| format!("copy {}: {error}", file.display()))?;
    }
    let initialized = git(&destination, &["init", "--quiet"], None)?;
    if !initialized.status.success() {
        return Err(format!(
            "initialize staged checkout: {}",
            String::from_utf8_lossy(&initialized.stderr)
        ));
    }
    for patch in patches {
        ensure(&destination, patch)?;
    }
    let output = fingerprint(&destination, &staged_files(&destination)?)?;
    fs::write(stamp, format!("{input} {output}\n"))
        .map_err(|error| format!("write staging stamp: {error}"))?;
    Ok(files.into_iter().map(|file| source.join(file)).collect())
}

fn source_files(source: &Path) -> Result<Vec<PathBuf>, String> {
    let output = git(
        source,
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
        None,
    )?;
    if !output.status.success() {
        return Err(format!(
            "{} must be a Git checkout: {}",
            source.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let mut files = Vec::new();
    for bytes in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|bytes| !bytes.is_empty())
    {
        #[cfg(unix)]
        let path = {
            use std::os::unix::ffi::OsStringExt;
            PathBuf::from(std::ffi::OsString::from_vec(bytes.to_vec()))
        };
        #[cfg(not(unix))]
        let path = PathBuf::from(
            std::str::from_utf8(bytes).map_err(|error| format!("read source path: {error}"))?,
        );
        if path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err("Git source inventory contains a path outside its checkout".to_owned());
        }
        if path.components().any(|part| {
            matches!(
                part.as_os_str().to_str(),
                Some(".git" | ".zig-cache" | "zig-cache" | "zig-out")
            )
        }) {
            continue;
        }
        if source.join(&path).is_file() {
            files.push(path);
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

// Unlike the source inventory, include every staged file, including files
// created by a patch or tracked source files matching a .gitignore rule.
fn staged_files(source: &Path) -> Result<Vec<PathBuf>, String> {
    fn walk(root: &Path, relative: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in fs::read_dir(root.join(relative))
            .map_err(|error| format!("read staged directory: {error}"))?
        {
            let entry = entry.map_err(|error| format!("read staged entry: {error}"))?;
            if relative.as_os_str().is_empty() && entry.file_name() == ".git" {
                continue;
            }
            let path = relative.join(entry.file_name());
            if entry.path().is_dir() {
                walk(root, &path, files)?;
            } else {
                files.push(path);
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(source, Path::new(""), &mut files)?;
    files.sort();
    Ok(files)
}

/// Track Git index changes (newly added source files) without watching ignored
/// Zig caches or the build-owned staged tree recursively.
pub fn index_path(source: &Path) -> Result<PathBuf, String> {
    let output = git(source, &["rev-parse", "--git-path", "index"], None)?;
    if !output.status.success() {
        return Err("read source Git index path".to_owned());
    }
    let path = PathBuf::from(
        String::from_utf8(output.stdout)
            .map_err(|error| format!("read Git index path: {error}"))?
            .trim(),
    );
    Ok(if path.is_absolute() {
        path
    } else {
        source.join(path)
    })
}

fn fingerprint(source: &Path, files: &[PathBuf]) -> Result<u64, String> {
    let mut hash = DefaultHasher::new();
    for file in files {
        file.hash(&mut hash);
        fs::read(source.join(file))
            .map_err(|error| format!("read {}: {error}", source.join(file).display()))?
            .hash(&mut hash);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::metadata(source.join(file))
                .map_err(|error| format!("read file mode: {error}"))?
                .permissions()
                .mode()
                .hash(&mut hash);
        }
    }
    Ok(hash.finish())
}

pub fn ensure(source: &Path, patch: &Path) -> Result<(), String> {
    let patch = patch
        .canonicalize()
        .map_err(|error| format!("read {}: {error}", patch.display()))?;
    let metadata = git(source, &["rev-parse", "--absolute-git-dir"], None)?;
    if !metadata.status.success() {
        return Err(format!("{} must be a Git checkout", source.display()));
    }
    let metadata = String::from_utf8(metadata.stdout)
        .map_err(|error| format!("read Git metadata path: {error}"))?;
    // Keep a stable lock file in Git metadata, outside the tracked source.
    // Parallel callers share this lock while checking/applying the patch.
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(Path::new(metadata.trim()).join("crabdash-resize-patch.lock"))
        .map_err(|error| format!("open patch lock: {error}"))?;
    lock.lock()
        .map_err(|error| format!("lock Ghostty checkout: {error}"))?;

    if git(source, &["apply", "--reverse", "--check"], Some(&patch))?
        .status
        .success()
    {
        return Ok(());
    }
    let check = git(source, &["apply", "--check"], Some(&patch))?;
    if !check.status.success() {
        return Err(format!(
            "{} is incompatible with {}: {}",
            source.display(),
            patch.display(),
            String::from_utf8_lossy(&check.stderr).trim()
        ));
    }
    let applied = git(source, &["apply"], Some(&patch))?;
    if !applied.status.success() {
        return Err(format!(
            "apply native extension: {}",
            String::from_utf8_lossy(&applied.stderr).trim()
        ));
    }
    if !git(source, &["apply", "--reverse", "--check"], Some(&patch))?
        .status
        .success()
    {
        return Err("native extension did not apply completely".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    struct Fixture(PathBuf);

    #[cfg(windows)]
    #[test]
    fn git_arguments_preserve_unicode_without_verbatim_disk_or_unc_prefixes() {
        for (native, expected) in [
            (
                r"\\?\C:\Thomas 工作\change.patch",
                r"C:\Thomas 工作\change.patch",
            ),
            (
                r"\\?\UNC\server\share\工作\change.patch",
                r"\\server\share\工作\change.patch",
            ),
            (
                r"C:\Thomas 工作\change.patch",
                r"C:\Thomas 工作\change.patch",
            ),
            (
                r"\\server\share\工作\change.patch",
                r"\\server\share\工作\change.patch",
            ),
            (
                r"\\?\Volume{abc}\change.patch",
                r"\\?\Volume{abc}\change.patch",
            ),
        ] {
            assert_eq!(&*git_path(Path::new(native)), OsStr::new(expected));
        }
    }

    impl Fixture {
        fn new(content: &str) -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "crabdash-native-patch-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).expect("create isolated fixture");
            assert!(
                git(&path, &["init", "--quiet"], None)
                    .expect("initialize fixture")
                    .status
                    .success()
            );
            // These fixtures assert exact LF bytes, including in staged copies.
            // Do not inherit a developer or Windows runner's core.autocrlf setting.
            fs::write(path.join(".gitattributes"), "* text eol=lf\n")
                .expect("set fixture line endings");
            fs::write(path.join("value"), content).expect("write native fixture");
            fs::write(
                path.join("change.patch"),
                "diff --git a/value b/value\n--- a/value\n+++ b/value\n@@ -1 +1 @@\n-before\n+after\n",
            )
            .expect("write patch fixture");
            Self(path)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn applies_once_and_accepts_already_applied_checkout() {
        let fixture = Fixture::new("before\n");
        let patch = fixture.0.join("change.patch");
        ensure(&fixture.0, &patch).expect("apply extension");
        ensure(&fixture.0, &patch).expect("accept existing extension");
        assert_eq!(
            fs::read_to_string(fixture.0.join("value")).unwrap(),
            "after\n"
        );
    }

    #[test]
    fn mismatched_checkout_fails_without_changing_source() {
        let fixture = Fixture::new("different\n");
        assert!(ensure(&fixture.0, &fixture.0.join("change.patch")).is_err());
        assert_eq!(
            fs::read_to_string(fixture.0.join("value")).unwrap(),
            "different\n"
        );
    }

    #[test]
    fn concurrent_targets_apply_one_complete_extension() {
        let fixture = Fixture::new("before\n");
        std::thread::scope(|scope| {
            let tasks = (0..4)
                .map(|_| {
                    let path = &fixture.0;
                    scope.spawn(move || ensure(path, &path.join("change.patch")))
                })
                .collect::<Vec<_>>();
            for task in tasks {
                task.join().expect("join target").expect("apply extension");
            }
        });
        assert_eq!(
            fs::read_to_string(fixture.0.join("value")).unwrap(),
            "after\n"
        );
    }
    fn staged(fixture: &Fixture, output: &Fixture) -> PathBuf {
        let destination = output.0.join("native");
        stage(&fixture.0, &destination, &[fixture.0.join("change.patch")])
            .expect("stage native extension");
        destination
    }

    #[test]
    fn staging_preserves_original_and_reuses_unchanged_native_sources() {
        let source = Fixture::new("before\n");
        let output = Fixture::new("unused\n");
        let metadata_before = fs::read_dir(source.0.join(".git")).unwrap().count();
        let destination = staged(&source, &output);
        let modified = fs::metadata(destination.join("value"))
            .unwrap()
            .modified()
            .unwrap();
        staged(&source, &output);
        assert_eq!(
            fs::read_to_string(source.0.join("value")).unwrap(),
            "before\n"
        );
        assert_eq!(
            fs::read_dir(source.0.join(".git")).unwrap().count(),
            metadata_before
        );
        assert_eq!(
            fs::read_to_string(destination.join("value")).unwrap(),
            "after\n"
        );
        assert_eq!(
            fs::metadata(destination.join("value"))
                .unwrap()
                .modified()
                .unwrap(),
            modified
        );
        assert!(destination.join(".git").is_dir());
        assert!(!destination.join(".git/objects/info/alternates").exists());
    }

    #[test]
    fn staging_keeps_local_edits_and_refreshes_additions_deletions_and_patch_changes() {
        let source = Fixture::new("before\n");
        let output = Fixture::new("unused\n");
        fs::write(source.0.join("extra"), "committed\n").unwrap();
        fs::write(source.0.join("deleted"), "committed\n").unwrap();
        fs::write(source.0.join("ignored-tracked"), "tracked source\n").unwrap();
        assert!(
            git(
                &source.0,
                &["add", "value", "extra", "deleted", "ignored-tracked"],
                None
            )
            .unwrap()
            .status
            .success()
        );
        fs::write(source.0.join("extra"), "local edit\n").unwrap();
        fs::remove_file(source.0.join("deleted")).unwrap();
        fs::write(source.0.join(".gitignore"), "/ignored\n/ignored-tracked\n").unwrap();
        fs::write(source.0.join("ignored"), "cache\n").unwrap();
        fs::create_dir(source.0.join(".zig-cache")).unwrap();
        fs::write(source.0.join(".zig-cache/artifact"), "cache\n").unwrap();
        let destination = staged(&source, &output);
        assert_eq!(
            fs::read_to_string(destination.join("extra")).unwrap(),
            "local edit\n"
        );
        assert!(!destination.join("deleted").exists());
        assert!(!destination.join("ignored").exists());
        assert_eq!(
            fs::read_to_string(destination.join("ignored-tracked")).unwrap(),
            "tracked source\n"
        );
        assert!(!destination.join(".zig-cache").exists());
        fs::write(source.0.join("extra"), "updated local edit\n").unwrap();
        fs::write(source.0.join("new"), "new source\n").unwrap();
        let patch = source.0.join("change.patch");
        fs::write(
            &patch,
            fs::read_to_string(&patch)
                .unwrap()
                .replace("+after", "+new extension"),
        )
        .unwrap();
        staged(&source, &output);
        assert_eq!(
            fs::read_to_string(destination.join("value")).unwrap(),
            "new extension\n"
        );
        assert_eq!(
            fs::read_to_string(destination.join("extra")).unwrap(),
            "updated local edit\n"
        );
        assert_eq!(
            fs::read_to_string(destination.join("new")).unwrap(),
            "new source\n"
        );
        fs::remove_file(source.0.join("new")).unwrap();
        staged(&source, &output);
        assert!(!destination.join("new").exists());
        assert_eq!(
            fs::read_to_string(source.0.join("value")).unwrap(),
            "before\n"
        );
    }

    #[test]
    fn staging_accepts_prepatched_source_and_applies_multiple_extensions() {
        let source = Fixture::new("after\n");
        let output = Fixture::new("unused\n");
        fs::write(source.0.join("other"), "before\n").unwrap();
        let other_patch = source.0.join("other.patch");
        fs::write(
            &other_patch,
            fs::read_to_string(source.0.join("change.patch"))
                .unwrap()
                .replace("a/value", "a/other")
                .replace("b/value", "b/other"),
        )
        .unwrap();
        let destination = output.0.join("native");
        let patches = [source.0.join("change.patch"), other_patch];
        stage(&source.0, &destination, &patches).unwrap();
        stage(&source.0, &destination, &patches).unwrap();
        assert_eq!(
            fs::read_to_string(destination.join("other")).unwrap(),
            "after\n"
        );
        assert_eq!(
            fs::read_to_string(source.0.join("other")).unwrap(),
            "before\n"
        );
    }

    #[test]
    fn staged_metadata_survives_cache_relocation_and_corrupt_sources_are_repaired() {
        let source = Fixture::new("before\n");
        let output = Fixture::new("unused\n");
        let destination = staged(&source, &output);
        let restored = output.0.join("restored");
        fs::rename(&destination, &restored).unwrap();
        fs::remove_dir_all(source.0.join(".git")).unwrap();
        ensure(&restored, &source.0.join("change.patch")).unwrap();
        assert_eq!(
            fs::read_to_string(restored.join("value")).unwrap(),
            "after\n"
        );
        assert!(
            git(&source.0, &["init", "--quiet"], None)
                .unwrap()
                .status
                .success()
        );
        fs::write(restored.join("value"), "corrupted cache\n").unwrap();
        stage(&source.0, &restored, &[source.0.join("change.patch")]).unwrap();
        assert_eq!(
            fs::read_to_string(restored.join("value")).unwrap(),
            "after\n"
        );
    }

    #[test]
    fn concurrent_staging_serializes_shared_output_without_mutating_source() {
        let source = Fixture::new("before\n");
        let output = Fixture::new("unused\n");
        std::thread::scope(|scope| {
            let tasks = (0..4)
                .map(|_| scope.spawn(|| staged(&source, &output)))
                .collect::<Vec<_>>();
            for task in tasks {
                task.join().unwrap();
            }
        });
        assert_eq!(
            fs::read_to_string(source.0.join("value")).unwrap(),
            "before\n"
        );
        assert_eq!(
            fs::read_to_string(output.0.join("native/value")).unwrap(),
            "after\n"
        );
    }
}
