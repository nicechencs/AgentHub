//! Agent detect service with a per-instance TTL cache.
//!
//! Install / lifecycle still call [`invalidate_detect_cache`] without an
//! `AgentService`: a process-wide generation makes every instance treat stored
//! results as a miss.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::adapters::AdapterRegistry;
use crate::catalog::limits::AGENT_DETECT_CACHE_TTL as CACHE_TTL;
use crate::models::{AgentId, DetectResult};

struct CacheEntry {
    at: Instant,
    generation: u64,
    results: Vec<DetectResult>,
}

impl CacheEntry {
    fn is_fresh(&self, ttl: Duration) -> bool {
        self.generation == CACHE_GENERATION.load(Ordering::SeqCst) && self.at.elapsed() < ttl
    }
}

/// Bumped by [`invalidate_detect_cache`] so every instance drops stale results.
static CACHE_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Drop cached agent detect results (call after install / upgrade / uninstall).
pub fn invalidate_detect_cache() {
    CACHE_GENERATION.fetch_add(1, Ordering::SeqCst);
}

#[derive(Clone)]
pub struct AgentService {
    registry: AdapterRegistry,
    /// Shared across clones of the same service; not shared across `new()`.
    cache: Arc<Mutex<Option<CacheEntry>>>,
}

impl AgentService {
    pub fn new(registry: AdapterRegistry) -> Self {
        Self {
            registry,
            cache: Arc::new(Mutex::new(None)),
        }
    }

    /// Invalidate the shared detect cache (install / upgrade / uninstall hooks).
    pub fn invalidate_cache(&self) {
        invalidate_detect_cache();
        if let Ok(mut guard) = self.cache.lock() {
            *guard = None;
        }
    }

    pub fn detect_all(&self) -> Vec<DetectResult> {
        self.detect_all_with_ttl(CACHE_TTL)
    }

    pub fn cache_is_warm(&self) -> bool {
        self.cache
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|entry| entry.is_fresh(CACHE_TTL)))
            .unwrap_or(false)
    }

    fn detect_all_with_ttl(&self, ttl: Duration) -> Vec<DetectResult> {
        if let Ok(guard) = self.cache.lock() {
            if let Some(entry) = guard.as_ref() {
                if entry.is_fresh(ttl) {
                    return entry.results.clone();
                }
            }
        }

        let generation = CACHE_GENERATION.load(Ordering::SeqCst);
        let results = self.detect_all_uncached();
        if CACHE_GENERATION.load(Ordering::SeqCst) != generation {
            return results;
        }

        if let Ok(mut guard) = self.cache.lock() {
            *guard = Some(CacheEntry {
                at: Instant::now(),
                generation,
                results: results.clone(),
            });
        }
        results
    }

    fn detect_all_uncached(&self) -> Vec<DetectResult> {
        use crate::logging::targets;
        use crate::models::DetectStatus;
        use std::time::Instant;

        let started = Instant::now();
        let adapters = self.registry.all();
        // Parallel probes: each agent may run --version (up to ~5s). Sequential
        // wall time was Agents-page jank; order still follows registry/ALL.
        let results: Vec<DetectResult> = std::thread::scope(|scope| {
            let handles: Vec<_> = adapters
                .into_iter()
                .map(|a| scope.spawn(move || a.detect()))
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("agent detect thread"))
                .collect()
        });

        let installed = results
            .iter()
            .filter(|r| r.status == DetectStatus::Installed)
            .count();
        let missing = results.len().saturating_sub(installed);
        tracing::info!(
            target: targets::DETECT,
            module = targets::DETECT,
            op = "detect_all",
            installed,
            missing,
            total = results.len(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "agent detect sweep complete"
        );
        for r in &results {
            match r.status {
                DetectStatus::Installed => {
                    tracing::debug!(
                        target: targets::DETECT,
                        module = targets::DETECT,
                        op = "detect_all",
                        agent = r.agent.as_str(),
                        status = "installed",
                        channel = r.channel.as_deref().unwrap_or("-"),
                        version = r.version.as_deref().unwrap_or("-"),
                        path = %r
                            .binary_path
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "-".into()),
                        notes = r.notes.len(),
                        "agent status"
                    );
                }
                DetectStatus::NotFound => {
                    tracing::debug!(
                        target: targets::DETECT,
                        module = targets::DETECT,
                        op = "detect_all",
                        agent = r.agent.as_str(),
                        status = "not_found",
                        "agent status"
                    );
                }
            }
        }
        results
    }

    pub fn detect(&self, id: AgentId) -> Option<DetectResult> {
        // Single-agent probe always hits the adapter (install redetect path).
        self.registry.get(id).map(|a| a.detect())
    }

    pub fn registry(&self) -> &AdapterRegistry {
        &self.registry
    }
}

#[cfg(test)]
mod tests;
