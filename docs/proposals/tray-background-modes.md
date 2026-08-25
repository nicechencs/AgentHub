---
title: Tray Background Modes
type: proposal
status: proposed
owner: maintainers
updated: 2026-08-25
---

# Tray Background Modes

> Status: proposed
> 
> This is a future behavior candidate. The current close-to-tray behavior remains the contract until a mode is implemented, tested, and documented as current.

## 1. Current baseline

Closing to the tray currently hides the window while the Tauri process, WebView, React tree, timers, and in-process route host remain alive. This preserves fast restore but does not materially reduce WebView memory. Route ownership is independent of whether the window is visible.

## 2. Candidate user model

The setting could eventually offer two or three modes:

| Mode | Candidate behavior | Trade-off |
|---|---|---|
| Standard background | Hide the window and keep the current process | Fast restore, little memory reduction |
| Power-saving background | Hide and pause eligible frontend polling/timers; refresh on visible | Lower background CPU, refresh latency on wake |
| Deep low-memory background | Destroy the WebView/window and keep Rust/tray state | Largest memory reduction, cold UI rebuild and state restoration |

The product may expose only “隐藏界面” and “低内存后台”, with the middle mode as an implementation detail. The choice must not alter whether a local route is configured or running.

## 3. Candidate design

### Settings and policy

Add a normalized `background_mode` setting only after the current close policy has a stable pure decision function and tests. Unknown values fall back to the safe standard mode. Decide residency first (`close_to_tray` or an active route), then choose hide versus destroy.

### Tauri lifecycle

Converge close and tray actions through one policy function. A deep mode destroys only the window/WebView; it does not use the process exit path and does not stop the in-process route host. Rebuilding the window must defer navigation until WebView ready and preserve a pending route when creation is asynchronous.

### Frontend lifecycle

Visibility events may pause the global tick, route health polling, and usage synchronization. Returning visible triggers one bounded refresh. Event subscription remains behind the backend façade; pages do not call `invoke` directly.

### Failure and recovery

- If WebView suspension is unavailable, degrade to ordinary hide.
- If window rebuild fails, keep the tray process alive and expose an explicit reopen/retry action.
- App update/restart remains a process-level operation and must be tested with both hidden and rebuilt-window states.
- Pending navigation is single-valued and replaced by the latest safe path.

## 4. Evaluation slices

1. Extract and test the close/residency policy without changing behavior.
2. Add pause/resume events and refresh accounting for the power-saving mode.
3. Prototype window destruction/rebuild behind an internal setting and verify navigation, localization, pending work, and update flows.
4. Measure memory and CPU on supported Windows configurations before considering a user-facing choice.

## 5. Acceptance gates

- No route listener is stopped merely because the window is hidden or rebuilt.
- Reopening restores the last safe page and does not drop a pending tray navigation.
- Frontend polling is paused exactly once and resumed with one refresh, without duplicate timers.
- Close, tray, update, restart, and explicit exit have distinct tests.
- Unsupported platform behavior is explicit and safe.

## 6. Explicit exclusions

This proposal does not create an operating-system service, change the local route process boundary, add credential encryption, or add domestic OAuth/API conversion. Those are outside this candidate.
