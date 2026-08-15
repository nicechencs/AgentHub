//! Platform-level installed-state detection capability.

mod detector;
mod registry;
mod sources;

pub use detector::{AdapterDetector, AgentDetector, FnDetector};
pub use registry::{builtin_detector_registry, DetectorRegistry};

#[cfg(test)]
mod tests;
