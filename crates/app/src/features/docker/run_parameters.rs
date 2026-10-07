//! Validated Docker run parameters, independent of the UI and machine platform.
use services::docker::{NetworkMode, RestartPolicy};
use utils::args::Args;

#[derive(Default)]
pub(super) struct Parameters {
    pub image: String,
    pub name: String,
    pub hostname: String,
    pub interactive: bool,
    pub remove: bool,
    pub restart: RestartPolicy,
    pub network: NetworkMode,
    pub ports: Vec<String>,
    pub volumes: Vec<String>,
    pub env_vars: Vec<String>,
    pub memory: String,
    pub cpus: String,
    pub user: String,
    pub working_dir: String,
    pub entrypoint: String,
    pub command: String,
}

impl Parameters {
    pub fn build_args(&self) -> Result<Args, String> {
        let image = self.image.trim();
        if image.is_empty() {
            return Err("Image is required.".into());
        }
        if image.starts_with('-') || image.chars().any(char::is_whitespace) {
            return Err("Enter an image reference without spaces or command flags.".into());
        }
        if self.remove && self.restart != RestartPolicy::No {
            return Err("Remove on exit cannot be combined with a restart policy.".into());
        }
        if self.network != NetworkMode::Bridge && self.ports.iter().any(|p| !p.trim().is_empty()) {
            return Err("Published ports require Bridge networking.".into());
        }
        let cpus = self.cpus.trim();
        if !cpus.is_empty() && !cpus.parse::<f64>().is_ok_and(|v| v.is_finite() && v > 0.0) {
            return Err("CPUs must be a positive number, such as 0.5 or 2.".into());
        }
        let memory = self.memory.trim();
        if !memory.is_empty() {
            let digits = memory
                .strip_suffix(['b', 'B', 'k', 'K', 'm', 'M', 'g', 'G'])
                .unwrap_or(memory);
            if !digits.bytes().all(|c| c.is_ascii_digit())
                || !digits.parse::<u64>().is_ok_and(|n| n > 0)
            {
                return Err("Memory must be a positive size, such as 512m or 2g.".into());
            }
        }
        let command = shell_words::split(&self.command)
            .map_err(|_| "Command has an unmatched quote or incomplete escape.".to_string())?;
        let mut args = Args::new();
        // The app manages containers asynchronously; it does not attach a TTY.
        args.push("-d");
        if self.interactive {
            args.push("-i");
        }
        if self.remove {
            args.push("--rm");
        }
        if self.restart != RestartPolicy::No {
            args.push(format!("--restart={}", self.restart.flag_value()));
        }
        if let Some(network) = self.network.flag_value() {
            args.push(format!("--network={network}"));
        }
        for (flag, value) in [
            ("--name", &self.name),
            ("--hostname", &self.hostname),
            ("--memory", &self.memory),
            ("--cpus", &self.cpus),
            ("--user", &self.user),
            ("--workdir", &self.working_dir),
            ("--entrypoint", &self.entrypoint),
        ] {
            let value = value.trim();
            if !value.is_empty() {
                args.push(format!("{flag}={value}"));
            }
        }
        for (flag, values) in [
            ("-p", &self.ports),
            ("-v", &self.volumes),
            ("-e", &self.env_vars),
        ] {
            for value in values {
                // Keep environment values verbatim, including spaces and empty values.
                let value = if flag == "-e" {
                    value.as_str()
                } else {
                    value.trim()
                };
                if value.trim().is_empty() {
                    continue;
                }
                if flag == "-e" {
                    let key = value.split('=').next().unwrap_or_default();
                    if key.is_empty() || key.chars().any(|c| c.is_whitespace() || c == '\0') {
                        return Err(
                            "Environment entries need a variable name, such as KEY=value.".into(),
                        );
                    }
                }
                args.push(flag);
                args.push(value);
            }
        }
        args.push(image);
        for argument in command {
            args.push(argument);
        }
        if args.iter().any(|a| a.contains('\0')) {
            return Err("Parameters cannot contain a null character.".into());
        }
        Ok(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_commands_and_mapping_values_keep_argument_boundaries() {
        let config = Parameters {
            image: " alpine:latest ".into(),
            command: r#"sh -c 'printf "%s" "$GREETING"' """#.into(),
            env_vars: vec!["GREETING=hello world ".into(), "EMPTY=".into()],
            volumes: vec!["/a path:/data:ro".into()],
            ports: vec!["127.0.0.1:8080:80/tcp".into()],
            ..Default::default()
        };
        assert_eq!(
            config.build_args().unwrap().as_str_slice(),
            vec![
                "-d",
                "-p",
                "127.0.0.1:8080:80/tcp",
                "-v",
                "/a path:/data:ro",
                "-e",
                "GREETING=hello world ",
                "-e",
                "EMPTY=",
                "alpine:latest",
                "sh",
                "-c",
                "printf \"%s\" \"$GREETING\"",
                "",
            ]
        );
    }

    #[test]
    fn rejects_invalid_or_conflicting_parameters() {
        let mut config = Parameters {
            image: "alpine".into(),
            ..Default::default()
        };
        for image in ["", "   ", "--privileged", "alpine echo"] {
            config.image = image.into();
            assert!(config.build_args().is_err());
        }
        config.image = "alpine".into();
        config.command = "'unterminated".into();
        assert!(config.build_args().is_err());
        config.command.clear();
        for cpus in ["-1", "0", "NaN", "inf", "one"] {
            config.cpus = cpus.into();
            assert!(config.build_args().is_err());
        }
        config.cpus = "0.5".into();
        config.memory = "512m".into();
        assert!(config.build_args().is_ok());
        config.memory = "12wat".into();
        assert!(config.build_args().is_err());
        config.memory.clear();
        config.remove = true;
        config.restart = RestartPolicy::Always;
        assert!(config.build_args().is_err());
        config.restart = RestartPolicy::No;
        config.network = NetworkMode::Host;
        config.ports = vec!["80:80".into()];
        assert!(config.build_args().is_err());
    }

    #[test]
    fn background_runs_do_not_request_an_unavailable_tty() {
        let config = Parameters {
            image: "alpine".into(),
            interactive: true,
            ..Default::default()
        };
        assert_eq!(
            config.build_args().unwrap().as_str_slice(),
            ["-d", "-i", "alpine"]
        );
    }
}
