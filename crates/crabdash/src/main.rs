#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    tracing_subscriber::fmt::init();
    app::desktop::run();
}
