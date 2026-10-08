//! Execute the production shell collector against changing Linux DRM inventories.
use super::{SCRIPT, parse};
use anyhow::{Context as _, Result, ensure};
use std::{
    fs,
    os::unix::fs::{PermissionsExt as _, symlink},
    path::PathBuf,
    process::Command,
};
use uuid::Uuid;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Result<Self> {
        // Spaces exercise argv passing and quoting without shell interpolation.
        let root = std::env::temp_dir().join(format!("crabdash gpu test {}", Uuid::new_v4()));
        fs::create_dir(&root)?;
        let fixture = Self(root);
        fs::create_dir(fixture.0.join("drm"))?;
        let bin = fixture.0.join("bin");
        fs::create_dir(&bin)?;
        symlink("/usr/bin/readlink", bin.join("readlink"))?;
        symlink("/usr/bin/timeout", bin.join("timeout"))?;
        // Keep installed host NVIDIA tools out of this sysfs-only fixture.
        let nvidia = bin.join("nvidia-smi");
        fs::write(&nvidia, "#!/bin/sh\nexit 1\n")?;
        fs::set_permissions(&nvidia, fs::Permissions::from_mode(0o700))?;
        Ok(fixture)
    }

    fn device(&self, card: &str, device: &str, driver: &str) -> Result<PathBuf> {
        let path = self.0.join("devices").join(device);
        fs::create_dir_all(&path)?;
        let driver_path = self.0.join("drivers").join(driver);
        fs::create_dir_all(&driver_path)?;
        symlink(&driver_path, path.join("driver"))?;
        self.card(card, &path)?;
        Ok(path)
    }

    fn card(&self, card: &str, device: &std::path::Path) -> Result<()> {
        let path = self.0.join("drm").join(card);
        fs::create_dir(&path)?;
        symlink(device, path.join("device"))?;
        Ok(())
    }

    fn sample(&self) -> Result<String> {
        let script = SCRIPT.replacen("drm=/sys/class/drm", "drm=$1", 1).replacen(
            "wsl_nvidia=/usr/lib/wsl/lib/nvidia-smi",
            "wsl_nvidia=$2",
            1,
        );
        let output = Command::new("/bin/sh")
            .args(["-eu", "-c", &script, "gpu-fixture"])
            .arg(self.0.join("drm"))
            .arg(self.0.join("wsl 'nvidia-smi'"))
            .env("PATH", self.0.join("bin"))
            .env("GPU_CALLS", self.0.join("calls"))
            .output()?;
        ensure!(
            output.status.success(),
            "GPU script failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        ensure!(output.stderr.is_empty(), "GPU script emitted diagnostics");
        String::from_utf8(output.stdout).context("GPU script returned invalid UTF-8")
    }

    fn tool(&self, projected: bool, script: &str) -> Result<()> {
        let path = if projected {
            self.0.join("wsl 'nvidia-smi'")
        } else {
            self.0.join("bin/nvidia-smi")
        };
        fs::write(&path, script)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
        Ok(())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn path_provider_precedes_projected_wsl_tool_and_keeps_optional_na_metrics() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.tool(false, "#!/bin/sh\nprintf 'PATH\\n' >> \"$GPU_CALLS\"\nprintf '00000000:01:00.0, Path GPU, N/A, N/A, 8192, N/A\\n'\n")?;
    fixture.tool(true, "#!/bin/sh\nprintf 'WSL\\n' >> \"$GPU_CALLS\"\nprintf '00000000:02:00.0, WSL GPU, 10, 10, 8192, 40\\n'\n")?;
    let output = fixture.sample()?;
    assert_eq!(fs::read_to_string(fixture.0.join("calls"))?, "PATH\n");
    let cards = parse(
        "STATUS\tunavailable",
        Some(crate::resources::section(&output, "nvidia")?),
    )
    .context("PATH GPU")?;
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].name, "Path GPU");
    assert_eq!(cards[0].busy_percent, None);
    assert_eq!(cards[0].memory_used_bytes, None);
    assert_eq!(cards[0].temperature_celsius, None);
    assert_eq!(cards[0].memory_total_bytes, Some(8192 * 1024 * 1024));
    Ok(())
}

