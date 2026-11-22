//! Requests the main loop hands to the pool, and their answers on the way
//! back.
//!
//! Work on the pool reads what it is given and nothing of the main loop's:
//! a request over the design takes the snapshot that is current when it
//! arrives. The main loop answers it when its result comes back, so that
//! every response is sent from one thread, and a cancelled request is
//! answered as cancelled whatever its work made.

use std::any::Any;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_channel::{Receiver, Sender};
use lsp_server::{ErrorCode, RequestId, ResponseError};
use serde_json::Value;

/// Whether the client has cancelled a request, which its work asks between
/// steps so that it can stop early.
#[derive(Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    /// Whether the client has cancelled the request.
    pub fn cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// `Err` once the request is cancelled, for work to return with `?`.
    pub fn check(&self) -> Result<(), ResponseError> {
        match self.cancelled() {
            true => Err(cancelled()),
            false => Ok(()),
        }
    }
}

/// The answer to a request cancelled before it was answered.
fn cancelled() -> ResponseError {
    ResponseError {
        code: ErrorCode::RequestCanceled as i32,
        message: "cancelled".to_string(),
        data: None,
    }
}

/// What a request's work made, or the panic it ended in.
pub type Outcome = std::thread::Result<Result<Value, ResponseError>>;

/// A request's work, done.
pub struct Done {
    pub id: RequestId,
    pub method: String,
    pub outcome: Outcome,
}

/// The requests on the pool, each with its flag, and the channel their
/// answers come back on.
pub struct Tasks {
    running: HashMap<RequestId, Cancel>,
    sender: Sender<Done>,
    /// What the main loop waits on beside the client.
    pub receiver: Receiver<Done>,
}

impl Tasks {
    pub fn new() -> Tasks {
        let (sender, receiver) = crossbeam_channel::unbounded();
        Tasks {
            running: HashMap::new(),
            sender,
            receiver,
        }
    }

    /// Runs `work` on the pool for the request `id`. A panic in it is
    /// caught there, since one that reached rayon would end the process.
    pub fn spawn(
        &mut self,
        id: RequestId,
        method: String,
        work: impl FnOnce(&Cancel) -> Result<Value, ResponseError> + Send + 'static,
    ) {
        let cancel = Cancel::default();
        self.running.insert(id.clone(), cancel.clone());
        let sender = self.sender.clone();
        rayon::spawn(move || {
            let outcome = catch_unwind(AssertUnwindSafe(|| work(&cancel)));
            let _ = sender.send(Done {
                id,
                method,
                outcome,
            });
        });
    }

    /// Marks the request `id` cancelled. One already answered, or never
    /// sent to the pool, is left alone: its answer is on its way or sent.
    pub fn cancel(&mut self, id: &RequestId) {
        if let Some(cancel) = self.running.get(id) {
            cancel.0.store(true, Ordering::Relaxed);
        }
    }

    /// What `done` answers its request with: cancelled if the client
    /// cancelled it, whatever its work made, unless it panicked, which is
    /// worth telling whether or not the answer is still wanted.
    pub fn finish(&mut self, done: Done) -> Outcome {
        let cancel = self.running.remove(&done.id).unwrap_or_default();
        match (cancel.cancelled(), done.outcome) {
            (true, Ok(_)) => Ok(Err(cancelled())),
            (_, outcome) => outcome,
        }
    }
}

/// What a panic said, from the payload `catch_unwind` caught.
pub fn panic_message(payload: &(dyn Any + Send)) -> &str {
    match payload.downcast_ref::<&str>() {
        Some(message) => message,
        None => match payload.downcast_ref::<String>() {
            Some(message) => message,
            None => "a panic with no message",
        },
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use lsp_server::{ErrorCode, RequestId};
    use serde_json::Value;

    use super::{Tasks, panic_message};

    #[test]
    fn a_cancelled_request_is_answered_as_cancelled() {
        let mut tasks = Tasks::new();
        let id = RequestId::from(1);
        // Waits for its cancellation, or answers after ten seconds, which
        // fails the test.
        tasks.spawn(id.clone(), "wait".to_string(), |cancel| {
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(10) {
                cancel.check()?;
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(Value::from("not cancelled"))
        });
        // Cancelling a request never sent changes nothing.
        tasks.cancel(&RequestId::from(2));
        tasks.cancel(&id);
        let done = tasks.receiver.recv().unwrap();
        let answer = tasks.finish(done).expect("no panic");
        assert_eq!(answer.unwrap_err().code, ErrorCode::RequestCanceled as i32);
    }

    #[test]
    fn a_panic_on_the_pool_is_caught() {
        let mut tasks = Tasks::new();
        tasks.spawn(RequestId::from(1), "panic".to_string(), |_| {
            panic!("work on the pool asked to panic")
        });
        let done = tasks.receiver.recv().unwrap();
        let payload = tasks.finish(done).expect_err("a panic");
        assert_eq!(panic_message(&*payload), "work on the pool asked to panic");
    }
}
