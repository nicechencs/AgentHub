//! Multi-agent run orchestration (parallel / sequential).

use std::sync::Arc;
use std::thread;
use std::time::Instant;

use chrono::Utc;

use crate::adapters::AdapterRegistry;
use crate::error::{AppError, Result};
use crate::logging::{self, targets};
use crate::models::{
    AgentId, AgentRunResult, DetectStatus, MultiRunReport, OutputStream, RunEvent, RunMode,
    RunOptions, RunSpec, RunStatus,
};
use crate::utils::process::{
    program_from_detect, CancelToken, ProcessRunner, StreamingProcessRunner, SystemProcessRunner,
};
use crate::utils::stream_parse::{StreamOutput, StreamSession};

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn stream_output_to_event(agent: AgentId, out: StreamOutput) -> RunEvent {
    match out {
        StreamOutput::Chunk { stream, text } => RunEvent::Chunk {
            agent,
            stream,
            text,
        },
        StreamOutput::Step(step) => RunEvent::Step { agent, step },
    }
}

fn emit_stream_outputs(
    agent: AgentId,
    outputs: Vec<StreamOutput>,
    on_event: &(dyn Fn(RunEvent) + Send + Sync),
) {
    for out in outputs {
        on_event(stream_output_to_event(agent, out));
    }
}

/// Structured mode: replace captured stdout with decoded assistant text only
/// when the parser consumed every byte (no truncation, reader error, live
/// drop, UTF-8 error, or leftover/overflow). Incomplete signals stay on the
/// result; native session id is still copied.
fn apply_structured_stdout(result: &mut AgentRunResult, session: &StreamSession) {
    if session.is_structured()
        && session.consumed_complete()
        && !result.truncated
        && !matches!(result.status, RunStatus::Timeout | RunStatus::Cancelled)
    {
        result.stdout = session.assistant_text().to_string();
    }
    if result.native_session_id.is_none() {
        result.native_session_id = session.native_session_id().map(str::to_string);
    }
}

fn log_failed_agent_results(op: &str, results: &[AgentRunResult]) {
    use crate::utils::redact::redact_text;
    for r in results {
        if matches!(
            r.status,
            RunStatus::Failed | RunStatus::Timeout | RunStatus::Cancelled
        ) {
            let cmd = redact_text(&truncate_prompt(&r.command, 120));
            let err = redact_text(r.error.as_deref().unwrap_or("-"));
            let code = match r.status {
                RunStatus::Timeout => "run.timeout",
                RunStatus::Cancelled => "run.cancelled",
                _ => "run.failed",
            };
            tracing::error!(
                module = targets::RUN,
                code = code,
                op = op,
                agent = r.agent.as_str(),
                status = r.status.as_str(),
                exit_code = ?r.exit_code,
                command = %cmd,
                error = %err,
                "agent run failed"
            );
        }
    }
}

pub struct RunService {
    registry: AdapterRegistry,
    runner: Arc<dyn ProcessRunner>,
    streaming: Arc<dyn StreamingProcessRunner>,
}

impl RunService {
    pub fn new(registry: AdapterRegistry) -> Self {
        Self {
            registry,
            runner: Arc::new(SystemProcessRunner),
            streaming: Arc::new(SystemProcessRunner),
        }
    }

    /// Construct with an injectable runner that supports both batch and streaming (unit tests).
    pub fn with_runner<R>(registry: AdapterRegistry, runner: Arc<R>) -> Self
    where
        R: ProcessRunner + StreamingProcessRunner + 'static,
    {
        Self {
            registry,
            runner: runner.clone(),
            streaming: runner,
        }
    }

