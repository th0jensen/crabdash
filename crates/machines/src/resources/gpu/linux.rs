//! Discover DRM physical devices once; AMD sysfs and optional NVIDIA tools supply telemetry.
use super::GpuSample;
use std::collections::BTreeMap;
pub(crate) const SCRIPT: &str = r#"
printf '\n[gpus]\n'
if [ -d /sys/class/drm ] && [ -r /sys/class/drm ] && [ -x /sys/class/drm ]; then
    printf 'STATUS\tok\n'
else
    printf 'STATUS\tunavailable\n'
fi
metric() {
    if [ -r "$1" ]; then IFS= read -r value 2>/dev/null < "$1" || value=-; printf '%s' "$value"; else printf '-'; fi
}
for card in /sys/class/drm/card[0-9]*; do
    name=${card##*/}; number=${name#card}
    case "$number" in ''|*[!0-9]*) continue;; esac
    [ -e "$card/device" ] || continue
    device=$(readlink -f "$card/device" 2>/dev/null) || continue
    driver=$(readlink -f "$device/driver" 2>/dev/null) || driver=-
    printf 'DRM\t%s\t%s\t%s\t%s\t%s\t' "$device" "${device##*/}" "${driver##*/}" "$(metric "$device/vendor")" "$(metric "$device/device")"
    printf '%s\t%s\t%s\t' "$(metric "$device/gpu_busy_percent")" "$(metric "$device/mem_info_vram_used")" "$(metric "$device/mem_info_vram_total")"
    temperature=-
    for sensor in "$device"/hwmon/hwmon*/temp1_input; do
        [ -r "$sensor" ] || continue
        temperature=$(metric "$sensor"); break
    done
    printf '%s\n' "$temperature"
done
printf '\n[nvidia]\n'
if command -v nvidia-smi >/dev/null 2>&1 && command -v timeout >/dev/null 2>&1; then
    if output=$(timeout -k 1s 2s nvidia-smi --query-gpu=pci.bus_id,name,utilization.gpu,memory.used,memory.total,temperature.gpu --format=csv,noheader,nounits 2>/dev/null); then
        printf 'STATUS\tok\n%s\n' "$output"
    else
        printf 'STATUS\tunavailable\n'
    fi
else
    printf 'STATUS\tunavailable\n'
