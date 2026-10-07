use super::ServiceItem;
use crate::output::Output;
use anyhow::{Context, Result, bail};
use std::collections::HashSet;

/// Parses the blank-line-separated property blocks emitted by systemctl show.
pub fn parse(output: &Output) -> Result<Vec<ServiceItem>> {
    let text = std::str::from_utf8(output.as_ref()).context("Invalid UTF-8 in systemctl output")?;
    let mut services = Vec::new();
    let mut names = HashSet::new();
    let mut block = PropertyBlock::default();

    for (index, line) in text.lines().enumerate() {
        if line.is_empty() {
            finish_block(&mut block, &mut services, &mut names)?;
            continue;
        }

        let (key, value) = line
            .split_once('=')
            .with_context(|| format!("Malformed systemctl property at line {}", index + 1))?;
        block
            .insert(key, value)
            .with_context(|| format!("Invalid systemctl property at line {}", index + 1))?;
    }
    finish_block(&mut block, &mut services, &mut names)?;
    Ok(services)
}

#[derive(Default)]
struct PropertyBlock<'a> {
    has_properties: bool,
    name: Option<&'a str>,
    description: Option<&'a str>,
    load_state: Option<&'a str>,
    active_state: Option<&'a str>,
    sub_state: Option<&'a str>,
    main_pid: Option<&'a str>,
    unit_file_state: Option<&'a str>,
}

impl<'a> PropertyBlock<'a> {
    fn insert(&mut self, key: &str, value: &'a str) -> Result<()> {
        if key.is_empty() {
            bail!("Empty systemctl property name");
        }
        self.has_properties = true;
        let slot = match key {
            "Id" => &mut self.name,
            "Description" => &mut self.description,
            "LoadState" => &mut self.load_state,
            "ActiveState" => &mut self.active_state,
            "SubState" => &mut self.sub_state,
            "MainPID" => &mut self.main_pid,
            "UnitFileState" => &mut self.unit_file_state,
            _ => return Ok(()),
        };
        if slot.replace(value).is_some() {
            bail!("Duplicate systemctl property {key}");
        }
        Ok(())
    }

    fn into_service(self) -> Result<ServiceItem> {
        let name = required(self.name, "Id")?;
        let load_state = required(self.load_state, "LoadState")?;
        let active_state = required(self.active_state, "ActiveState")?;
        // Unloaded/not-found units may omit MainPID entirely.
        let main_pid = self.main_pid.map_or("", |pid| pid);
        if !main_pid.is_empty() {
            main_pid
                .parse::<u32>()
                .with_context(|| format!("Invalid MainPID for systemctl unit {name}"))?;
        }

        Ok(ServiceItem {
            id: main_pid.to_string(),
            name: name.to_string(),
            status: active_state.to_string(),
            description: non_empty(self.description),
            load_state: Some(load_state.to_string()),
            sub_state: non_empty(self.sub_state),
            unit_file_state: non_empty(self.unit_file_state),
            error: None,
        })
    }
}

fn finish_block(
    block: &mut PropertyBlock<'_>,
    services: &mut Vec<ServiceItem>,
    names: &mut HashSet<String>,
) -> Result<()> {
    if !block.has_properties {
        return Ok(());
    }
    let service = std::mem::take(block).into_service()?;
    if !names.insert(service.name.clone()) {
        bail!("Duplicate systemctl unit {}", service.name);
    }
    services.push(service);
    Ok(())
}