#[test]
fn projected_wsl_path_is_quoted_and_unsupported_telemetry_retains_inventory() -> Result<()> {
    let fixture = Fixture::new()?;
    fs::remove_file(fixture.0.join("bin/nvidia-smi"))?;
    fixture.tool(true, r#"#!/bin/sh
case "$1" in
    --query-gpu=pci.bus_id,name,*) printf 'full\n' >> "$GPU_CALLS"; exit 1;;
    --query-gpu=pci.bus_id,name) printf 'inventory\n' >> "$GPU_CALLS"; printf '00000000:01:00.0, NVIDIA 雪\n00000000:02:00.0, NVIDIA 雪\nN/A, Missing identity\ninvalid, Malformed identity\n';;
    *) exit 2;;
esac
"#)?;
    let output = fixture.sample()?;
    assert_eq!(
        fs::read_to_string(fixture.0.join("calls"))?,
        "full\ninventory\n"
    );
    let nvidia = crate::resources::section(&output, "nvidia")?;
    let cards = parse("STATUS\tunavailable", Some(nvidia)).context("WSL inventory")?;
    assert_eq!(cards.len(), 2);
    assert_eq!(cards[0].id, "0000:01:00.0");
    assert_eq!(cards[0].name, "NVIDIA 雪");
    assert_eq!(cards[1].name, "NVIDIA 雪");
    assert_ne!(cards[0].id, cards[1].id);
    assert!(cards.iter().all(|card| card.busy_percent.is_none()
        && card.memory_used_bytes.is_none()
        && card.memory_total_bytes.is_none()
        && card.temperature_celsius.is_none()));
    let merged = parse("DRM\t/sys/devices/0000:01:00.0\t0000:01:00.0\tnvidia\t0x10de\t0x1234\t13\t1024\t4096\t45000", Some(nvidia)).context("Merged inventory")?;
    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].name, "NVIDIA 雪");
    assert_eq!(merged[0].busy_percent, Some(13.0));
    assert_eq!(merged[0].memory_used_bytes, Some(1024));
    Ok(())
}

#[test]
fn absent_nonexecutable_or_failing_projected_provider_is_unavailable() -> Result<()> {
    let fixture = Fixture::new()?;
    fs::remove_file(fixture.0.join("bin/nvidia-smi"))?;
    for mode in [None, Some(0o600), Some(0o700)] {
        if let Some(mode) = mode {
            fixture.tool(true, "#!/bin/sh\nexit 1\n")?;
            fs::set_permissions(
                fixture.0.join("wsl 'nvidia-smi'"),
                fs::Permissions::from_mode(mode),
            )?;
        }
        let output = fixture.sample()?;
        assert!(
            parse(
                "STATUS\tunavailable",
                Some(crate::resources::section(&output, "nvidia")?)
            )
            .is_none()
        );
    }
    Ok(())
}

#[test]
fn telemetry_and_inventory_attempts_share_one_deadline() -> Result<()> {
    let fixture = Fixture::new()?;
    fs::remove_file(fixture.0.join("bin/nvidia-smi"))?;
    fixture.tool(true, r#"#!/bin/sh
case "$1" in
    --query-gpu=pci.bus_id,name,*) printf 'full\n' >> "$GPU_CALLS"; /bin/sleep 1.25; exit 1;;
    --query-gpu=pci.bus_id,name) printf 'inventory\n' >> "$GPU_CALLS"; /bin/sleep 1.25; printf '00000000:01:00.0, Too late\n';;
    *) exit 2;;
esac
"#)?;
    let started = std::time::Instant::now();
    let output = fixture.sample()?;
    // Two independent two-second queries would return the inventory at 2.5s.
    // The shared deadline terminates the second attempt without publishing it.
    assert_eq!(
        fs::read_to_string(fixture.0.join("calls"))?,
        "full\ninventory\n"
    );
    assert!(
        parse(
            "STATUS\tunavailable",
            Some(crate::resources::section(&output, "nvidia")?)
        )
        .is_none()
    );
    ensure!(
        started.elapsed() < std::time::Duration::from_secs(4),
        "NVIDIA queries exceeded their bounded timeout"
    );
    Ok(())
}