fi
"#;
fn pci_id(value: &str) -> Option<String> {
    let mut fields = value.split(':');
    let domain = u32::from_str_radix(fields.next()?, 16).ok()?;
    let bus = u8::from_str_radix(fields.next()?, 16).ok()?;
    let device = fields.next()?;
    if fields.next().is_some() || !device.contains('.') {
        return None;
    }
    Some(format!(
        "{domain:04x}:{bus:02x}:{}",
        device.to_ascii_lowercase()
    ))
}
fn finite(value: &str) -> Option<f64> {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}
fn temperature(value: &str, divisor: f64) -> Option<f64> {
    finite(value)
        .map(|value| value / divisor)
        .filter(|value| (-50.0..=250.0).contains(value))
}
pub(in crate::resources) fn parse(drm: &str, nvidia: Option<&str>) -> Option<Vec<GpuSample>> {
    let mut cards = BTreeMap::new();
    for line in drm.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 10 || fields[0] != "DRM" {
            continue;
        }
        let id = pci_id(fields[2]).unwrap_or_else(|| fields[1].to_owned());
        let vendor = match fields[4] {
            "0x1002" => "AMD",
            "0x10de" => "NVIDIA",
            "0x8086" => "Intel",
            "0x1af4" => "Virtio",
            _ => "GPU",
        };
        let driver = (fields[3] != "-").then(|| fields[3].to_owned());
        let used = fields[7].parse::<u64>().ok();
        let total = fields[8].parse::<u64>().ok().filter(|value| *value > 0);
        let used = used
            .zip(total)
            .filter(|(used, total)| used <= total)
            .map(|(used, _)| used);
        cards.entry(id.clone()).or_insert(GpuSample {
            id,
            name: format!("{vendor} {}", fields[5]),
            vendor: vendor.into(),
            driver,
            busy_percent: finite(fields[6]).filter(|value| (0.0..=100.0).contains(value)),
            memory_used_bytes: used,
            memory_total_bytes: total,
            temperature_celsius: temperature(fields[9], 1000.0),
        });
    }
    if let Some(output) = nvidia {
        for line in output.lines() {
            let fields: Vec<_> = line.split(',').map(str::trim).collect();
            if fields.len() != 6 {
                continue;
            }
            let Some(id) = pci_id(fields[0]) else {
                continue;
            };
            let bytes = |value: &str| {
                finite(value)
                    .filter(|value| *value >= 0.0 && *value <= (u64::MAX / (1024 * 1024)) as f64)
                    .map(|value| (value * 1024.0 * 1024.0) as u64)
            };
            let card = cards.entry(id.clone()).or_insert(GpuSample {
                id,
                name: fields[1].into(),
                vendor: "NVIDIA".into(),
                driver: Some("nvidia".into()),
                busy_percent: None,
                memory_used_bytes: None,
                memory_total_bytes: None,
                temperature_celsius: None,
            });
            card.name = fields[1].chars().take(256).collect();
            card.busy_percent = finite(fields[2]).filter(|value| (0.0..=100.0).contains(value));
            card.memory_total_bytes = bytes(fields[4]).filter(|value| *value > 0);
            card.memory_used_bytes = bytes(fields[3])
                .zip(card.memory_total_bytes)
                .filter(|(used, total)| used <= total)
                .map(|(used, _)| used);
            card.temperature_celsius = temperature(fields[5], 1.0);
        }
    }
    let available = !cards.is_empty()
        || drm.lines().any(|line| line == "STATUS\tok")
        || nvidia.is_some_and(|output| output.lines().any(|line| line == "STATUS\tok"));
    available.then(|| cards.into_values().collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dual_gpu_identity_merges_nvidia_and_deduplicates_drm_nodes() -> anyhow::Result<()> {
        let amd = "DRM\t/sys/devices/0000:01:00.0\t0000:01:00.0\tamdgpu\t0x1002\t0x73bf\t25\t1024\t8192\t45000";
        let nv = "DRM\t/sys/devices/0000:02:00.0\t0000:02:00.0\tnvidia\t0x10de\t0x1234\t-\t-\t-\t-";
        let values = parse(
            &format!("{amd}\n{amd}\n{nv}"),
            Some("00000000:02:00.0, NVIDIA RTX, 50, 1024, 8192, 55"),
        )
        .ok_or_else(|| anyhow::anyhow!("GPU"))?;
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].vendor, "AMD");
        assert_eq!(values[0].temperature_celsius, Some(45.0));
        assert_eq!(values[1].name, "NVIDIA RTX");
        assert_eq!(values[1].memory_used_bytes, Some(1024 * 1024 * 1024));
        assert_eq!(values[1].busy_percent, Some(50.0));
        Ok(())
    }
    #[test]
    fn missing_and_invalid_driver_telemetry_remains_unavailable() -> anyhow::Result<()> {
        let values = parse(
            "DRM\t/sys/virtual/gpu\tgpu\ti915\t0x8086\t0x1234\t200\t1024\t100\t-",
            None,
        )
        .ok_or_else(|| anyhow::anyhow!("GPU"))?;
        assert_eq!(values[0].busy_percent, None);
        assert_eq!(values[0].memory_used_bytes, None);
        assert_eq!(values[0].temperature_celsius, None);
        Ok(())
    }
    #[test]
    fn absent_provider_and_successful_empty_enumeration_are_distinct() -> anyhow::Result<()> {
        assert!(parse("STATUS\tunavailable", Some("STATUS\tunavailable")).is_none());
        assert!(parse("", None).is_none());
        assert!(
            parse("STATUS\tok", Some("STATUS\tunavailable"))
                .ok_or_else(|| anyhow::anyhow!("Expected successful DRM enumeration"))?
                .is_empty()
        );
        assert!(
            parse("STATUS\tunavailable", Some("STATUS\tok"))
                .ok_or_else(|| anyhow::anyhow!("Expected successful NVIDIA enumeration"))?
                .is_empty()
        );
        Ok(())
    }
}
