//! crabdash-tui: Crabdash's dashboard for the terminal and for Rex panes.
mod app;
mod palette;
mod poll;
mod report;
mod rex;
mod search;
mod section;
mod touch;
mod view;

use anyhow::{Result, bail};
use section::Section;

const USAGE: &str = "\
crabdash-tui: Crabdash's dashboard for this machine, in the terminal

Usage:
  crabdash-tui [docker|disks|services|system]   open the dashboard at a section
  crabdash-tui rex install | uninstall          add Crabdash to Rex's command palette

Keys: left/right or 1-4 switch sections, / search, f/F next/previous filter,
j/k select, tab/enter expand, s start/stop, r restart, p pause, l logs,
esc clear or close, q quit. Tap tabs, chips, the search field and rows;
swipe to scroll.";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["-h" | "--help" | "help"] => println!("{USAGE}"),
        ["-V" | "--version"] => println!("crabdash-tui {}", env!("CARGO_PKG_VERSION")),
        ["rex", "install"] => println!("{}", rex::install()?),
        ["rex", "uninstall"] => println!("{}", rex::uninstall()?),
        [] => open(Section::default())?,
        [name] => match Section::parse(name) {
            Some(section) => open(section)?,
            None => bail!("Unknown section {name:?}.\n\n{USAGE}"),
        },
        _ => bail!("{USAGE}"),
    }
    Ok(())
}

fn open(section: Section) -> Result<()> {
    // A failed refresh leaves the installed plugin as it was.
    let _ = rex::refresh();
    app::run(section)
}
