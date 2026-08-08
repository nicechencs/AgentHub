//! Debounced filesystem watch on shared + agent skill roots.
//! Emits `skills-fs-changed` so the GUI can refresh list/matrix without a manual reload.

use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use agenthub_core::catalog::limits::SKILL_FS_DEBOUNCE as DEBOUNCE;
use agenthub_core::logging::targets;
use agenthub_core::models::AgentId;
use agenthub_core::AgentHub;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

const EVENT_NAME: &str = "skills-fs-changed";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillsFsChanged {
    /// Always "fs" for now; reserved for future sources (e.g. "manual").
    source: &'static str,
    /// Number of watched roots at start (informational).
    roots: usize,
}

/// Collect directories to watch: shared library + each agent skills dir (or parent if missing).
pub fn collect_skill_watch_roots(hub: &AgentHub) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    let shared = hub.skills.source_root().to_path_buf();
    push_watch_path(&mut roots, shared);

    for id in AgentId::ALL {
        let Some(adapter) = hub.skills.registry().get(id) else {
            continue;
        };
        if let Some(dir) = adapter.skills_dir() {
            push_watch_path(&mut roots, dir);
        }
    }

    roots.sort();
    roots.dedup();
    roots
}

fn push_watch_path(out: &mut Vec<PathBuf>, path: PathBuf) {
    if path.is_dir() {
        out.push(path);
        return;
    }
    // Root not created yet — watch parent so first create is visible.
    if let Some(parent) = path.parent() {
        if parent.is_dir() {
            out.push(parent.to_path_buf());
        }
    }
}

/// Spawn a background watcher thread. Best-effort: failures are logged, GUI still runs.
pub fn start_skill_watcher(app: AppHandle, hub: Arc<AgentHub>) {
    let roots = collect_skill_watch_roots(&hub);
    if roots.is_empty() {
        tracing::info!(
            target: targets::GUI,
            module = targets::GUI,
            op = "skill_watch",
            "no skill roots to watch"
        );
        return;
    }

    let roots_count = roots.len();
    let root_labels: Vec<String> = roots
        .iter()
        .map(|p| p.display().to_string())
        .collect();

    std::thread::Builder::new()
        .name("skill-fs-watch".into())
        .spawn(move || {
            if let Err(e) = run_watch_loop(app, hub, roots, roots_count) {
                tracing::warn!(
                    target: targets::GUI,
                    module = targets::GUI,
                    op = "skill_watch",
                    error = %e,
                    roots = ?root_labels,
                    "skill filesystem watcher stopped"
                );
            }
        })
        .ok();
}

fn run_watch_loop(
    app: AppHandle,
    hub: Arc<AgentHub>,
    roots: Vec<PathBuf>,
    roots_count: usize,
) -> Result<(), String> {
    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.send(res);
        },
        Config::default(),
    )
    .map_err(|e| e.to_string())?;

    for root in &roots {
        if let Err(e) = watcher.watch(root, RecursiveMode::Recursive) {
            tracing::warn!(
                target: targets::GUI,
                module = targets::GUI,
                op = "skill_watch",
                path = %root.display(),
                error = %e,
                "failed to watch skill root"
            );
        } else {
            tracing::info!(
                target: targets::GUI,
                module = targets::GUI,
                op = "skill_watch",
                path = %root.display(),
                "watching skill root"
            );
        }
    }

    // Keep watcher alive for the thread lifetime.
    let _watcher = watcher;

    let mut pending = false;
    let mut last_event = Instant::now();

    loop {
        // Wait for first event (or timeout while pending).
        let timeout = if pending {
            DEBOUNCE
                .checked_sub(last_event.elapsed())
                .unwrap_or(Duration::ZERO)
        } else {
            Duration::from_secs(3600)
        };

        match rx.recv_timeout(timeout) {
            Ok(Ok(_ev)) => {
                pending = true;
                last_event = Instant::now();
                // Drain burst.
                while rx.try_recv().is_ok() {}
            }
            Ok(Err(e)) => {
                tracing::debug!(
                    target: targets::GUI,
                    module = targets::GUI,
                    op = "skill_watch",
                    error = %e,
                    "notify error"
                );
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if pending {
                    pending = false;
                    // Drop process list cache before GUI reloads so the next
                    // list_skills / list_installed does not serve a stale matrix.
                    hub.skills.invalidate_list_cache();
                    let payload = SkillsFsChanged {
                        source: "fs",
                        roots: roots_count,
                    };
                    if let Err(e) = app.emit(EVENT_NAME, &payload) {
                        tracing::debug!(
                            target: targets::GUI,
                            module = targets::GUI,
                            op = "skill_watch",
                            error = %e,
                            "emit skills-fs-changed failed"
                        );
                    } else {
                        tracing::debug!(
                            target: targets::GUI,
                            module = targets::GUI,
                            op = "skill_watch",
                            "emitted skills-fs-changed"
                        );
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    Ok(())
}
