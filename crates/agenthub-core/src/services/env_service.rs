use crate::models::{EnvNotReady, EnvStatus, RuntimeId};
use crate::runtime;

pub struct EnvService;

impl EnvService {
    pub fn new() -> Self {
        Self
    }

    pub fn detect_all(&self) -> Vec<EnvStatus> {
        runtime::detect_all()
    }

    pub fn detect(&self, id: RuntimeId) -> EnvStatus {
        runtime::detect_one(id)
    }

    pub fn ensure(&self, requires: &[RuntimeId]) -> Result<(), EnvNotReady> {
        runtime::ensure(requires)
    }

    pub fn invalidate_cache(&self) {
        runtime::invalidate_cache();
    }
}

impl Default for EnvService {
    fn default() -> Self {
        Self::new()
    }
}
