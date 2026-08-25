---
title: UI 页面模式
type: reference
status: current
owner: maintainers
updated: 2026-08-25
---

# UI Page Patterns

> Status: current contract
> 
> This document defines the current navigation, page shells, and user workflows. Reusable visual and component rules live in [design-system.md](design-system.md). It describes the product surface as it exists now; future sidecar, tray, and modularity options are documented under [../proposals/](../proposals/README.md).

## 1. Navigation contract

The application is organized by work and management, with Agent filtering inside a page where it helps the task.

| Group | Label | Canonical path | Pattern |
|---|---|---|---|
| Workspace | Chat | `/chat` | Full-height conversation workbench |
| Workspace | Agents | `/agents` | Installed Agent catalog and lifecycle |
| Workspace | Skills | `/skills` | Shared/private skill inventory and preview |
| Workspace | MCP | `/mcp` | Read-only configuration inventory |
| Workspace | Projects | `/projects` | Project/session tree and read-only preview |
| Manage | Dashboard | `/` | Agent status, usage, and shortcuts |
| Manage | Connections | `/connections` | Global login list and connection actions |
| Manage | Routes | `/routes` | Local route runtime list and details |
| Manage | Settings | `/settings` | Preferences, local device, backups, and about |

`Routes` is shown in the sidebar by default. The `routesNavVisible` preference can hide the sidebar item, but it does not disable the page or change `/routes`. Usage is a Dashboard section; `/usage` redirects to `/?section=usage`. Backups are a Settings tab; `/backups` redirects to `/settings?tab=backups`.

The compatibility paths `/adapter` and `/router` replace-navigate to `/routes`. They are recovery paths for existing links, not current navigation labels.

## 2. Application shell

### 2.1 Standard shell

The standard shell has an 8px canvas gutter, a rounded sidebar panel, a rounded main panel, and a top bar. The main column uses the edge-column pattern with an 18px horizontal inset. A standard page is composed in this order:

```text
PageHeader
  -> chrome / chromeRow (tabs, filters, Agent strip, or actions)
  -> lead (environment status or one Notice)
  -> stack / blocks (main content)
  -> PageSection / ruled section where a real boundary is needed
```

The page title is one line plus one short metadata line. Do not repeat the same explanation in a card immediately below the title.

### 2.2 Full-height workbench

Chat, Skills, and Projects manage their own vertical scrolling and use `fullBleed`. Full-height does not create a third content width: Chat messages use the reading column, while Skills and Projects use the edge column with a split preview surface. The workbench header uses the compact page-header rhythm.

### 2.3 Settings reading page

Settings keeps the standard shell and a reading-column content area. The four page tabs are:

| Tab | Query | Contents |
|---|---|---|
| Preferences | `?tab=preferences` | Language, theme, startup, close-to-tray, Routes visibility, skill source, usage interval |
| This device | `?tab=local` | Data directory, log level, retention, log directory |
| Backups | `?tab=backups` | Agent live configuration snapshots and restore/delete |
| About | `?tab=about` | Version, update check, repository, and read-only credential-storage notes |

Invalid or old tab values replace to the nearest current tab. Tab changes use `replace` so normal navigation history does not fill with panel changes.

## 3. Shared page behavior

### 3.1 Agent filtering

Use `AgentTabStrip` where content is naturally scoped by Agent: Connections, Skills, Projects, and Backups. Installed Agents appear first; hidden Agents do not occupy the default strip unless they have recoverable data. Do not turn every page into an Agent-first two-level navigation.

### 3.2 Four states

Every independently loaded page or block implements loading, empty, error, and partial states. A failed usage parser must not remove Dashboard status cards. An unavailable Agent must be shown as unavailable or partial, not silently converted to mock data.

### 3.3 Route and deep-link behavior

- Use canonical paths for links and navigation.
- Preserve safe query parameters when replacing compatibility paths.
- A missing detail ID leaves the user on the list with no misleading success toast.
- A successful mutation refreshes the owning page through its backend façade; a refresh error says “已完成，但刷新失败” rather than “未完成”.

## 4. Dashboard

Dashboard is the overview for installed Agents and usage, not a second Connections or Routes workbench.

- Render only installed Agents. Use an auto-fit grid so the number of Agents is not encoded in the layout.
- Agent cards show identity, readiness, and one primary “连接 / 切换” entry. Do not duplicate management buttons already present in Agents or Connections.
- Usage filters are shared by summary metrics, trend, distribution, and details: time, Agent, and model. Model options are the distinct models in the selected records, not a model-management catalog.
- Usage collection is explicit and shows last/next sync. A parser health block is compact and partial; it names the affected Agent and keeps the rest of the dashboard usable.
- A usage-empty state guides the first manual collection. Routes health-empty is the exception described below.

## 5. Connections

Connections is a global login list. It is not a list of generated route providers and it does not expose internal binding implementation names.

- The top `AgentTabStrip` filters the list. Do not add a second row of “official / API key / unknown” filter chips.
- OAuth rows use an identity/person icon; API key rows use a key icon. The icon has an accessible label and a short hint.
- The row actions are **分享** and **路由**. The destination action opens the shared ConnectFlow dialog with source and target context fixed by the entry point.
- ConnectFlow explains one of four outcomes: **直连**, **用这份登录**, **本机路由**, or **当前不支持**. The explanation is a user outcome, not a protocol number.
- A disabled destination retains the reason and offers the appropriate recovery path. Missing data and a genuinely empty login list are different states.
- Connections never shows a secret in a row, badge, tooltip, or diagnostic copy.

