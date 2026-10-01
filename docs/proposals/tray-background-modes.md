---
title: Tray Background Modes
type: proposal
status: proposed
owner: maintainers
updated: 2026-09-29
---

# Tray Background Modes

> Status: proposed. The current close-to-tray behavior stays the contract until a new mode is implemented and documented as current.

## 1. Current baseline

- Close goes through the pure policy `decide_close_action` (`src-tauri/src/window_policy.rs`, tests in `window_policy/tests.rs`): tray 退出 exits; otherwise hide when `close_to_tray` is on **or** a local route is running.
- Hiding keeps the Tauri process, WebView, React tree, and in-process route host alive. Restore is fast; WebView memory is not reduced.
- Usage sync already pauses its timer when the page is hidden and reschedules on visible (`src/components/shared/UsageSyncProvider.tsx`). Other polling (route health, global tick) does not.
- There is no `background_mode` setting and no window destroy/rebuild path.

## 2. Candidate modes

| Mode | Behavior | Trade-off |
|---|---|---|
| Standard | Hide window, keep process (today) | Fast restore, little memory saved |
| Power-saving | Hide and pause eligible polling; refresh once on visible | Less CPU, short refresh on wake |
| Low-memory | Destroy WebView/window, keep Rust and tray | Most memory saved, cold UI rebuild |

The UI may show only “隐藏界面” and “低内存后台”. No mode changes whether a route is configured or running.

## 3. Design constraints

- `background_mode` is normalized; unknown values fall back to Standard. Decide residency first (existing policy), then hide vs destroy.
- Low-memory destroys only the window. It does not use the exit path or stop the route host.
- Rebuild waits for WebView ready before navigating; a pending tray navigation is single-valued, latest safe path wins.
- Visibility pause/resume stays behind the backend façade; pages do not call `invoke`.
- If suspend is unsupported, degrade to Standard. If rebuild fails, keep the tray alive with a reopen/retry action.

## 4. Remaining slices

1. ~~Extract and test the close policy.~~ Done (`window_policy.rs`).
2. Pause/resume for route health and global tick, one refresh on return (usage sync already does this).
3. Prototype window destroy/rebuild behind an internal setting; verify navigation, locale, pending work, update.
4. Measure memory and CPU on supported Windows setups before any user-facing option.

## 5. Gates

- Hiding or rebuilding never stops a route listener.
- Reopen restores the last safe page and keeps a pending tray navigation.
- Polling pauses once and resumes with one refresh; no duplicate timers.
- Close, tray, update, restart, and explicit exit each have tests.
- Unsupported platforms behave explicitly and safely.

## 6. Non-goals

OS services, changing the route process boundary, credential encryption, domestic OAuth/API conversion.
