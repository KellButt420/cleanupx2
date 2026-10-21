//! Background operations whose messages are pulled across the FFI boundary.
//!
//! Work runs on a core-owned thread and pushes serialized JSON messages into
//! a channel. The frontend pulls them with a timeout, so no callbacks ever
//! enter .NET from foreign threads.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use cleanup_core::{CoreError, ErrorCode};
use serde::Serialize;

pub struct Operation {
    receiver: Mutex<Receiver<Vec<u8>>>,
    cancelled: Arc<AtomicBool>,
}

pub struct Sink {
    sender: Sender<Vec<u8>>,
    cancelled: Arc<AtomicBool>,
}

pub enum Next {
    Message(Vec<u8>),
    Timeout,
    Complete,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Failure<'a> {
    Error { error: &'a CoreError },
}

impl Sink {
    pub fn send<T: Serialize>(&self, message: &T) {
        if let Ok(bytes) = serde_json::to_vec(message) {
            // A closed channel means the handle was freed; drop the message.
            let _ = self.sender.send(bytes);
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

impl Operation {
    pub fn spawn(
        name: &str,
        work: impl FnOnce(&Sink) + Send + 'static,
    ) -> Result<Box<Operation>, CoreError> {
        let (sender, receiver) = channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let sink = Sink {
            sender,
            cancelled: Arc::clone(&cancelled),
        };
        thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                if let Err(payload) = catch_unwind(AssertUnwindSafe(|| work(&sink))) {
                    let error = crate::panic_error(payload);
                    sink.send(&Failure::Error { error: &error });
                }
            })
            .map_err(|e| {
                CoreError::new(ErrorCode::Internal, format!("Could not start worker: {e}"))
            })?;
        Ok(Box::new(Operation {
            receiver: Mutex::new(receiver),
            cancelled,
        }))
    }

    pub fn next(&self, timeout_ms: u32) -> Next {
        let receiver = match self.receiver.lock() {
            Ok(receiver) => receiver,
            Err(poisoned) => poisoned.into_inner(),
        };
        match receiver.recv_timeout(Duration::from_millis(u64::from(timeout_ms))) {
            Ok(bytes) => Next::Message(bytes),
            Err(RecvTimeoutError::Timeout) => Next::Timeout,
            Err(RecvTimeoutError::Disconnected) => Next::Complete,
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_panics_are_reported_as_messages() {
        let operation = Operation::spawn("test", |_| panic!("worker exploded")).unwrap();
        let mut seen_error = false;
        loop {
            match operation.next(1000) {
                Next::Message(bytes) => {
                    let text = String::from_utf8(bytes).unwrap();
                    assert!(
                        text.contains("\"type\":\"error\"") && text.contains("worker exploded")
                    );
                    seen_error = true;
                }
                Next::Timeout => {}
                Next::Complete => break,
            }
        }
        assert!(seen_error);
    }
}
