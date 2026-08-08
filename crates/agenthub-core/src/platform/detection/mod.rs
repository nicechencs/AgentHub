//! Platform-level installed-state detection capability.

mod detector;
mod registry;

pub use detector::{AdapterDetector, AgentDetector};
pub use registry::{builtin_detector_registry, DetectorRegistry};

#[cfg(test)]
mod tests;
