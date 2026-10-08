//! Startup input is bounded and never replays a truncated command sequence.
const MAX_BYTES: usize = 64 * 1024;
const MAX_EVENTS: usize = 4096;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Input {
    Bytes(Vec<u8>),
    Paste(String),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Submission {
    Pending,
    Send(Input),
    Overflow,
    Discard,
}

enum State {
    Connecting,
    Ready,
    Disabled,
}

pub(super) struct Queue {
    state: State,
    pending: Vec<Input>,
    bytes: usize,
    overflowed: bool,
}

impl Queue {
    pub fn new() -> Self {
        Self {
            state: State::Connecting,
            pending: Vec::new(),
            bytes: 0,
            overflowed: false,
        }
    }

    pub fn submit(&mut self, input: Input) -> Submission {
        match self.state {
            State::Ready => Submission::Send(input),
            State::Disabled => Submission::Discard,
            State::Connecting if self.overflowed => Submission::Discard,
            State::Connecting => {
                let bytes = match &input {
                    Input::Bytes(bytes) => bytes.len(),
                    Input::Paste(text) => text.len(),
                };
                if bytes > MAX_BYTES.saturating_sub(self.bytes) || self.pending.len() >= MAX_EVENTS
                {
                    self.pending.clear();
                    self.bytes = 0;
                    self.overflowed = true;
                    return Submission::Overflow;
                }
                self.bytes += bytes;
                self.pending.push(input);
                Submission::Pending
            }
        }
    }

    pub fn connect(&mut self) -> Vec<Input> {
        self.state = State::Ready;
        self.bytes = 0;
        self.overflowed = false;
        std::mem::take(&mut self.pending)
    }

    pub fn overflowed(&self) -> bool {
        self.overflowed
    }

    pub fn disconnect(&mut self) -> bool {
        let changed = !matches!(self.state, State::Disabled);
        self.state = State::Disabled;
        self.pending.clear();
        self.bytes = 0;
        self.overflowed = false;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_preserves_typed_and_pasted_events_in_order() {
        let mut queue = Queue::new();
        assert_eq!(
            queue.submit(Input::Bytes(b"printf ".to_vec())),
            Submission::Pending
        );
        assert_eq!(queue.submit(Input::Paste("雪".into())), Submission::Pending);
        assert_eq!(
            queue.submit(Input::Bytes(b"\r".to_vec())),
            Submission::Pending
        );
        assert_eq!(
            queue.connect(),
            vec![
                Input::Bytes(b"printf ".to_vec()),
                Input::Paste("雪".into()),
                Input::Bytes(b"\r".to_vec())
            ]
        );
        assert_eq!(
            queue.submit(Input::Bytes(b"next".to_vec())),
            Submission::Send(Input::Bytes(b"next".to_vec()))
        );
        assert!(queue.connect().is_empty());
    }

    #[test]
    fn byte_overflow_discards_the_entire_sequence_until_connection() {
        let mut queue = Queue::new();
        assert_eq!(
            queue.submit(Input::Bytes(vec![b'x'; MAX_BYTES])),
            Submission::Pending
        );
        assert_eq!(
            queue.submit(Input::Paste("overflow".into())),
            Submission::Overflow
        );
        assert!(queue.overflowed());
        assert_eq!(
            queue.submit(Input::Bytes(b"\r".to_vec())),
            Submission::Discard
        );
        assert!(queue.connect().is_empty());
        assert!(!queue.overflowed());
        assert_eq!(
            queue.submit(Input::Bytes(b"new".to_vec())),
            Submission::Send(Input::Bytes(b"new".to_vec()))
        );
        let mut oversized = Queue::new();
        assert_eq!(
            oversized.submit(Input::Paste("x".repeat(MAX_BYTES + 1))),
            Submission::Overflow
        );
        assert!(oversized.connect().is_empty());
    }

    #[test]
    fn event_limit_bounds_small_or_empty_events_without_partial_replay() {
        let mut queue = Queue::new();
        for _ in 0..MAX_EVENTS {
            assert_eq!(queue.submit(Input::Bytes(Vec::new())), Submission::Pending);
        }
        assert_eq!(
            queue.submit(Input::Bytes(b"\r".to_vec())),
            Submission::Overflow
        );
        assert!(queue.connect().is_empty());
    }

    #[test]
    fn failure_and_disconnect_clear_disable_and_never_replay_after_reconnect() {
        for connected in [false, true] {
            let mut queue = Queue::new();
            if connected {
                queue.connect();
            }
            queue.submit(Input::Paste("old command".into()));
            queue.disconnect();
            assert_eq!(
                queue.submit(Input::Bytes(b"\r".to_vec())),
                Submission::Discard
            );
            assert!(queue.connect().is_empty());
            assert_eq!(
                queue.submit(Input::Bytes(b"fresh".to_vec())),
                Submission::Send(Input::Bytes(b"fresh".to_vec()))
            );
        }
    }
}
