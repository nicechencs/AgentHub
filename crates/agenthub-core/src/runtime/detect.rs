//! Runtime detection with short TTL cache.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use crate::models::{EnvStatus, EnvStatusKind, RuntimeId};

use super::bootstrap::remediation_for;
use super::nodejs::{detect_git, detect_nodejs, detect_npm, detect_powershell};

use crate::catalog::limits::DETECT_CACHE_TTL as CACHE_TTL;

struct CacheEntry {
    at: Instant,
    statuses: Vec<EnvStatus>,
}

static CACHE: OnceLock<Mutex<Option<CacheEntry>>> = OnceLock::new();

fn cache() -> &'static Mutex<Option<CacheEntry>> {
    CACHE.get_or_init(|| Mutex::new(None))
}

pub fn invalidate_cache() {
    if let Ok(mut guard) = cache().lock() {
        *guard = None;
    }
}

/// Runtimes probed by doctor / env bar on the current host.
///
/// PowerShell is a Windows-only shared runtime: native installers on macOS/Linux
/// use official `install.sh` (bash/sh), so probing `pwsh` there only creates
/// noise and false "fix environment" chips.
pub fn host_runtimes() -> &'static [RuntimeId] {
    #[cfg(windows)]
    {
        &RuntimeId::ALL
    }
    #[cfg(not(windows))]
    {
        &[RuntimeId::NodeJs, RuntimeId::Npm, RuntimeId::Git]
    }
}

pub fn detect_all() -> Vec<EnvStatus> {
    if let Ok(guard) = cache().lock() {
        if let Some(entry) = guard.as_ref() {
            if entry.at.elapsed() < CACHE_TTL {
                return entry.statuses.clone();
            }
        }
    }

    let statuses: Vec<EnvStatus> = host_runtimes().iter().copied().map(detect_one).collect();

    if let Ok(mut guard) = cache().lock() {
        *guard = Some(CacheEntry {
            at: Instant::now(),
            statuses: statuses.clone(),
        });
    }
    statuses
}

pub fn detect_one(id: RuntimeId) -> EnvStatus {
    // Explicit PowerShell probes are Windows-only.  On macOS/Linux return a
    // static "not applicable" row so any residual caller still fails soft
    // instead of spawning `pwsh` / claiming a broken environment.
    #[cfg(not(windows))]
    if id == RuntimeId::PowerShell {
        return EnvStatus {
            id: RuntimeId::PowerShell,
            status: EnvStatusKind::Ok,
            version: None,
            path: None,
            min_required: None,
            remediation: None,
            notes: vec![
                "PowerShell is not required on this platform (native installers use bash/sh)"
                    .into(),
            ],
        };
    }

    let mut status = match id {
        RuntimeId::NodeJs => detect_nodejs(),
        RuntimeId::Npm => detect_npm(),
        RuntimeId::PowerShell => detect_powershell(),
        RuntimeId::Git => detect_git(),
    };
    if matches!(
        status.status,
        EnvStatusKind::Missing | EnvStatusKind::Outdated | EnvStatusKind::BrokenPath
    ) && status.remediation.is_none()
    {
        status.remediation = Some(remediation_for(id));
    }
    status
}

pub fn ensure(requires: &[RuntimeId]) -> Result<(), crate::models::EnvNotReady> {
    let all = detect_all();
    let map: HashMap<RuntimeId, &EnvStatus> = all.iter().map(|s| (s.id, s)).collect();
    let mut missing = Vec::new();
    let mut remediations = Vec::new();

    for req in requires {
        match map.get(req) {
            Some(s) if s.status == EnvStatusKind::Ok => {}
            Some(s) => {
                missing.push(*req);
                if let Some(r) = &s.remediation {
                    remediations.push(r.clone());
                } else {
                    remediations.push(remediation_for(*req));
                }
            }
            None => {
                missing.push(*req);
                remediations.push(remediation_for(*req));
            }
        }
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(crate::models::EnvNotReady {
            agent: None,
            channel: None,
            missing,
            remediations,
            hint: Some(
                "Install missing runtimes, restart shell/AgentHub, then re-run. Or use --install-deps when available."
                    .into(),
            ),
        })
    }
}

/// Convenience: all required runtimes are Ok.
pub fn is_ready(requires: &[RuntimeId]) -> bool {
    ensure(requires).is_ok()
}
