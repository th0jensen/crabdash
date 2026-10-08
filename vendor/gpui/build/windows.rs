//! Windows target resources and release DXBC shaders, independent of the build host.
use std::{
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const MODULES: [&str; 8] = [
    "quad",
    "shadow",
    "path_rasterization",
    "path_sprite",
    "underline",
    "monochrome_sprite",
    "polychrome_sprite",
    "emoji_rasterization",
];
const SOURCES: [&str; 3] = [
    "shaders.hlsl",
    "color_text_raster.hlsl",
    "alpha_correction.hlsl",
];

pub(super) fn build() -> Result<()> {
    println!("cargo:rerun-if-changed=build/windows.rs");
    println!("cargo:rerun-if-env-changed=GPUI_FXC_PATH");
    println!("cargo:rerun-if-env-changed=GPUI_VKD3D_COMPILER");
    for source in SOURCES {
        println!("cargo:rerun-if-changed=src/platform/windows/{source}");
    }
    // Keep the renderer's existing debug/runtime versus release/embedded contract.
    #[cfg(not(debug_assertions))]
    compile_shaders()?;
    #[cfg(feature = "windows-manifest")]
    {
        let manifest = Path::new("resources/windows/gpui.manifest.xml");
        let resource = Path::new("resources/windows/gpui.rc");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rerun-if-changed={}", resource.display());
        embed_resource::compile(resource, embed_resource::NONE).manifest_required()?;
    }
    Ok(())
}

enum Compiler {
    Fxc(PathBuf),
    Vkd3d(PathBuf),
}

impl Compiler {
    fn find() -> Result<Self> {
        if let Some(path) = env::var_os("GPUI_VKD3D_COMPILER") {
            return executable(path.into(), "GPUI_VKD3D_COMPILER").map(Self::Vkd3d);
        }
        if let Some(path) = env::var_os("GPUI_FXC_PATH") {
            return executable(path.into(), "GPUI_FXC_PATH").map(Self::Fxc);
        }
        if let Ok(output) = Command::new("where.exe").arg("fxc.exe").output()
            && output.status.success()
            && let Some(path) = first_existing_path(&String::from_utf8_lossy(&output.stdout))
        {
            return Ok(Self::Fxc(path));
        }
        let sdk =
            PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\fxc.exe");
        if sdk.is_file() {
            return Ok(Self::Fxc(sdk));
        }
        Err("No Windows shader compiler found. Set GPUI_FXC_PATH to native fxc.exe or GPUI_VKD3D_COMPILER to a host-executable HLSL-to-DXBC vkd3d-compiler.".into())
    }

    fn command(&self, entry: &str, profile: &str, source: &Path, output: &Path) -> Command {
        match self {
            Self::Fxc(path) => {
                let mut command = Command::new(path);
                command
                    .args(["/T", profile, "/E", entry, "/Fo"])
                    .arg(output)
                    .arg("/O3")
                    .arg(source);
                command
            }
            Self::Vkd3d(path) => {
                let mut command = Command::new(path);
                // Its default include callback resolves quoted HLSL includes from CWD.
                if let Some(parent) = source
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                {
                    command.current_dir(parent);
                }
                command
                    .args([
                        "-x", "hlsl", "-b", "dxbc-tpf", "-p", profile, "-e", entry, "-o",
                    ])
                    .arg(output)
                    .arg(source);
                command
            }
        }
    }
}

fn executable(path: PathBuf, variable: &str) -> Result<PathBuf> {
    if !path.is_file() {
        return Err(format!(
            "{variable} must name an existing host executable file: {}",
            path.display()
        )
        .into());
    }
    // Execution permissions and binary compatibility are checked by Command::output.
    path.canonicalize().map_err(|error| {
        format!(
            "Cannot resolve {variable} host executable {}: {error}",
            path.display()
        )
        .into()
    })
}

fn first_existing_path(paths: &str) -> Option<PathBuf> {
    paths
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .find(|path| path.is_file())
}

fn compile_shaders() -> Result<()> {
    let source_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is missing")?)
            .join("src/platform/windows");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is missing")?);
    let compiler = Compiler::find()?;
    let mut bindings = String::new();
    for module in MODULES {
        let source = source_dir.join(if module == "emoji_rasterization" {
            "color_text_raster.hlsl"
        } else {
            "shaders.hlsl"
        });
        for (stage, suffix, profile) in [("vertex", "vs", "vs_4_1"), ("fragment", "ps", "ps_4_1")] {
            let entry = format!("{module}_{stage}");
            let filename = format!("{module}_{suffix}.cso");
            let output = out_dir.join(&filename);
            match fs::remove_file(&output) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(
                        format!("Cannot replace shader {}: {error}", output.display()).into(),
                    );
                }
            }
            let result = compiler
                .command(&entry, profile, &source, &output)
                .output()
                .map_err(|error| {
                    format!("Cannot run Windows shader compiler for {entry}: {error}")
                })?;
            if !result.status.success() {
                return Err(format!(
                    "Windows shader compilation failed for {entry} ({profile}), status {}:\n{}\n{}",
                    result.status,
                    String::from_utf8_lossy(&result.stdout),
                    String::from_utf8_lossy(&result.stderr)
                )
                .into());
            }
            let bytes = fs::read(&output).map_err(|error| {
                format!(
                    "Compiler did not produce {} for {entry}: {error}",
                    output.display()
                )
            })?;
            validate_dxbc(&bytes)
                .map_err(|error| format!("Invalid DXBC shader for {entry}: {error}"))?;
            bindings.push_str(&format!(
                "const {}_{}_BYTES: &[u8] = include_bytes!(concat!(env!(\"OUT_DIR\"), \"/{filename}\"));\n",
                module.to_uppercase(), stage.to_uppercase(),
            ));
        }
    }
    fs::write(out_dir.join("shaders_bytes.rs"), bindings)?;
    Ok(())
}