    /// Run the same prompt on one or more agents.
    pub fn run(
        &self,
        agents: &[AgentId],
        prompt: &str,
        opts: &RunOptions,
    ) -> Result<MultiRunReport> {
        let started = Instant::now();
        let result = (|| {
            let prompt = prompt.trim();
            if prompt.is_empty() {
                return Err(AppError::InvalidArg("prompt must not be empty".into()));
            }
            if agents.is_empty() {
                return Err(AppError::InvalidArg("agent list is empty".into()));
            }

            let started_at = Utc::now().to_rfc3339();

            // Resolve specs first (sequential, cheap).
            let mut jobs: Vec<(AgentId, Option<RunSpec>, Option<AgentRunResult>)> = Vec::new();
            for &id in agents {
                match self.resolve_job(id, prompt, opts) {
                    Ok(ResolveOutcome::Ready(spec)) => jobs.push((id, Some(spec), None)),
                    Ok(ResolveOutcome::Early(result)) => jobs.push((id, None, Some(result))),
                    Err(e) => return Err(e),
                }
            }

            let results = match opts.mode {
                RunMode::Sequential => self.run_sequential(&jobs, opts),
                RunMode::Parallel => self.run_parallel(&jobs, opts),
            };

            let finished_at = Utc::now().to_rfc3339();
            Ok(MultiRunReport::from_results(
                prompt.to_string(),
                opts.mode,
                results,
                started_at,
                finished_at,
            ))
        })();

        match &result {
            Ok(report) => {
                log_failed_agent_results("run", &report.results);
                tracing::info!(
                    module = targets::RUN,
                    op = "run",
                    agents = agents.len(),
                    mode = opts.mode.as_str(),
                    ok = report.ok,
                    elapsed_ms = elapsed_ms(started),
                    "run finished"
                );
            }
            Err(e) => {
                logging::log_app_error(targets::RUN, "run", e);
            }
        }
        result
    }

    /// Run each agent with its own prompt (for per-agent chat context isolation).
    ///
    /// Emits [`RunEvent`]s through `on_event`. Uses parallel scheduling when
    /// `opts.mode == Parallel` and more than one job is ready.
    pub fn run_each(
        &self,
        jobs: &[(AgentId, String)],
        opts: &RunOptions,
        cancel: &CancelToken,
        on_event: &(dyn Fn(RunEvent) + Send + Sync),
    ) -> Result<MultiRunReport> {
        let started = Instant::now();
        let result = (|| {
            if jobs.is_empty() {
                return Err(AppError::InvalidArg("agent job list is empty".into()));
            }
            for (_, prompt) in jobs {
                if prompt.trim().is_empty() {
                    return Err(AppError::InvalidArg("prompt must not be empty".into()));
                }
            }

            let started_at = Utc::now().to_rfc3339();
            let mut resolved: Vec<(AgentId, Option<RunSpec>, Option<AgentRunResult>)> = Vec::new();
            for (id, prompt) in jobs {
                match self.resolve_job(*id, prompt, opts) {
                    Ok(ResolveOutcome::Ready(spec)) => resolved.push((*id, Some(spec), None)),
                    Ok(ResolveOutcome::Early(result)) => resolved.push((*id, None, Some(result))),
                    Err(e) => return Err(e),
                }
            }

            let results = match opts.mode {
                RunMode::Sequential => self.run_each_sequential(&resolved, opts, cancel, on_event),
                RunMode::Parallel => self.run_each_parallel(&resolved, opts, cancel, on_event),
            };

            let finished_at = Utc::now().to_rfc3339();
            let prompt_summary = jobs
                .iter()
                .map(|(id, p)| format!("{}:{}", id.as_str(), truncate_prompt(p, 40)))
                .collect::<Vec<_>>()
                .join(" | ");
            Ok(MultiRunReport::from_results(
                prompt_summary,
                opts.mode,
                results,
                started_at,
                finished_at,
            ))
        })();

        match &result {
            Ok(report) => {
                log_failed_agent_results("run_each", &report.results);
                tracing::info!(
                    module = targets::RUN,
                    op = "run_each",
                    jobs = jobs.len(),
                    mode = opts.mode.as_str(),
                    ok = report.ok,
                    elapsed_ms = elapsed_ms(started),
                    "run_each finished"
                );
            }
            Err(e) => {
                logging::log_app_error(targets::RUN, "run_each", e);
            }
        }
        result
    }

