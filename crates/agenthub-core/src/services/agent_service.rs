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
mod tests {
    use super::*;
    use crate::adapters::{AdapterRegistry, AgentAdapter};
    use crate::error::{AppError, Result};
    use crate::models::{
        AgentConfig, AuthState, Capability, CapabilityState, DetectStatus, InstallChannel,
        RunOptions, RunSpec,
    };
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;

    static DETECT_CALLS: AtomicUsize = AtomicUsize::new(0);
    /// Serialize cache tests: they share process-wide DETECT_CALLS + generation.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    struct CountingAdapter {
        id: AgentId,
    }

    impl AgentAdapter for CountingAdapter {
        fn id(&self) -> AgentId {
            self.id
        }

        fn detect(&self) -> DetectResult {
            DETECT_CALLS.fetch_add(1, Ordering::SeqCst);
            DetectResult {
                agent: self.id,
                status: DetectStatus::Installed,
                version: Some("1.0.0".into()),
                binary_path: Some(PathBuf::from(format!("/bin/{}", self.id.as_str()))),
                channel: Some("test".into()),
                env_ready: true,
                notes: vec![],
                extra_copies: Vec::new(),
            }
        }

        fn install_channels(&self) -> Vec<InstallChannel> {
            vec![]
        }

        fn read_config(&self) -> Result<AgentConfig> {
            Err(AppError::Unsupported("count".into()))
        }

        fn read_auth(&self) -> Result<AuthState> {
            Err(AppError::Unsupported("count".into()))
        }

        fn capability(&self, _cap: Capability) -> CapabilityState {
            CapabilityState::unsupported("count")
        }

        fn skills_dir(&self) -> Option<PathBuf> {
            None
        }

        fn live_backup_paths(&self) -> Vec<PathBuf> {
            vec![]
        }

        fn build_run_spec(
            &self,
            _binary: &Path,
            _prompt: &str,
            _opts: &RunOptions,
        ) -> Result<RunSpec> {
            Err(AppError::Unsupported("count".into()))
        }
    }

    fn counting_service() -> AgentService {
        let mut reg = AdapterRegistry::new();
        for id in AgentId::ALL {
            reg.register(Arc::new(CountingAdapter { id }));
        }
        AgentService::new(reg)
    }

    #[test]
    fn cache_hit_within_ttl() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        invalidate_detect_cache();
        DETECT_CALLS.store(0, Ordering::SeqCst);
        let svc = counting_service();
        assert!(!svc.cache_is_warm());

        let a = svc.detect_all();
        let calls_after_first = DETECT_CALLS.load(Ordering::SeqCst);
        assert_eq!(a.len(), AgentId::ALL.len());
        assert_eq!(calls_after_first, AgentId::ALL.len());
        assert!(svc.cache_is_warm());

        let b = svc.detect_all();
        assert_eq!(DETECT_CALLS.load(Ordering::SeqCst), calls_after_first);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.agent, y.agent);
            assert_eq!(x.status, y.status);
            assert_eq!(x.version, y.version);
        }
    }

    #[test]
    fn cache_expires_and_redetects() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        invalidate_detect_cache();
        DETECT_CALLS.store(0, Ordering::SeqCst);
        let svc = counting_service();

        let _ = svc.detect_all_with_ttl(Duration::from_millis(20));
        let first = DETECT_CALLS.load(Ordering::SeqCst);
        assert_eq!(first, AgentId::ALL.len());

        // Still within TTL — hit.
        let _ = svc.detect_all_with_ttl(Duration::from_millis(20));
        assert_eq!(DETECT_CALLS.load(Ordering::SeqCst), first);

        thread::sleep(Duration::from_millis(40));
        let _ = svc.detect_all_with_ttl(Duration::from_millis(20));
        assert_eq!(
            DETECT_CALLS.load(Ordering::SeqCst),
            first + AgentId::ALL.len()
        );
    }

    #[test]
    fn invalidate_forces_redetect() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        invalidate_detect_cache();
        DETECT_CALLS.store(0, Ordering::SeqCst);
        let svc = counting_service();

        let _ = svc.detect_all();
        let first = DETECT_CALLS.load(Ordering::SeqCst);

        // Without invalidate: still cached.
        let _ = svc.detect_all();
        assert_eq!(DETECT_CALLS.load(Ordering::SeqCst), first);

        // Install / upgrade / uninstall hook.
        svc.invalidate_cache();
        let _ = svc.detect_all();
        assert_eq!(
            DETECT_CALLS.load(Ordering::SeqCst),
            first + AgentId::ALL.len()
        );
    }

    #[test]
    fn cache_is_warm_false_after_global_invalidate() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        invalidate_detect_cache();
        DETECT_CALLS.store(0, Ordering::SeqCst);
        let svc = counting_service();
        let _ = svc.detect_all();
        assert!(svc.cache_is_warm());

        invalidate_detect_cache();
        assert!(!svc.cache_is_warm());

        let first = DETECT_CALLS.load(Ordering::SeqCst);
        let _ = svc.detect_all();
        assert_eq!(
            DETECT_CALLS.load(Ordering::SeqCst),
            first + AgentId::ALL.len()
        );
        assert!(svc.cache_is_warm());
    }

    #[test]
    fn separate_instances_do_not_share_detect_cache() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        invalidate_detect_cache();
        DETECT_CALLS.store(0, Ordering::SeqCst);
        let a = counting_service();
        let b = counting_service();

        let _ = a.detect_all();
        let after_a = DETECT_CALLS.load(Ordering::SeqCst);
        assert_eq!(after_a, AgentId::ALL.len());
        assert!(a.cache_is_warm());
        assert!(!b.cache_is_warm());

        let _ = b.detect_all();
        assert_eq!(
            DETECT_CALLS.load(Ordering::SeqCst),
            after_a + AgentId::ALL.len()
        );
        assert!(b.cache_is_warm());
    }

    #[test]
    fn clones_share_detect_cache() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        invalidate_detect_cache();
        DETECT_CALLS.store(0, Ordering::SeqCst);
        let a = counting_service();
        let b = a.clone();

        let _ = a.detect_all();
        let after_a = DETECT_CALLS.load(Ordering::SeqCst);
        assert_eq!(after_a, AgentId::ALL.len());
        assert!(b.cache_is_warm());

        let _ = b.detect_all();
        assert_eq!(DETECT_CALLS.load(Ordering::SeqCst), after_a);
    }
}
