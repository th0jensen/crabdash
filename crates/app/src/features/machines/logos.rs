//! Bundled distro artwork and sidebar presentation, shared by Linux and macOS.
use crate::components::common::{lucide_icon, machine_icon};
use gpui::{prelude::*, *};
use machines::machine::{Machine, MachineKind};
use std::borrow::Cow;

struct Logo {
    slug: &'static str,
    label: &'static str,
    path: &'static str,
    bytes: &'static [u8],
    color: u32,
}

macro_rules! logo {
    ($slug:literal, $label:literal, $color:literal) => {
        Logo {
            slug: $slug,
            label: $label,
            path: concat!("machines/logos/", $slug, ".svg"),
            bytes: include_bytes!(concat!("../../../assets/machine-logos/", $slug, ".svg")),
            color: $color,
        }
    };
}

const LOGOS: &[Logo] = &[
    logo!("almalinux", "AlmaLinux", 0x91C9EF),
    logo!("alpine", "Alpine", 0x71B5DF),
    logo!("apple", "macOS", 0xD9DEE7),
    logo!("archlinux", "Arch Linux", 0x6DC2ED),
    logo!("cachyos", "CachyOS", 0x64D8CD),
    logo!("centos", "CentOS", 0xC9A9E5),
    logo!("debian", "Debian", 0xEF799C),
    logo!("endeavour", "EndeavourOS", 0xC59CF0),
    logo!("fedora", "Fedora", 0x8AB4F1),
    logo!("gentoo", "Gentoo", 0xBDB1EA),
    logo!("linuxmint", "Linux Mint", 0xA6D783),
    logo!("manjaro", "Manjaro", 0x70D9B0),
    logo!("nixos", "NixOS", 0x8ABAE9),
    logo!("opensuse", "openSUSE", 0x8CCA66),
    logo!("pop-os", "Pop!_OS", 0x83D6E0),
    logo!("redhat", "Red Hat", 0xEF7878),
    logo!("rocky-linux", "Rocky Linux", 0x70D2AB),
    logo!("tux", "Linux", 0xD8E0E9),
    logo!("ubuntu", "Ubuntu", 0xF69C75),
    logo!("zorin", "Zorin OS", 0x80BCF5),
];

fn distro_logo(id: &str) -> Option<&'static Logo> {
    let slug = match id {
        "rhel" => "redhat",
        "rocky" => "rocky-linux",
        "endeavouros" => "endeavour",
        "pop" => "pop-os",
        "opensuse-leap" | "opensuse-tumbleweed" | "sles" | "sled" => "opensuse",
        id => id,
    };
    LOGOS
        .iter()
        .find(|logo| logo.slug == slug && logo.slug != "apple" && logo.slug != "tux")
}

fn machine_logo(machine: &Machine) -> Option<&'static Logo> {
    match machine.kind {
        MachineKind::MacOS => LOGOS.iter().find(|logo| logo.slug == "apple"),
        MachineKind::Linux => machine
            .system_info
            .distribution
            .as_ref()
            .and_then(|distribution| distro_logo(&distribution.id))
            .or_else(|| LOGOS.iter().find(|logo| logo.slug == "tux")),
        MachineKind::Windows | MachineKind::Unknown => None,
    }
}

pub(super) fn platform_label(machine: &Machine) -> &str {
    if let Some(logo) = machine_logo(machine)
        && logo.slug != "tux"
    {
        logo.label
    } else {
        machine.system_info.platform_label()
    }
}

pub(super) fn render(machine: &Machine) -> AnyElement {
    if let Some(logo) = machine_logo(machine) {
        svg()
            .path(logo.path)
            .size(rems(20.0 / 16.0))
            .text_color(rgb(logo.color))
            .into_any_element()
    } else {
        lucide_icon(machine_icon(machine.kind), 17.0).into_any_element()
    }
}

// Embedded paths are independent of the build tree, installation directory,
// network, and desktop icon theme. This source is installed once by runtime.rs.
pub(crate) struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == crate::features::docker::brand::PATH {
            return Ok(Some(Cow::Borrowed(crate::features::docker::brand::BYTES)));
        }
        Ok(LOGOS
            .iter()
            .find(|logo| logo.path == path)
            .map(|logo| Cow::Borrowed(logo.bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut assets: Vec<_> = LOGOS
            .iter()
            .filter(|logo| logo.path.starts_with(path))
            .map(|logo| SharedString::from(logo.path))
            .collect();
        if crate::features::docker::brand::PATH.starts_with(path) {
            assets.push(crate::features::docker::brand::PATH.into());
        }
        Ok(assets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use machines::machine::LinuxDistribution;
    use std::prelude::v1::test;

    #[test]
    fn logo_uses_selected_machine_vendor_identity_instead_of_host_or_kernel() {
        let mut machine = Machine {
            kind: MachineKind::Linux,
            ..Default::default()
        };
        machine.system_info.machine_name = "fedora".into();
        machine.system_info.os_version = "Linux 7.2-arch".into();
        machine.system_info.distribution = Some(LinuxDistribution {
            id: "cachyos".into(),
            name: "CachyOS".into(),
            pretty_name: "CachyOS Linux".into(),
        });
        assert_eq!(machine_logo(&machine).unwrap().slug, "cachyos");
        assert_eq!(platform_label(&machine), "CachyOS");
        machine.kind = MachineKind::MacOS;
        assert_eq!(machine_logo(&machine).unwrap().slug, "apple");
    }

    #[test]
    fn unknown_linux_distributions_keep_their_name_and_use_tux() {
        let mut machine = Machine {
            kind: MachineKind::Linux,
            ..Default::default()
        };
        machine.system_info.distribution = Some(LinuxDistribution {
            id: "new-distro".into(),
            name: "New Distro".into(),
            pretty_name: "New Distro 1".into(),
        });
        assert_eq!(machine_logo(&machine).unwrap().slug, "tux");
        assert_eq!(platform_label(&machine), "New Distro");
        machine.system_info.distribution = None;
        assert_eq!(machine_logo(&machine).unwrap().slug, "tux");
    }
}
