//! Collects the shown section from this computer on a worker thread, using the
//! same machine operations as the desktop app's local machine.
use crate::section::Section;
use machines::{
    machine::Machine,
    resources::{ResourceMonitor, ResourceUsage},
};
use services::{
    ServiceAction, Services,
    docker::{Docker, DockerAction, DockerNotInstalled},
};
use std::{
    sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel},
    thread,
    time::Duration,
};
use utils::{
    container::Container,
    disks::{Disk, Disks},
    services::ServiceItem,
};

const LOG_LINES: u32 = 200;

pub enum Request {
    Show(Section),
    Act(Action),
}

pub enum Action {
    Docker(String, DockerAction),
    Service(String, ServiceAction),
    /// A container's ID and name.
    DockerLogs(String, String),
    ServiceLogs(String),
}

pub enum Update {
    Host {
        name: String,
        platform: String,
    },
    Failed(String),
    Usage(Result<ResourceUsage, String>),
    Docker(Result<Vec<Container>, String>),
    Disks(Result<Vec<Disk>, String>),
    Services(Result<Vec<ServiceItem>, String>),
    Logs {
        title: String,
        text: Result<String, String>,
    },
    Done(Result<String, String>),
}

pub fn spawn(section: Section) -> (Sender<Request>, Receiver<Update>) {
    let (requests, inbox) = channel();
    let (outbox, updates) = channel();
    thread::spawn(move || run(section, &inbox, &outbox));
    (requests, updates)
}

fn interval(section: Section) -> Duration {
    match section {
        Section::System => Duration::from_secs(2),
        Section::Docker => Duration::from_secs(3),
        Section::Disks | Section::Services => Duration::from_secs(10),
    }
}

fn run(mut section: Section, inbox: &Receiver<Request>, outbox: &Sender<Update>) {
    let mut machine = match smol::block_on(Machine::local()) {
        Ok(machine) => machine,
        Err(error) => {
            let _ = outbox.send(Update::Failed(format!("{error:#}")));
            return;
        }
    };
    let _ = outbox.send(Update::Host {
        name: machine.display_name().to_owned(),
        platform: machine.system_info.platform_label().to_owned(),
    });
    let mut monitor = ResourceMonitor::default();
    loop {
        let update = smol::block_on(collect(&mut machine, &mut monitor, section));
        if outbox.send(update).is_err() {
            return;
        }
        match inbox.recv_timeout(interval(section)) {
            Ok(Request::Show(next)) => section = next,
            Ok(Request::Act(action)) => {
                // The refresh right after shows what the action changed.
                let update = smol::block_on(perform(&mut machine, action));
                if outbox.send(update).is_err() {
                    return;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn message(error: anyhow::Error) -> String {
    if error.is::<DockerNotInstalled>() {
        return "Docker is not installed on this machine.".into();
    }
    format!("{error:#}")
}

async fn collect(machine: &mut Machine, monitor: &mut ResourceMonitor, section: Section) -> Update {
    match section {
        Section::System => Update::Usage(
            machine
                .sample_resources()
                .await
                .and_then(|sample| monitor.update(sample))
                .map_err(message),
        ),
        Section::Docker => Update::Docker(machine.list_docker().await.map_err(message)),
        Section::Disks => Update::Disks(machine.list_disks().await.map_err(message)),
        Section::Services => Update::Services(machine.list_services().await.map_err(message)),
    }
}

async fn perform(machine: &mut Machine, action: Action) -> Update {
    match action {
        Action::Docker(id, action) => Update::Done(
            machine
                .container_action(&id, action)
                .await
                .map(|_| "Done".into())
                .map_err(message),
        ),
        Action::Service(name, action) => Update::Done(
            machine
                .service_action(&name, action)
                .await
                .map(|_| "Done".into())
                .map_err(message),
        ),
        Action::DockerLogs(id, name) => Update::Logs {
            title: format!("Logs: {name}"),
            text: machine
                .container_logs(&id, LOG_LINES)
                .await
                .map(String::from)
                .map_err(message),
        },
        Action::ServiceLogs(name) => Update::Logs {
            title: format!("Logs: {name}"),
            text: machine
                .service_logs(&name, LOG_LINES)
                .await
                .map(String::from)
                .map_err(message),
        },
    }
}

pub fn short_id(id: &str) -> &str {
    id.get(..12).unwrap_or(id)
}
