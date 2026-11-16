//! Reads the OSC 10 and 11 color replies from the terminal on stdin. A device
//! attributes query goes last: every terminal answers it, so a terminal that
//! ignores color queries costs one round trip, not the whole timeout.
use std::{
    io::{Read, Write, stdin, stdout},
    os::fd::AsRawFd,
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_millis(400);

pub fn query() -> Option<String> {
    let mut out = stdout();
    out.write_all(b"\x1b]10;?\x1b\\\x1b]11;?\x1b\\\x1b[c")
        .ok()?;
    out.flush().ok()?;
    let input = stdin();
    let fd = input.as_raw_fd();
    let mut reply = Vec::new();
    let deadline = Instant::now() + TIMEOUT;
    // Done once the device attributes reply, `ESC [ ? ... c`, arrives.
    while !answered(&reply) {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let mut poll = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: `poll` points to one valid pollfd for the duration of the call.
        let ready = unsafe { libc::poll(&mut poll, 1, left.as_millis() as libc::c_int) };
        if ready <= 0 {
            break;
        }
        let mut chunk = [0u8; 256];
        let read = input.lock().read(&mut chunk).ok()?;
        if read == 0 {
            break;
        }
        reply.extend_from_slice(&chunk[..read]);
    }
    Some(String::from_utf8_lossy(&reply).into_owned())
}

/// Whether the device attributes reply, `ESC [ ? ... c`, has arrived.
fn answered(reply: &[u8]) -> bool {
    reply
        .windows(3)
        .rposition(|window| window == b"\x1b[?")
        .is_some_and(|start| reply[start..].contains(&b'c'))
}