## 6. Routes

Routes is the runtime management page for local loopback forwarding. It is not a general connection-binding editor.

### 6.1 List

The list is an edge-column management table with stable rows. Each row shows the route target, runtime state, loopback address (`127.0.0.1` and port when available), upstream summary, and the permitted actions. A row can be opened through `?profile=<id>`.

The page treats the following states separately:

| State | Meaning | UI |
|---|---|---|
| Running | Listener and route are available | Address, port, health, and stop action |
| Starting / stopping | Lifecycle mutation is in progress | Busy state, stable row, dismissal guarded |
| Degraded | Listener exists but the last upstream check failed | Warning state plus retry/diagnostics |
| Stopped | Durable route exists but is not running | Start action |
| Host unavailable | The current runtime host cannot be reached | Explicit unavailable error; never “running” and never silent mock |
| Healthy empty | No local route is configured | Informational empty state without a conversion CTA |

Do not infer “running” from a durable database row when the runtime host is unavailable. Do not use an account or generated provider badge as a substitute for route health.

### 6.2 Detail

The detail panel is a focused dialog or side surface opened from the list. It shows route identity, loopback address and port, downstream surface, upstream summary, last health result, and member health when the runtime reports members. It never shows local bearer values or refresh credentials.

The primary runtime actions are start, stop, retry, and remove/unbind where the product flow permits them. A stop or unbind confirmation explains listener impact and whether the current live configuration will be restored. A failed unbind remains retryable; it must not fall back to force deletion.

### 6.3 Runtime boundary

The current `local_bridge` runtime is hosted in the Tauri process through the in-process control host. Routes may report unavailable when that host is not reachable. A future sidecar is a proposal and is not a current UI assumption.

## 7. Chat

Chat is a one-conversation, one-Agent workbench with a session rail, transcript, process panel, and composer.

- The rail supports new conversation, search by title and working directory, day grouping, selection, rename, and delete confirmation.
- The current conversation header exposes Agent identity, working directory, automatic-approval state, and connection context. A missing working directory is a blocker, not an automatic modal.
- A conversation has one active Agent. Hidden or unauthorized Agents remain visible with a reason but cannot be selected for a new send.
- The composer validates blockers in order: hidden Agent, missing authorization, missing working directory, then another conversation currently sending. It renders only the first blocker with a recovery action.
- The send button is the page's one accent action. Sending changes it to a stop action. Retry creates a new turn using the same validation path.
- Streaming process details use a compact summary and an expandable timeline. Commands, stderr, and exit codes stay in a secondary runtime-details disclosure.
- Switching conversations clears in-memory process buffers for the old view but does not cancel the active operation. The target conversation shows a “sending elsewhere” recovery line.
- Copy is available for completed user/Agent messages. Running messages do not show copy or retry.

## 8. Skills and Projects

Both are full-height workbenches with a left inventory and an optional right preview.

### Skills

- Library and Market are page-level tabs. Filtering and Agent scope stay in the chrome row.
- A skill name opens the preview; Enter is equivalent. Checkbox selection is only for batch operations and never opens the preview.
- The preview identity is separate from checkbox selection. It remains open when filters hide the selected skill, with a short source label in the header.
- The list keeps the name and at most one line of description. Absolute paths move to the preview footer or an explicit open-directory action.
- The matrix represents supported/unavailable/unknown states without blanking the page. A missing skill directory is a partial state, not a global error.

### Projects

- The left tree contains projects and expandable sessions; the right panel is a read-only excerpt preview.
- Search covers project and session names. Delete and summarize are session actions and require confirmation where supported.
- A project/session can bootstrap a new Chat conversation through the documented session storage handoff. It does not silently edit the original Agent log.
- Agent capabilities such as transcript support are explicit. Unsupported actions are hidden or disabled with a hint.

## 9. Agents and MCP

### Agents

Agents is the lifecycle surface: installed state, runtime readiness, install/update, and environment remediation. A missing runtime is shown before Agent installation, with repair steps and a re-detect action. Do not offer a successful installation action while its prerequisite environment is known to be missing.

### MCP

MCP is a read-only inventory of known configuration files. It lists Agent, server, transport, source path, and enabled status. Parse errors, missing files, and an empty inventory each get their own recoverable state. Inventory does not imply that editing or injection is supported.

## 10. Responsive and interaction constraints

- Use stable grid tracks (`auto-fit`/`minmax`) for Agent cards, tables, toolbars, and preview panes.
- On narrow windows, wrap labels and metadata instead of shrinking type or allowing overlap. Icon-only actions remain available through an overflow menu when the row cannot fit.
- A split preview has a focusable separator. Keyboard adjustment moves it in fixed increments; double-click restores the default width. Dragging must not select the document body.
- Escape closes the topmost dialog/menu/popover before closing a preview. Focus order follows content, row actions, separator, preview tools, then document body.
- Page-specific user copy belongs with the page pattern or locale dictionary. Do not embed implementation phase labels in visible UI.

## 11. Current implementation references

- Layout and routing composition: `src/App.tsx`, `src/components/layout/`, and `src/pages/`.
- Shared component rules: `src/components/ui/`, `src/components/shared/`, and [design-system.md](design-system.md).
- Backend access: `src/lib/api/`, `src/lib/backend/contracts/`, and `src/lib/backend/tauri/`.
- Route control: the backend/control contract and the current in-process Tauri host. The proposal for a separate process is deliberately outside this current page contract.

When code and this document disagree, verify the current implementation and update this current contract in the same change. Do not revive a completed redesign document as a new task list.