fn word(bytes: &[u8], offset: usize) -> Option<u32> {
    let word = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes(word.try_into().ok()?))
}

/// Reject text/header output, truncated containers and DXIL-only shaders.
fn validate_dxbc(bytes: &[u8]) -> Result<()> {
    if !bytes.starts_with(b"DXBC")
        || bytes.len() < 32
        || word(bytes, 24).map(|len| len as usize) != Some(bytes.len())
    {
        return Err("expected a complete binary DXBC container".into());
    }
    let chunks = word(bytes, 28).ok_or("missing DXBC chunk count")? as usize;
    let header = chunks
        .checked_mul(4)
        .and_then(|size| size.checked_add(32))
        .filter(|size| *size <= bytes.len())
        .ok_or("invalid DXBC chunk table")?;
    let mut shader = false;
    for index in 0..chunks {
        let offset = word(bytes, 32 + index * 4).ok_or("missing DXBC chunk offset")? as usize;
        let data = offset
            .checked_add(8)
            .filter(|data| offset >= header && *data <= bytes.len())
            .ok_or("invalid DXBC chunk offset")?;
        let size = word(bytes, offset + 4).ok_or("missing DXBC chunk size")? as usize;
        let end = data
            .checked_add(size)
            .filter(|end| *end <= bytes.len())
            .ok_or("truncated DXBC chunk")?;
        let kind = bytes
            .get(offset..offset + 4)
            .ok_or("missing DXBC chunk type")?;
        shader |= (kind == b"SHDR" || kind == b"SHEX") && end > data;
    }
    if !shader {
        return Err("DXBC container has no tokenized SHDR/SHEX shader".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Compiler, MODULES, first_existing_path, validate_dxbc};
    use std::path::Path;

    fn container(kind: &[u8; 4]) -> Vec<u8> {
        let mut bytes = vec![0; 48];
        bytes[..4].copy_from_slice(b"DXBC");
        bytes[24..28].copy_from_slice(&48u32.to_le_bytes());
        bytes[28..32].copy_from_slice(&1u32.to_le_bytes());
        bytes[32..36].copy_from_slice(&36u32.to_le_bytes());
        bytes[36..40].copy_from_slice(kind);
        bytes[40..44].copy_from_slice(&4u32.to_le_bytes());
        bytes
    }

    #[test]
    fn binary_validation_rejects_wrong_truncated_and_dxil_only_outputs() {
        assert!(validate_dxbc(&container(b"SHDR")).is_ok());
        assert!(validate_dxbc(&container(b"SHEX")).is_ok());
        assert!(validate_dxbc(b"const BYTE shader[] = { 1, 2 };").is_err());
        assert!(validate_dxbc(&container(b"DXIL")).is_err());
        let mut bad = container(b"SHDR");
        bad.pop();
        assert!(validate_dxbc(&bad).is_err());
        let mut bad = container(b"SHDR");
        bad[32..36].copy_from_slice(&0u32.to_le_bytes());
        assert!(validate_dxbc(&bad).is_err());
    }

    #[test]
    fn both_compilers_receive_binary_output_and_stage_entry_arguments() {
        let source = Path::new("shader sources/source with space.hlsl");
        let output = Path::new("output with space.cso");
        let vkd3d =
            Compiler::Vkd3d("compiler".into()).command("quad_vertex", "vs_4_1", source, output);
        assert_eq!(
            vkd3d.get_args().collect::<Vec<_>>(),
            [
                "-x",
                "hlsl",
                "-b",
                "dxbc-tpf",
                "-p",
                "vs_4_1",
                "-e",
                "quad_vertex",
                "-o",
                "output with space.cso",
                "shader sources/source with space.hlsl"
            ]
            .map(std::ffi::OsStr::new)
        );
        assert_eq!(vkd3d.get_current_dir(), Some(Path::new("shader sources")));
        let fxc =
            Compiler::Fxc("fxc.exe".into()).command("quad_fragment", "ps_4_1", source, output);
        assert_eq!(
            fxc.get_args().collect::<Vec<_>>(),
            [
                "/T",
                "ps_4_1",
                "/E",
                "quad_fragment",
                "/Fo",
                "output with space.cso",
                "/O3",
                "shader sources/source with space.hlsl"
            ]
            .map(std::ffi::OsStr::new)
        );
        assert_eq!(MODULES.len() * 2, 16);
        assert!(MODULES.contains(&"emoji_rasterization"));
    }

    #[test]
    fn where_output_uses_one_existing_file_instead_of_the_entire_multiline_result()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("gpui-fxc-path-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let first = dir.join("first compiler.exe");
        let second = dir.join("second compiler.exe");
        std::fs::write(&first, [])?;
        std::fs::write(&second, [])?;
        let output = format!(
            "{}\r\n{}\r\n{}\r\n",
            dir.join("missing.exe").display(),
            first.display(),
            second.display()
        );
        assert_eq!(first_existing_path(&output), Some(first));
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
}
