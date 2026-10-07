//! Parse Windows services without depending on localized tabular output.
use super::ServiceItem;
use anyhow::{Context as _, Result};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WindowsService {
    name: String,
    display_name: String,
    state: String,
    #[serde(default)]
    process_id: u32,
    #[serde(default)]
    start_mode: String,
    description: Option<String>,
}

pub fn parse(output: &str) -> Result<Vec<ServiceItem>> {
    let services: Vec<WindowsService> = serde_json::from_str(output.trim_start_matches('\u{feff}'))
        .context("Invalid Windows service response")?;
    Ok(services
        .into_iter()
        .map(|service| ServiceItem {
            id: service.process_id.to_string(),
            name: service.name,
            status: match service.state.as_str() {
                "Running" => "active",
                "Stopped" => "inactive",
                _ => service.state.as_str(),
            }
            .to_owned(),
            description: Some(
                service
                    .description
                    .filter(|value| !value.trim().is_empty())
                    .map_or_else(
                        || service.display_name.clone(),
                        |description| format!("{} — {description}", service.display_name),
                    ),
            ),
            load_state: Some("installed".into()),
            sub_state: Some(service.state),
            unit_file_state: Some(service.start_mode),
            error: None,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_service_names_and_normalizes_states() -> Result<()> {
        let services = parse(
            r#"[{"Name":"Spooler","DisplayName":"Print Spooler","State":"Running","ProcessId":123,"StartMode":"Auto","Description":"Print jobs"},{"Name":"DisabledService","DisplayName":"Disabled service","State":"Stopped","ProcessId":0,"StartMode":"Disabled","Description":null}]"#,
        )?;
        assert_eq!(services[0].name, "Spooler");
        assert!(services[0].is_running());
        assert!(!services[1].is_running());
        assert_eq!(services[1].unit_file_state.as_deref(), Some("Disabled"));
        assert!(parse("[]")?.is_empty());
        Ok(())
    }
}
