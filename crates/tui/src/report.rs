//! Tells the terminal what the dashboard is and is doing: its title (OSC 2)
//! and its program status (OSC 7501), which Rex shows on the pane's tab, such
//! as a spinner while a container restarts. Rex also picks a pane's icon by
//! the status's `app`.
use base64::{Engine, engine::general_purpose::STANDARD};
use std::io::{self, Write};

const APP: &str = "crabdash";
/// OSC 7501's cap on a decoded `msg`.
const MSG_BYTES: usize = 2048;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub title: String,
    /// An action is running.
    pub working: bool,
    pub msg: Option<String>,
}

/// Writes a report only when it differs from the last one.
#[derive(Default)]
pub struct Reporter {
    last: Option<Report>,
}

impl Reporter {
    /// Saves the terminal's title, to put back when the dashboard closes.
    pub fn start(out: &mut impl Write) -> io::Result<()> {
        out.write_all(b"\x1b[22;0t")?;
        out.flush()
    }

    pub fn send(&mut self, out: &mut impl Write, report: Report) -> io::Result<()> {
        if self.last.as_ref() == Some(&report) {
            return Ok(());
        }
        if self
            .last
            .as_ref()
            .is_none_or(|last| last.title != report.title)
        {
            write!(out, "\x1b]2;{}\x07", clean(&report.title))?;
        }
        out.write_all(status(&report).as_bytes())?;
        self.last = Some(report);
        out.flush()
    }

    /// Clears the status and restores the saved title.
    pub fn finish(out: &mut impl Write) -> io::Result<()> {
        out.write_all(b"\x1b]7501;state=clear\x1b\\\x1b[23;0t")?;
        out.flush()
    }
}

/// The OSC 7501 report: `idle` at rest, `working` while an action runs.
fn status(report: &Report) -> String {
    let state = if report.working { "working" } else { "idle" };
    let title = STANDARD.encode(clean(&report.title));
    let mut sequence = format!("\x1b]7501;state={state}:app={APP}:title={title}");
    if let Some(msg) = &report.msg {
        let mut msg = clean(msg);
        while msg.len() > MSG_BYTES {
            msg.pop();
        }
        sequence.push_str(":msg=");
        sequence.push_str(&STANDARD.encode(msg));
    }
    sequence.push_str("\x1b\\");
    sequence
}

/// Terminals refuse a title or message with control characters, so a
/// multi-line error becomes one line.
fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_working_with_a_one_line_message_and_only_on_change()
    -> Result<(), Box<dyn std::error::Error>> {
        let report = Report {
            title: "Crabdash · Docker".into(),
            working: true,
            msg: Some("Restarting cache…\nsecond line".into()),
        };
        let mut out = Vec::new();
        let mut reporter = Reporter::default();
        reporter.send(&mut out, report.clone())?;
        let written = String::from_utf8(out.clone())?;
        let title = STANDARD.encode("Crabdash · Docker");
        let msg = STANDARD.encode("Restarting cache… second line");
        assert_eq!(
            written,
            format!(
                "\x1b]2;Crabdash · Docker\x07\x1b]7501;state=working:app=crabdash:title={title}:msg={msg}\x1b\\"
            )
        );
        reporter.send(&mut out, report.clone())?;
        assert_eq!(out.len(), written.len());
        // A new state without a message, keeping the title, writes only the status.
        reporter.send(
            &mut out,
            Report {
                working: false,
                msg: None,
                ..report
            },
        )?;
        assert_eq!(
            String::from_utf8(out)?[written.len()..],
            format!("\x1b]7501;state=idle:app=crabdash:title={title}\x1b\\")
        );
        Ok(())
    }
}