    fn resolve_job(&self, id: AgentId, prompt: &str, opts: &RunOptions) -> Result<ResolveOutcome> {
        let adapter = self.registry.get(id).ok_or_else(|| {
            AppError::NotFound(format!("adapter not registered for {}", id.as_str()))
        })?;
        let detect = adapter.detect();
        if detect.status != DetectStatus::Installed {
            if opts.skip_missing {
                return Ok(ResolveOutcome::Early(AgentRunResult::skipped(
                    id,
                    format!("agent {} not installed", id.as_str()),
                )));
            }
            return Err(AppError::NotFound(format!(
                "agent {} not installed",
                id.as_str()
            )));
        }
        let program = program_from_detect(detect.binary_path.as_deref(), id.as_str());
        let spec = match adapter.build_run_spec(&program, prompt, opts) {
            Ok(spec) => spec,
            // Desktop-only ZCode has no verified headless argv (`Unsupported`).
            // Skip that agent instead of aborting the rest of a multi-agent run.
            // Other spec errors (e.g. InvalidArg) still fail the whole batch so
            // chat resume can clear the native session id.
            Err(e) if opts.skip_missing && matches!(e, AppError::Unsupported(_)) => {
                return Ok(ResolveOutcome::Early(AgentRunResult::skipped(
                    id,
                    e.to_string(),
                )));
            }
            Err(e) => return Err(e),
        };
        if opts.dry_run {
            return Ok(ResolveOutcome::Early(AgentRunResult::dry_run(
                id,
                spec.display_command(),
            )));
        }
        Ok(ResolveOutcome::Ready(spec))
    }

    fn run_sequential(
        &self,
        jobs: &[(AgentId, Option<RunSpec>, Option<AgentRunResult>)],
        opts: &RunOptions,
    ) -> Vec<AgentRunResult> {
        let mut out = Vec::with_capacity(jobs.len());
        for (id, spec, early) in jobs {
            if let Some(r) = early {
                out.push(r.clone());
                continue;
            }
            let spec = spec.as_ref().expect("ready job has spec");
            let mut result = self.runner.run(spec, opts.timeout, opts.max_output_bytes);
            // Keep agent id stable even if runner forgets.
            result.agent = *id;
            out.push(result);
        }
        out
    }

    fn run_parallel(
        &self,
        jobs: &[(AgentId, Option<RunSpec>, Option<AgentRunResult>)],
        opts: &RunOptions,
    ) -> Vec<AgentRunResult> {
        // Pre-fill early results; spawn only real work.
        let mut results: Vec<Option<AgentRunResult>> = vec![None; jobs.len()];
        let mut work: Vec<(usize, AgentId, RunSpec)> = Vec::new();

        for (i, (id, spec, early)) in jobs.iter().enumerate() {
            if let Some(r) = early {
                results[i] = Some(r.clone());
            } else if let Some(spec) = spec {
                work.push((i, *id, spec.clone()));
            }
        }

        if !work.is_empty() {
            let runner = Arc::clone(&self.runner);
            let timeout = opts.timeout;
            let max_out = opts.max_output_bytes;
            thread::scope(|scope| {
                let mut handles = Vec::with_capacity(work.len());
                for (i, id, spec) in work {
                    let runner = Arc::clone(&runner);
                    let handle = scope.spawn(move || {
                        let mut r = runner.run(&spec, timeout, max_out);
                        r.agent = id;
                        r
                    });
                    handles.push((i, id, handle));
                }
                for (i, id, h) in handles {
                    match h.join() {
                        Ok(r) => results[i] = Some(r),
                        Err(_) => {
                            // Panic in worker must surface as hard failure, not skipped.
                            results[i] = Some(AgentRunResult {
                                agent: id,
                                status: RunStatus::Failed,
                                exit_code: None,
                                duration_ms: 0,
                                stdout: String::new(),
                                stderr: String::new(),
                                command: String::new(),
                                error: Some("worker thread panicked".into()),
                                truncated: false,
                                native_session_id: None,
                            });
                        }
                    }
                }
            });
        }

        results
            .into_iter()
            .enumerate()
            .map(|(i, r)| {
                r.unwrap_or_else(|| AgentRunResult {
                    agent: jobs[i].0,
                    status: RunStatus::Failed,
                    exit_code: None,
                    duration_ms: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                    command: String::new(),
                    error: Some("internal: missing result slot".into()),
                    truncated: false,
                    native_session_id: None,
                })
            })
            .collect()
    }

