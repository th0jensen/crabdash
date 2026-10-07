use gpui::*;
use services::docker::{NetworkMode, RestartPolicy};
use utils::args::Args;

use crate::app::Crabdash;
use crate::components::text_field::TextField;

#[derive(Clone)]
pub struct DockerRunConfig {
    pub image: Entity<TextField>,
    pub name: Entity<TextField>,
    pub hostname: Entity<TextField>,
    pub detach: bool,
    pub interactive: bool,
    pub remove: bool,
    pub restart: RestartPolicy,
    pub network: NetworkMode,
    pub ports: Vec<Entity<TextField>>,
    pub volumes: Vec<Entity<TextField>>,
    pub env_vars: Vec<Entity<TextField>>,
    pub memory: Entity<TextField>,
    pub cpus: Entity<TextField>,
    pub user: Entity<TextField>,
    pub working_dir: Entity<TextField>,
    pub entrypoint: Entity<TextField>,
    pub command: Entity<TextField>,
}

impl DockerRunConfig {
    pub fn new(cx: &mut Context<Crabdash>) -> Self {
        Self {
            image: cx.new(|cx| TextField::new("", "nginx:latest", 50, cx)),
            name: cx.new(|cx| TextField::new("", "my-container", 51, cx)),
            hostname: cx.new(|cx| TextField::new("", "my-host", 52, cx)),
            detach: true,
            interactive: false,
            remove: false,
            restart: RestartPolicy::default(),
            network: NetworkMode::default(),
            ports: vec![],
            volumes: vec![],
            env_vars: vec![],
            memory: cx.new(|cx| TextField::new("", "512m", 53, cx)),
            cpus: cx.new(|cx| TextField::new("", "1.0", 54, cx)),
            user: cx.new(|cx| TextField::new("", "1000:1000", 55, cx)),
            working_dir: cx.new(|cx| TextField::new("", "/app", 56, cx)),
            entrypoint: cx.new(|cx| TextField::new("", "/bin/sh", 57, cx)),
            command: cx.new(|cx| TextField::new("", "", 58, cx)),
        }
    }

    pub fn reset(&mut self, cx: &mut Context<Crabdash>) {
        let fields: &[&Entity<TextField>] = &[
            &self.image,
            &self.name,
            &self.hostname,
            &self.memory,
            &self.cpus,
            &self.user,
            &self.working_dir,
            &self.entrypoint,
            &self.command,
        ];
        for field in fields {
            field.update(cx, |f, cx| f.clear(cx));
        }
        for field in self.ports.iter().chain(&self.volumes).chain(&self.env_vars) {
            field.update(cx, |f, cx| f.clear(cx));
        }
        self.ports.clear();
        self.volumes.clear();
        self.env_vars.clear();
        self.detach = true;
        self.interactive = false;
        self.remove = false;
        self.restart = RestartPolicy::default();
        self.network = NetworkMode::default();
    }

    pub fn build_args(&self, cx: &App) -> Args {
        let mut parts = Args::new();

        if self.detach {
            parts.push("-d");
        }
        if self.interactive {
            parts.push("-it");
        }
        if self.remove {
            parts.push("--rm");
        }
        if self.restart != RestartPolicy::No {
            parts.push(format!("--restart={}", self.restart.flag_value()));
        }
        if let Some(net) = self.network.flag_value() {
            parts.push(format!("--network={net}"));
        }

        let name = self.name.read(cx).text();
        if !name.is_empty() {
            parts.push(format!("--name={name}"));
        }
        let hostname = self.hostname.read(cx).text();
        if !hostname.is_empty() {
            parts.push(format!("--hostname={hostname}"));
        }
        let memory = self.memory.read(cx).text();
        if !memory.is_empty() {
            parts.push(format!("--memory={memory}"));
        }
        let cpus = self.cpus.read(cx).text();
        if !cpus.is_empty() {
            parts.push(format!("--cpus={cpus}"));
        }
        let user = self.user.read(cx).text();
        if !user.is_empty() {
            parts.push(format!("--user={user}"));
        }
        let working_dir = self.working_dir.read(cx).text();
        if !working_dir.is_empty() {
            parts.push(format!("--workdir={working_dir}"));
        }
        let entrypoint = self.entrypoint.read(cx).text();
        if !entrypoint.is_empty() {
            parts.push(format!("--entrypoint={entrypoint}"));
        }

        for field in &self.ports {
            let v = field.read(cx).text();
            if !v.is_empty() {
                parts.push("-p");
                parts.push(v);
            }
        }
        for field in &self.volumes {
            let v = field.read(cx).text();
            if !v.is_empty() {
                parts.push("-v");
                parts.push(v);
            }
        }
        for field in &self.env_vars {
            let v = field.read(cx).text();
            if !v.is_empty() {
                parts.push("-e");
                parts.push(v);
            }
        }

        let image = self.image.read(cx).text();
        parts.push(image);

        let command = self.command.read(cx).text();
        if !command.is_empty() {
            parts.push(command);
        }

        parts
    }
}