#[test]
fn shell_collects_each_physical_gpu_once_and_tracks_hotplug() -> Result<()> {
    let fixture = Fixture::new()?;
    let amd = fixture.device("card0", "0000:01:00.0", "amdgpu")?;
    fixture.card("card1", &amd)?;
    fixture.card("card0-DP-1", &amd)?;
    let intel = fixture.device("card10", "0000:02:00.0", "i915")?;
    fixture.card("card40", &fixture.0.join("removed-device"))?;
    for (file, value) in [
        ("vendor", "0x1002\n"),
        ("device", "0x73bf\n"),
        ("gpu_busy_percent", "25\n"),
        ("mem_info_vram_used", "1024\n"),
        ("mem_info_vram_total", "8192\n"),
    ] {
        fs::write(amd.join(file), value)?;
    }
    let sensor = amd.join("hwmon/hwmon0");
    fs::create_dir_all(&sensor)?;
    fs::write(sensor.join("temp1_input"), "45000\n")?;
    fs::write(intel.join("vendor"), "0x8086\n")?;
    fs::write(intel.join("device"), "0x46a8\n")?;
    fs::write(intel.join("gpu_busy_percent"), "")?;
    // POSIX read fails without a newline, as it does for disappearing sysfs data.
    fs::write(intel.join("mem_info_vram_used"), "12")?;
    let output = fixture.sample()?;
    let drm = crate::resources::section(&output, "gpus")?;
    let rows: Vec<Vec<_>> = drm
        .lines()
        .filter(|line| line.starts_with("DRM\t"))
        .map(|line| line.split('\t').collect())
        .collect();
    // Inspect wire rows too: parser deduplication must not mask repeated reads.
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|fields| fields.len() == 10));
    assert_eq!(rows[0][2], "0000:01:00.0");
    assert_eq!(rows[0][3], "amdgpu");
    assert_eq!(
        &rows[0][4..],
        &["0x1002", "0x73bf", "25", "1024", "8192", "45000"]
    );
    assert_eq!(rows[1][2], "0000:02:00.0");
    assert_eq!(rows[1][3], "i915");
    assert_eq!(&rows[1][4..], &["0x8086", "0x46a8", "-", "-", "-", "-"]);
    let cards = parse(drm, None).context("DRM inventory unavailable")?;
    assert_eq!(cards.len(), 2);
    assert_eq!(cards[0].temperature_celsius, Some(45.0));
    assert_eq!(cards[0].busy_percent, Some(25.0));
    assert_eq!(cards[1].busy_percent, None);
    assert_eq!(cards[1].memory_used_bytes, None);
    assert_eq!(cards[1].temperature_celsius, None);

    fs::remove_dir_all(fixture.0.join("drm/card10"))?;
    fs::remove_file(amd.join("driver"))?;
    fs::remove_file(sensor.join("temp1_input"))?;
    fs::write(amd.join("gpu_busy_percent"), "91\n")?;
    let next = fixture.sample()?;
    let cards = parse(crate::resources::section(&next, "gpus")?, None)
        .context("Updated DRM inventory unavailable")?;
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].busy_percent, Some(91.0));
    assert_eq!(cards[0].driver, None);
    assert_eq!(cards[0].temperature_celsius, None);
    fs::remove_dir_all(&amd)?;
    let empty = fixture.sample()?;
    assert!(
        parse(crate::resources::section(&empty, "gpus")?, None)
            .context("Empty DRM inventory unavailable")?
            .is_empty()
    );
    fs::remove_dir_all(fixture.0.join("drm"))?;
    let unavailable = fixture.sample()?;
    assert!(parse(crate::resources::section(&unavailable, "gpus")?, None).is_none());
    Ok(())
}
