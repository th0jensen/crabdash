use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf();

    let git_dir = workspace_root.join(".git");
    let head_path = git_dir.join("HEAD");

    if head_path.exists() {
        println!("cargo:rerun-if-changed={}", head_path.display());

        if let Ok(head) = fs::read_to_string(&head_path) {
            if let Some(reference) = head.strip_prefix("ref: ").map(str::trim) {
                let reference_path = git_dir.join(reference);
                if reference_path.exists() {
                    println!("cargo:rerun-if-changed={}", reference_path.display());
                }
            }
        }
    }

    let commit_hash = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&workspace_root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|stdout| stdout.trim().to_string())
        .filter(|stdout| !stdout.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    let icon_path = workspace_root.join("assets/icons/AppIcon.png");

    println!("cargo:rustc-env=CRABDASH_GIT_COMMIT_HASH={commit_hash}");
    println!(
        "cargo:rustc-env=CRABDASH_APP_ICON_PATH={}",
        icon_path.display()
    );
}
