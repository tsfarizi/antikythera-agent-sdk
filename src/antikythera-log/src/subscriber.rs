//! Log Subscriber System
//!
//! Real-time log streaming via subscription.

use crate::entries::LogEntry;
use crate::logger::Logger;
use crossbeam_channel::{Receiver, Sender};

/// Sender side (held by Logger)
pub type LogSender = Sender<LogEntry>;

/// Receiver side (given to subscriber)
pub type LogReceiver = Receiver<LogEntry>;

/// Log Subscriber
///
/// Receives log entries in real-time as they are written.
/// The subscriber is automatically removed when dropped.
pub struct LogSubscriber {
    receiver: LogReceiver,
}

impl LogSubscriber {
    pub(crate) fn new(logger: &Logger) -> Self {
        let (tx, rx) = crossbeam_channel::bounded(1000);

        let mut subscribers = logger.subscribers.lock().unwrap_or_else(|e| e.into_inner());
        subscribers.push(tx);

        Self { receiver: rx }
    }

    /// Receive next log entry (blocking)
    pub fn recv(&self) -> Result<LogEntry, crossbeam_channel::RecvError> {
        self.receiver.recv()
    }

    /// Receive next log entry (non-blocking)
    pub fn try_recv(&self) -> Result<LogEntry, crossbeam_channel::TryRecvError> {
        self.receiver.try_recv()
    }

    /// Receive next log entry with timeout
    pub fn recv_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Result<LogEntry, crossbeam_channel::RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }
}
