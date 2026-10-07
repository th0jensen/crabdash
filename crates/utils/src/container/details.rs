//! A compact, secret-free view of Docker container inspection output.
use crate::output::Output;
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainerDetails {
    pub id: String,
    pub name: String,
    pub image: Option<String>,
    pub status: Option<String>,
    /// None when Docker does not report health state.
    pub health: Option<String>,
    pub restart_policy: Option<ContainerRestartPolicy>,
    pub ports: Option<Vec<ContainerPort>>,
    pub mounts: Option<Vec<ContainerMount>>,
    pub created: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainerRestartPolicy {
    pub name: String,
    pub maximum_retry_count: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainerPort {
    pub container_port: u16,
    pub protocol: String,
    /// Some(empty) means unpublished; None means runtime mappings unavailable.
    pub bindings: Option<Vec<PortBinding>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortBinding {
    /// Kept as reported, including IPv6 and the empty wildcard address.
    pub host_ip: String,
    pub host_port: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainerMount {
    pub name: Option<String>,
    pub mount_type: Option<String>,
    pub source: Option<String>,
    pub destination: Option<String>,
    pub mode: Option<String>,
    pub read_write: Option<bool>,
}

/// Accepts exactly one container from the JSON array emitted by docker inspect.
pub fn parse(output: &Output) -> Result<ContainerDetails> {
    let containers: Vec<InspectContainer> = serde_json::from_slice(output.as_ref())
        .context("Invalid Docker container inspection JSON")?;
    if containers.len() != 1 {
        bail!(
            "Expected one Docker container inspection result, received {}",
            containers.len()
        );
    }
    let container = containers
        .into_iter()
        .next()
        .context("Missing Docker container inspection result")?;
    container
        .into_details()
        .context("Invalid Docker container inspection data")
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectContainer {
    #[serde(rename = "Id")]
    id: String,
    name: String,
    state: Option<InspectState>,
    config: Option<InspectConfig>,
    host_config: Option<InspectHostConfig>,
    network_settings: Option<InspectNetworkSettings>,
    mounts: Option<Vec<InspectMount>>,
    created: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectState {
    status: Option<String>,
    health: Option<InspectHealth>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectHealth {
    status: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectConfig {
    image: Option<String>,
    exposed_ports: Option<BTreeMap<String, serde_json::Map<String, serde_json::Value>>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectHostConfig {
    restart_policy: Option<InspectRestartPolicy>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectRestartPolicy {
    name: Option<String>,
    maximum_retry_count: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectNetworkSettings {
    ports: Option<BTreeMap<String, Option<Vec<InspectPortBinding>>>>,
}

#[derive(Deserialize)]
struct InspectPortBinding {
    #[serde(rename = "HostIp")]
    host_ip: String,
    #[serde(rename = "HostPort")]
    host_port: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectMount {
    name: Option<String>,
    #[serde(rename = "Type")]
    mount_type: Option<String>,
    source: Option<String>,
    destination: Option<String>,
    mode: Option<String>,
    #[serde(rename = "RW")]
    read_write: Option<bool>,
}

fn non_empty(value: &str, field: &str) -> Result<()> {
    if value.is_empty() {
        bail!("Docker inspection field {field} is empty");
    }
    Ok(())
}

fn available(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

impl InspectContainer {
    fn into_details(self) -> Result<ContainerDetails> {
        non_empty(&self.id, "Id")?;
        let name = self.name.strip_prefix('/').map_or(&*self.name, |name| name);
        non_empty(name, "Name")?;
        let runtime_ports = self.network_settings.and_then(|network| network.ports);
        let exposed_ports = self
            .config
            .as_ref()
            .and_then(|config| config.exposed_ports.as_ref());
        let ports_known = runtime_ports.is_some() || exposed_ports.is_some();
        let runtime_known = runtime_ports.is_some();
        let mut ports: BTreeMap<_, _> = runtime_ports
            .into_iter()
            .flatten()
            .map(|(key, bindings)| {
                (
                    key,
                    Some(bindings.into_iter().flatten().collect::<Vec<_>>()),
                )
            })
            .collect();
        if let Some(exposed_ports) = exposed_ports {
            for exposed in exposed_ports.keys() {
                ports
                    .entry(exposed.clone())
                    .or_insert_with(|| runtime_known.then(Vec::new));
            }
        }
        let mut ports = ports
            .into_iter()
            .map(|(port, bindings)| {
                let (number, protocol) = port
                    .split_once('/')
                    .with_context(|| format!("Invalid Docker container port {port}"))?;
                non_empty(protocol, "port protocol")?;
                let container_port = number
                    .parse::<u16>()
                    .with_context(|| format!("Invalid Docker container port {port}"))?;
                let bindings = bindings
                    .map(|bindings| {
                        bindings
                            .into_iter()
                            .map(|binding| {
                                let host_port =
                                    binding.host_port.parse::<u16>().with_context(|| {
                                        format!("Invalid Docker host port for {port}")
                                    })?;
                                Ok(PortBinding {
                                    host_ip: binding.host_ip,
                                    host_port,
                                })
                            })
                            .collect::<Result<Vec<_>>>()
                            .map(|mut bindings| {
                                bindings.sort_by(|left, right| {
                                    (&left.host_ip, left.host_port)
                                        .cmp(&(&right.host_ip, right.host_port))
                                });
                                bindings.dedup();
                                bindings
                            })
                    })
                    .transpose()?;
                Ok(ContainerPort {
                    container_port,
                    protocol: protocol.to_string(),
                    bindings,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        ports.sort_by(|left, right| {
            (left.container_port, &left.protocol).cmp(&(right.container_port, &right.protocol))
        });
        let mounts = self.mounts.map(|mounts| {
            mounts
                .into_iter()
                .map(|mount| ContainerMount {
                    name: available(mount.name),
                    mount_type: available(mount.mount_type),
                    source: mount.source,
                    destination: available(mount.destination),
                    mode: mount.mode,
                    read_write: mount.read_write,
                })
                .collect()
        });
        let restart_policy = self
            .host_config
            .and_then(|host| host.restart_policy)
            .and_then(|policy| {
                available(policy.name).map(|name| ContainerRestartPolicy {
                    name,
                    maximum_retry_count: policy.maximum_retry_count,
                })
            });
        let (status, health) = self.state.map_or((None, None), |state| {
            (
                available(state.status),
                state.health.and_then(|health| available(health.status)),
            )
        });
        Ok(ContainerDetails {
            id: self.id,
            name: name.to_string(),
            image: self.config.and_then(|config| available(config.image)),
            status,
            health,
            restart_policy,
            ports: ports_known.then_some(ports),
            mounts,
            created: available(self.created),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn fixture() -> Value {
        json!([{
            "Id": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "Name": "/web",
            "Created": "2026-10-07T10:00:00Z",
            "State": {"Status": "running", "Health": {"Status": "healthy"}},
            "Config": {
                "Image": "nginx:alpine",
                "ExposedPorts": {"80/tcp": {}, "443/tcp": {}, "53/udp": {}},
                "Env": ["SECRET=never expose this"],
                "Cmd": ["app", "--password", "never expose this"]
            },
            "HostConfig": {"RestartPolicy": {"Name": "on-failure", "MaximumRetryCount": 3}},
            "NetworkSettings": {"Ports": {
                "80/tcp": [{"HostIp": "0.0.0.0", "HostPort": "8080"}, {"HostIp": "::", "HostPort": "8080"}],
                "443/tcp": null
            }},
            "Mounts": [
                {"Type": "bind", "Source": "/srv/web", "Destination": "/app", "Mode": "ro,z", "RW": false},
                {"Type": "volume", "Name": "data", "Source": "/var/lib/docker/volumes/data/_data", "Destination": "/data", "Mode": "", "RW": true},
                {"Type": "tmpfs", "Source": "", "Destination": "/tmp", "Mode": "", "RW": true}
            ]
        }])
    }

    fn parse_value(value: &Value) -> Result<ContainerDetails> {
        parse(&Output::from(serde_json::to_vec(value)?))
    }

    #[test]
    fn parses_identity_health_restart_ports_and_mounts_without_secrets() -> Result<()> {
        let details = parse_value(&fixture())?;
        assert_eq!(details.id.len(), 64);
        assert_eq!(details.name, "web");
        assert_eq!(details.image.as_deref(), Some("nginx:alpine"));
        assert_eq!(details.status.as_deref(), Some("running"));
        assert_eq!(details.health.as_deref(), Some("healthy"));
        let restart = details.restart_policy.as_ref().context("restart policy")?;
        assert_eq!(restart.name, "on-failure");
        assert_eq!(restart.maximum_retry_count, Some(3));
        let ports = details.ports.as_ref().context("ports")?;
        assert_eq!(ports.len(), 3);
        assert_eq!(ports[0].container_port, 53);
        assert_eq!(ports[0].protocol, "udp");
        assert_eq!(ports[0].bindings, Some(vec![]));
        let bindings = ports[1].bindings.as_ref().context("bindings")?;
        assert_eq!(bindings[0].host_ip, "0.0.0.0");
        assert_eq!(bindings[1].host_ip, "::");
        assert_eq!(bindings[1].host_port, 8080);
        assert_eq!(ports[2].bindings, Some(vec![]));
        let mounts = details.mounts.as_ref().context("mounts")?;
        assert_eq!(mounts[0].mode.as_deref(), Some("ro,z"));
        assert_eq!(mounts[0].read_write, Some(false));
        assert_eq!(mounts[1].mount_type.as_deref(), Some("volume"));
        assert_eq!(mounts[1].name.as_deref(), Some("data"));
        assert_eq!(mounts[1].read_write, Some(true));
        assert_eq!(mounts[2].source.as_deref(), Some(""));
        assert_eq!(details.created.as_deref(), Some("2026-10-07T10:00:00Z"));
        assert!(!format!("{details:?}").contains("never expose this"));
        Ok(())
    }

    #[test]
    fn null_runtime_ports_preserve_exposures_with_unknown_bindings() -> Result<()> {
        let mut value = fixture();
        value[0]["State"] = json!({"Status": "exited"});
        value[0]["HostConfig"]["RestartPolicy"] = json!({"Name": "no", "MaximumRetryCount": 0});
        value[0]["NetworkSettings"]["Ports"] = Value::Null;
        value[0]["Mounts"] = Value::Null;
        let details = parse_value(&value)?;
        assert_eq!(details.status.as_deref(), Some("exited"));
        assert!(details.health.is_none());
        assert_eq!(details.restart_policy.context("policy")?.name, "no");
        let ports = details.ports.context("configured exposures")?;
        assert_eq!(ports.len(), 3);
        assert!(ports.iter().all(|port| port.bindings.is_none()));
        assert!(details.mounts.is_none());
        Ok(())
    }

    #[test]
    fn missing_and_null_sections_remain_unavailable() -> Result<()> {
        for field in ["State", "Config", "HostConfig", "NetworkSettings", "Mounts"] {
            for set_null in [false, true] {
                let mut value = fixture();
                if set_null {
                    value[0][field] = Value::Null;
                } else {
                    value[0]
                        .as_object_mut()
                        .context("fixture object")?
                        .remove(field);
                }
                let details = parse_value(&value)?;
                match field {
                    "State" => assert!(details.status.is_none() && details.health.is_none()),
                    "Config" => assert!(details.image.is_none()),
                    "HostConfig" => assert!(details.restart_policy.is_none()),
                    "NetworkSettings" => assert!(
                        details
                            .ports
                            .context("exposed ports")?
                            .iter()
                            .all(|port| port.bindings.is_none())
                    ),
                    "Mounts" => assert!(details.mounts.is_none()),
                    _ => unreachable!(),
                }
            }
        }
        let details = parse_value(&json!([{"Id": "id", "Name": "/name"}]))?;
        assert!(details.image.is_none());
        assert!(details.status.is_none());
        assert!(details.ports.is_none());
        assert!(details.mounts.is_none());
        Ok(())
    }

    #[test]
    fn empty_known_collections_are_distinct_from_unavailable_collections() -> Result<()> {
        let mut value = fixture();
        value[0]["Config"]["ExposedPorts"] = json!({});
        value[0]["NetworkSettings"]["Ports"] = json!({});
        value[0]["Mounts"] = json!([]);
        let details = parse_value(&value)?;
        assert_eq!(details.ports, Some(vec![]));
        assert_eq!(details.mounts, Some(vec![]));
        Ok(())
    }

    #[test]
    fn binding_order_is_stable_and_distinct_addresses_are_retained() -> Result<()> {
        let mut value = fixture();
        value[0]["NetworkSettings"]["Ports"]["80/tcp"] = json!([
            {"HostIp": "::", "HostPort": "8080"},
            {"HostIp": "0.0.0.0", "HostPort": "8081"},
            {"HostIp": "::1", "HostPort": "8080"},
            {"HostIp": "0.0.0.0", "HostPort": "8080"},
            {"HostIp": "::", "HostPort": "8080"}
        ]);
        let details = parse_value(&value)?;
        let ports = details.ports.context("ports")?;
        let bindings = ports[1].bindings.as_ref().context("bindings")?;
        assert_eq!(bindings.len(), 4);
        assert_eq!(
            (&*bindings[0].host_ip, bindings[0].host_port),
            ("0.0.0.0", 8080)
        );
        assert_eq!(
            (&*bindings[1].host_ip, bindings[1].host_port),
            ("0.0.0.0", 8081)
        );
        assert_eq!(bindings[2].host_ip, "::");
        assert_eq!(bindings[3].host_ip, "::1");
        Ok(())
    }

    #[test]
    fn rejects_malformed_empty_multiple_or_non_container_results() -> Result<()> {
        for raw in [
            "",
            "not json",
            "[]",
            "{}",
            "[null]",
            "[{}]",
            "[{\"Id\": \"image-id\", \"Config\": {\"Image\": \"image\"}}]",
        ] {
            assert!(
                parse(&Output::from(raw.to_string())).is_err(),
                "accepted {raw}"
            );
        }
        let value = fixture();
        assert!(parse_value(&json!([value[0], value[0]])).is_err());
        assert!(parse(&Output::from(vec![0xff])).is_err());
        Ok(())
    }

    #[test]
    fn rejects_missing_empty_and_wrongly_typed_identity() -> Result<()> {
        for field in ["Id", "Name"] {
            let mut value = fixture();
            value[0]
                .as_object_mut()
                .context("fixture object")?
                .remove(field);
            assert!(parse_value(&value).is_err());
            for invalid in [json!(""), Value::Null, json!(123), json!([])] {
                value[0][field] = invalid;
                assert!(parse_value(&value).is_err());
            }
        }
        let mut value = fixture();
        value[0]["Name"] = json!("/");
        assert!(parse_value(&value).is_err());
        value[0]["Mounts"][0]["RW"] = json!("false");
        assert!(parse_value(&value).is_err());
        Ok(())
    }

    #[test]
    fn rejects_invalid_port_numbers_and_binding_types() -> Result<()> {
        for ports in [
            json!({"not-a-port": null}),
            json!({"65536/tcp": null}),
            json!({"80/": null}),
            json!({"80/tcp": [{"HostIp": "::", "HostPort": "invalid"}]}),
            json!({"80/tcp": [{"HostIp": "::", "HostPort": "65536"}]}),
            json!({"80/tcp": [{"HostIp": "::", "HostPort": 8080}]}),
            json!({"80/tcp": {"HostIp": "::", "HostPort": "8080"}}),
        ] {
            let mut value = fixture();
            value[0]["NetworkSettings"]["Ports"] = ports;
            assert!(parse_value(&value).is_err());
        }
        Ok(())
    }
}
