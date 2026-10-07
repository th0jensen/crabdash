use gpui::*;
use services::docker::{NetworkMode, RestartPolicy};
use utils::args::Args;

use super::run_parameters::Parameters;
use crate::app::Crabdash;
use crate::components::text_field::TextField;

pub struct DockerRunConfig {
    pub image: Entity<TextField>,
    pub name: Entity<TextField>,
    pub hostname: Entity<TextField>,
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
    pub busy: bool,
    pub error: Option<String>,
    _changes: Vec<Subscription>,
}

impl DockerRunConfig {
    pub fn new(cx: &mut Context<Crabdash>) -> Self {
        let mut config = Self {
            image: cx.new(|cx| TextField::new("", "nginx:latest", 50, cx)),
            name: cx.new(|cx| TextField::new("", "my-container", 51, cx)),
            hostname: cx.new(|cx| TextField::new("", "my-host", 52, cx)),
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
            command: cx.new(|cx| TextField::new("", "sh -c 'echo hello'", 58, cx)),
            busy: false,
            error: None,
            _changes: Vec::new(),
        };
        let fields = [
            &config.image,
            &config.name,
            &config.hostname,
            &config.memory,
            &config.cpus,
            &config.user,
            &config.working_dir,
            &config.entrypoint,
            &config.command,
        ];
        for field in fields {
            config._changes.push(observe_field(field, cx));
        }
        config
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
        self.interactive = false;
        self.remove = false;
        self.restart = RestartPolicy::default();
        self.network = NetworkMode::default();
        self.error = None;
        self._changes.truncate(9);
    }

    pub fn add_field(&mut self, kind: &'static str, cx: &mut Context<Crabdash>) {
        let (placeholder, tab) = match kind {
            "port" => ("127.0.0.1:8080:80/tcp", 60),
            "volume" => ("/host/path:/container/path:ro", 61),
            _ => ("KEY=value", 62),
        };
        let field = cx.new(|cx| TextField::new("", placeholder, tab, cx));
        self._changes.push(observe_field(&field, cx));
        match kind {
            "port" => self.ports.push(field),
            "volume" => self.volumes.push(field),
            _ => self.env_vars.push(field),
        }
    }

    pub fn build_args(&self, cx: &App) -> Result<Args, String> {
        Parameters {
            image: self.image.read(cx).text(),
            name: self.name.read(cx).text(),
            hostname: self.hostname.read(cx).text(),
            interactive: self.interactive,
            remove: self.remove,
            restart: self.restart,
            network: self.network,
            ports: self.ports.iter().map(|f| f.read(cx).text()).collect(),
            volumes: self.volumes.iter().map(|f| f.read(cx).text()).collect(),
            env_vars: self.env_vars.iter().map(|f| f.read(cx).text()).collect(),
            memory: self.memory.read(cx).text(),
            cpus: self.cpus.read(cx).text(),
            user: self.user.read(cx).text(),
            working_dir: self.working_dir.read(cx).text(),
            entrypoint: self.entrypoint.read(cx).text(),
            command: self.command.read(cx).text(),
        }
        .build_args()
    }
}

fn observe_field(field: &Entity<TextField>, cx: &mut Context<Crabdash>) -> Subscription {
    let mut previous = field.read(cx).text();
    cx.observe(field, move |this, field, cx| {
        let text = field.read(cx).text();
        if text != previous {
            previous = text;
            this.docker_run_config.error = None;
            cx.notify();
        }
    })
}
