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
        let script = SCRIPT.replacen("drm=/sys/class/drm", "drm=$1", 1);
        let output = Command::new("/bin/sh")
            .args(["-eu", "-c", &script, "gpu-fixture"])
            .arg(self.0.join("drm"))
            .env("PATH", self.0.join("bin"))
            .output()?;
        ensure!(
            output.status.success(),
            "GPU script failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        ensure!(output.stderr.is_empty(), "GPU script emitted diagnostics");
        String::from_utf8(output.stdout).context("GPU script returned invalid UTF-8")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
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
