#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() -> std::process::ExitCode {
    tracing_subscriber::fmt::init();
    match app::desktop::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Unable to start Crabdash: {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}