fn required<'a>(value: Option<&'a str>, key: &str) -> Result<&'a str> {
    value
        .filter(|value| !value.is_empty())
        .with_context(|| format!("Missing or empty systemctl property {key}"))
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value.filter(|value| !value.is_empty()).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_text(text: &str) -> Result<Vec<ServiceItem>> {
        parse(&Output::from(text.to_string()))
    }

    #[test]
    fn parses_active_inactive_and_failed_services_with_file_states() {
        let services = parse_text(
            "Id=sshd.service\nDescription=OpenBSD Secure Shell server\nLoadState=loaded\nActiveState=active\nSubState=running\nMainPID=101\nUnitFileState=enabled\n\n\
             Id=cron.service\nDescription=Background tasks\nLoadState=loaded\nActiveState=inactive\nSubState=dead\nMainPID=0\nUnitFileState=disabled\n\n\
             Id=broken.service\nDescription=Broken service\nLoadState=loaded\nActiveState=failed\nSubState=failed\nMainPID=0\nUnitFileState=static\n\n\
             Id=masked.service\nDescription=Masked service\nLoadState=masked\nActiveState=inactive\nSubState=dead\nMainPID=0\nUnitFileState=masked\n",
        )
        .expect("valid property blocks");

        assert_eq!(services.len(), 4);
        assert_eq!(services[0].name, "sshd.service");
        assert_eq!(services[0].id, "101");
        assert_eq!(services[0].status_label(), "Active");
        assert_eq!(services[0].sub_state.as_deref(), Some("running"));
        assert_eq!(services[1].status_label(), "Inactive");
        assert_eq!(services[1].unit_file_state.as_deref(), Some("disabled"));
        assert_eq!(services[2].status_label(), "Failed");
        assert_eq!(services[2].unit_file_state.as_deref(), Some("static"));
        assert_eq!(services[3].load_state.as_deref(), Some("masked"));
        assert_eq!(services[3].unit_file_state.as_deref(), Some("masked"));
    }

    #[test]
    fn preserves_values_and_accepts_reordered_fields_and_eof_without_separator() {
        let description = "  Unicode blåbær\tkey=value \\ path [unprintable]  ";
        let services = parse_text(&format!(
            "UnitFileState=enabled\nUnknownProperty=ignored\nSubState=running\nDescription={description}\nMainPID=12\nActiveState=active\nLoadState=loaded\nId=example.service"
        ))
        .expect("reordered properties");

        assert_eq!(services.len(), 1);
        assert_eq!(services[0].description.as_deref(), Some(description));
        assert_eq!(services[0].name, "example.service");
    }

    #[test]
    fn preserves_escaped_and_leading_dash_unit_names() {
        let services = parse_text(
            "Id=example\\x2dname.service\nLoadState=loaded\nActiveState=inactive\n\n\
             Id=-example.service\nLoadState=loaded\nActiveState=inactive\n",
        )
        .expect("canonical unit names");

        assert_eq!(services[0].name, r"example\x2dname.service");
        assert_eq!(services[1].name, "-example.service");
    }

    #[test]
    fn accepts_empty_optional_properties_and_missing_pid_for_unloaded_units() {
        let services = parse_text(
            "Id=missing.service\nLoadState=not-found\nActiveState=inactive\nDescription=\nSubState=\nMainPID=\nUnitFileState=\n\n\
             Id=unloaded.service\nLoadState=not-found\nActiveState=inactive\n",
        )
        .expect("unloaded units");

        assert_eq!(services.len(), 2);
        for service in services {
            assert!(service.id.is_empty());
            assert!(service.description.is_none());
            assert!(service.sub_state.is_none());
            assert!(service.unit_file_state.is_none());
        }
    }

    #[test]
    fn retains_unknown_state_strings() {
        let services = parse_text(
            "Id=future.service\nLoadState=future-load\nActiveState=future-active\nSubState=future-sub\nUnitFileState=future-file\nMainPID=0\n",
        )
        .expect("open-ended systemd states");

        assert_eq!(services[0].load_state.as_deref(), Some("future-load"));
        assert_eq!(services[0].status, "future-active");
        assert_eq!(services[0].sub_state.as_deref(), Some("future-sub"));
        assert_eq!(services[0].unit_file_state.as_deref(), Some("future-file"));
    }

    #[test]
    fn empty_output_and_extra_blank_lines_are_valid() {
        assert!(parse_text("").expect("empty output").is_empty());
        assert!(parse_text("\n\n").expect("blank output").is_empty());
        assert_eq!(
            parse_text("\nId=example.service\nLoadState=loaded\nActiveState=active\n\n\n")
                .expect("extra separators")
                .len(),
            1
        );
    }

    #[test]
    fn rejects_missing_or_empty_required_properties() {
        for text in [
            "LoadState=loaded\nActiveState=active\n",
            "Id=\nLoadState=loaded\nActiveState=active\n",
            "Id=example.service\nActiveState=active\n",
            "Id=example.service\nLoadState=\nActiveState=active\n",
            "Id=example.service\nLoadState=loaded\n",
            "Id=example.service\nLoadState=loaded\nActiveState=\n",
            "UnknownProperty=ignored\n",
        ] {
            assert!(
                parse_text(text).is_err(),
                "accepted malformed block: {text}"
            );
        }
    }

    #[test]
    fn rejects_invalid_or_overflowing_pid() {
        for pid in ["invalid", "-1", "4294967296", "12 34"] {
            assert!(
                parse_text(&format!(
                    "Id=example.service\nLoadState=loaded\nActiveState=active\nMainPID={pid}\n"
                ))
                .is_err(),
                "accepted invalid PID: {pid}"
            );
        }
    }

    #[test]
    fn rejects_duplicate_known_properties_including_empty_values() {
        for duplicate in ["Id=second.service", "Description=", "MainPID=1"] {
            assert!(
                parse_text(&format!(
                    "Id=example.service\nLoadState=loaded\nActiveState=active\nDescription=\nMainPID=0\n{duplicate}\n"
                ))
                .is_err(),
                "accepted duplicate property: {duplicate}"
            );
        }
    }

    #[test]
    fn rejects_duplicate_unit_ids() {
        assert!(
            parse_text(
                "Id=example.service\nLoadState=loaded\nActiveState=active\n\n\
                 Id=example.service\nLoadState=loaded\nActiveState=inactive\n"
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_the_whole_collection_when_a_later_block_is_invalid() {
        let valid = "Id=example.service\nLoadState=loaded\nActiveState=active\nMainPID=1\n\n";
        for invalid in [
            "Id=missing-state.service\nLoadState=loaded\n",
            "Id=example.service\nLoadState=loaded\nActiveState=inactive\n",
        ] {
            assert!(
                parse_text(&format!("{valid}{invalid}")).is_err(),
                "returned a partial collection before invalid block: {invalid}"
            );
        }
    }

    #[test]
    fn rejects_malformed_property_lines_and_invalid_utf8() {
        assert!(parse_text("Id=example.service\nMalformed line\n").is_err());
        assert!(
            parse_text("Id=example.service\nLoadState=loaded\nActiveState=active\n=value\n")
                .is_err()
        );
        assert!(parse(&Output::from(vec![0xff])).is_err());
    }
}