    fn run_each_sequential(
        &self,
        jobs: &[(AgentId, Option<RunSpec>, Option<AgentRunResult>)],
        opts: &RunOptions,
        cancel: &CancelToken,
        on_event: &(dyn Fn(RunEvent) + Send + Sync),
    ) -> Vec<AgentRunResult> {
        let mut out = Vec::with_capacity(jobs.len());
        for (id, spec, early) in jobs {
            if cancel.is_cancelled() {
                out.push(cancelled_result(*id));
                continue;
            }
            if let Some(r) = early {
                on_event(RunEvent::Started {
                    agent: *id,
                    command: r.command.clone(),
                });
                on_event(RunEvent::Finished { agent: *id });
                out.push(r.clone());
                continue;
            }
            let spec = spec.as_ref().expect("ready job has spec");
            on_event(RunEvent::Started {
                agent: *id,
                command: spec.display_command(),
            });
            let agent = *id;
            // Callback is `Fn` (not FnMut): session needs interior mutability.
            let session = std::sync::Mutex::new(StreamSession::new(agent, opts.process_mode));
            let mut result = self.streaming.run_streaming(
                spec,
                opts.timeout,
                opts.max_output_bytes,
                cancel,
                &|stream, text| {
                    let mut guard = match session.lock() {
                        Ok(g) => g,
                        Err(p) => p.into_inner(),
                    };
                    emit_stream_outputs(agent, guard.feed(stream, text), on_event);
                },
            );
            {
                let mut guard = match session.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                emit_stream_outputs(agent, guard.flush(), on_event);
                apply_structured_stdout(&mut result, &guard);
                guard.log_summary();
            }
            result.agent = *id;
            on_event(RunEvent::Finished { agent: *id });
            out.push(result);
        }
        out
    }

