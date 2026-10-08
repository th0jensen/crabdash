//! Native POSIX discovery also covers SSH into Linux/WSL distributions.
use crate::machine::Machine;
use anyhow::Result;
use std::path::{Path, PathBuf};
use utils::{args, args::Args};

const PATHS: &[&str] = &[
    "/opt/homebrew/bin/docker",
    "/usr/local/bin/docker",
    "/usr/bin/docker",
];

fn executable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        false
    }
}

pub(in crate::docker) fn resolve_executable(
    paths: impl IntoIterator<Item = PathBuf>,
) -> Option<String> {
    paths
        .into_iter()
        .find(|path| executable(path))
        .map(|path| path.to_string_lossy().into_owned())
}

pub(super) fn local(cached: Option<&str>) -> Option<String> {
    let search = std::env::var_os("PATH").into_iter().flat_map(|path| {
        std::env::split_paths(&path)
            .map(|directory| directory.join("docker"))
            .collect::<Vec<_>>()
    });
    resolve_executable(
        cached
            .into_iter()
            .chain(PATHS.iter().copied())
            .map(PathBuf::from)
            .chain(search)
            .chain(
                dirs::home_dir()
                    .into_iter()
                    .map(|home| home.join(".docker/bin/docker")),
            ),
    )
}

pub(super) async fn remote(machine: &mut Machine) -> Result<Option<String>> {
    // Missing Docker is ordinary state; SSH and command failures stay errors.
    let mut args = args![
        "-c",
        r#"for path in "$@" "$HOME/.docker/bin/docker"; do if [ -f "$path" ] && [ -x "$path" ]; then printf '%s' "$path"; exit; fi; done; command -v docker || true"#,
        "crabdash-docker"
    ];
    if let Some(cached) = &machine.docker_path {
        args.push(cached.clone());
    }
    args.0.extend(PATHS.iter().map(|path| (*path).to_owned()));
    let path = String::from(machine.run("sh", &args).await?);
    Ok((!path.is_empty()).then_some(path))
}
