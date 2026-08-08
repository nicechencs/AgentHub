//! In-call progress sink (not a global event bus).

use super::types::ProgressEvent;

/// Receives typed progress for a single lifecycle invocation.
pub trait ProgressSink: Send {
    fn on_progress(&mut self, event: ProgressEvent);
}

/// No-op sink.
pub struct NullProgressSink;

impl ProgressSink for NullProgressSink {
    fn on_progress(&mut self, _event: ProgressEvent) {}
}

/// Collects events for tests.
#[derive(Default)]
pub struct VecProgressSink {
    pub events: Vec<ProgressEvent>,
}

impl ProgressSink for VecProgressSink {
    fn on_progress(&mut self, event: ProgressEvent) {
        self.events.push(event);
    }
}

/// Forwards each progress message as a plain log line (GUI install stream hook).
pub struct LogLineProgressSink {
    emit: Box<dyn FnMut(&str) + Send>,
}

impl LogLineProgressSink {
    pub fn new(emit: impl FnMut(&str) + Send + 'static) -> Self {
        Self {
            emit: Box::new(emit),
        }
    }
}

impl ProgressSink for LogLineProgressSink {
    fn on_progress(&mut self, event: ProgressEvent) {
        let line = format!("# [{}] {}", event.step, event.message);
        (self.emit)(&line);
    }
}