    fn run_each_parallel(
        &self,
        jobs: &[(AgentId, Option<RunSpec>, Option<AgentRunResult>)],
        opts: &RunOptions,
        cancel: &CancelToken,
        on_event: &(dyn Fn(RunEvent) + Send + Sync),
    ) -> Vec<AgentRunResult> {
        let mut results: Vec<Option<AgentRunResult>> = vec![None; jobs.len()];
        let mut work: Vec<(usize, AgentId, RunSpec)> = Vec::new();

        for (i, (id, spec, early)) in jobs.iter().enumerate() {
            if let Some(r) = early {
                on_event(RunEvent::Started {
                    agent: *id,
                    command: r.command.clone(),
                });
                on_event(RunEvent::Finished { agent: *id });
                results[i] = Some(r.clone());
            } else if let Some(spec) = spec {
                work.push((i, *id, spec.clone()));
            }
        }

        if !work.is_empty() {
            let work_meta: Vec<(usize, AgentId)> =
                work.iter().map(|(i, id, _)| (*i, *id)).collect();
            let (tx, rx) = std::sync::mpsc::channel::<RunEvent>();
            let streaming = Arc::clone(&self.streaming);
            let timeout = opts.timeout;
            let max_out = opts.max_output_bytes;
            let cancel = cancel.clone();

            let collected = thread::scope(|scope| {
                let mut handles = Vec::with_capacity(work.len());
                for (i, id, spec) in work {
                    let streaming = Arc::clone(&streaming);
                    let cancel = cancel.clone();
                    let tx = tx.clone();
                    let process_mode = opts.process_mode;
                    let handle = scope.spawn(move || {
                        let _ = tx.send(RunEvent::Started {
                            agent: id,
                            command: spec.display_command(),
                        });
                        let tx_chunk = tx.clone();
                        let session =
                            Arc::new(std::sync::Mutex::new(StreamSession::new(id, process_mode)));
                        let session_cb = Arc::clone(&session);
                        let mut r = streaming.run_streaming(
                            &spec,
                            timeout,
                            max_out,
                            &cancel,
                            &move |stream: OutputStream, text: &str| {
                                let mut guard = match session_cb.lock() {
                                    Ok(g) => g,
                                    Err(p) => p.into_inner(),
                                };
                                for out in guard.feed(stream, text) {
                                    let _ = tx_chunk.send(stream_output_to_event(id, out));
                                }
                            },
                        );
                        {
                            let mut guard = match session.lock() {
                                Ok(g) => g,
                                Err(p) => p.into_inner(),
                            };
                            for out in guard.flush() {
                                let _ = tx.send(stream_output_to_event(id, out));
                            }
                            apply_structured_stdout(&mut r, &guard);
                            guard.log_summary();
                        }
                        r.agent = id;
                        let _ = tx.send(RunEvent::Finished { agent: id });
                        (i, id, r)
                    });
                    handles.push((i, id, handle));
                }
                drop(tx);

                while let Ok(ev) = rx.recv() {
                    on_event(ev);
                }

                let mut out = Vec::with_capacity(handles.len());
                for (i, id, h) in handles {
                    match h.join() {
                        Ok(v) => out.push(v),
                        Err(_) => {
                            // Panic in worker must surface as hard failure (parity with run_parallel).
                            out.push((
                                i,
                                id,
                                AgentRunResult {
                                    agent: id,
                                    status: RunStatus::Failed,
                                    exit_code: None,
                                    duration_ms: 0,
                                    stdout: String::new(),
                                    stderr: String::new(),
                                    command: String::new(),
                                    error: Some("worker thread panicked".into()),
                                    truncated: false,
                                    native_session_id: None,
                                },
                            ));
                        }
                    }
                }
                out
            });

            for (i, id, r) in collected {
                results[i] = Some(r);
                let _ = id;
            }
            // Defensive: any work slot still empty after join handling.
            for (i, id) in work_meta {
                if results[i].is_none() {
                    results[i] = Some(AgentRunResult {
                        agent: id,
                        status: RunStatus::Failed,
                        exit_code: None,
                        duration_ms: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                        command: String::new(),
                        error: Some("worker thread panicked".into()),
                        truncated: false,
                        native_session_id: None,
                    });
                }
            }
        }

        results
            .into_iter()
            .enumerate()
            .map(|(i, r)| {
                r.unwrap_or_else(|| AgentRunResult {
                    agent: jobs[i].0,
                    status: RunStatus::Failed,
                    exit_code: None,
                    duration_ms: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                    command: String::new(),
                    error: Some("internal: missing result slot".into()),
                    truncated: false,
                    native_session_id: None,
                })
            })
            .collect()
    }
}

fn cancelled_result(agent: AgentId) -> AgentRunResult {
    AgentRunResult {
        agent,
        status: RunStatus::Cancelled,
        exit_code: None,
        duration_ms: 0,
        stdout: String::new(),
        stderr: String::new(),
        command: String::new(),
        error: Some("cancelled".into()),
        truncated: false,
        native_session_id: None,
    }
}

fn truncate_prompt(s: &str, max: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() <= max {
        return s;
    }
    let t: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{t}…")
}

enum ResolveOutcome {
    Ready(RunSpec),
    Early(AgentRunResult),
}

#[cfg(test)]
mod tests;
