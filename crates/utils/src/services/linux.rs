use super::ServiceItem;
use crate::output::Output;
pub fn parse(output: &Output) -> Vec<ServiceItem> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(7, '\t');
            let id = parts.next()?.to_string();
            let load_state = non_empty(parts.next()?);
            let status = parts.next()?.to_string();
            let sub_state = non_empty(parts.next()?);
            let unit_file_state = non_empty(parts.next()?);
            let name = parts.next()?.to_string();
            let description = non_empty(parts.next()?);

            Some(ServiceItem {
                id,
                name,
                status,
                description,
                load_state,
                sub_state,
                unit_file_state,
                error: None,
            })
        })
        .collect()
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_all_systemd_rows_and_preserves_descriptions() {
        let output = Output::from(
            "101\tloaded\tactive\trunning\tenabled\tsshd.service\tOpenBSD Secure Shell server\n0\tloaded\tinactive\tdead\tdisabled\tcron.service\tRegular background program processing daemon\n"
                .to_string(),
        );

        let services = parse(&output);

        assert_eq!(services.len(), 2);
        assert_eq!(services[0].name, "sshd.service");
        assert_eq!(
            services[0].description.as_deref(),
            Some("OpenBSD Secure Shell server")
        );
        assert_eq!(services[1].name, "cron.service");
        assert_eq!(services[1].sub_state.as_deref(), Some("dead"));
    }

    #[test]
    fn retains_failed_systemd_service() {
        let output = Output::from(
            "0\tloaded\tfailed\tfailed\tenabled\tbroken.service\tBroken service\n".to_string(),
        );

        let services = parse(&output);

        assert_eq!(services.len(), 1);
        assert_eq!(services[0].status_label(), "Failed");
    }
}
