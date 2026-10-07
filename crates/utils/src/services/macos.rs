use super::ServiceItem;
use crate::output::Output;
pub fn parse(output: &Output) -> Vec<ServiceItem> {
    output
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.split('\t');
            Some(ServiceItem {
                id: parts.next()?.to_string(),
                status: parts.next()?.to_string(),
                name: parts.next()?.to_string(),
                description: None,
                load_state: None,
                sub_state: None,
                unit_file_state: None,
                error: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launchctl_parser_skips_header_only() {
        let output = Output::from("PID\tStatus\tLabel\n123\t0\tcom.example.service\n".to_string());

        let services = parse(&output);

        assert_eq!(services.len(), 1);
        assert_eq!(services[0].name, "com.example.service");
        assert!(services[0].description.is_none());
    }
}
