---
title: UI 页面模式
type: reference
status: current
owner: maintainers
updated: 2026-08-30
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
| Workspace | Skills | `/skills` | User skills (shared library + this-tool), project skills by workspace, and market |
| Workspace | MCP | `/mcp` | Read-only configuration inventory |
| Workspace | Projects | `/projects` | Project/session tree and read-only preview |
| Workspace | Plugins | `/plugins` | Read-only vendor plugin / extension pack inventory |
| Manage | Dashboard | `/` | Agent status, usage, and shortcuts |
| Manage | Connections | `/connections` | Global login list and connection actions |
| Manage | Routes | `/routes` | Local route runtime list and details |
| Manage | Settings | `/settings` | Preferences, local device, backups, and about |

`Routes`, `Plugins`, and `MCP` are in development. New installs hide the Routes and Plugins sidebar entries (`routesNavVisible` / `pluginsNavVisible` default off). Turning the setting on shows those entries; the pages stay reachable at `/routes` and `/plugins`. MCP stays in the workspace nav. Settings (Routes / Plugins), the page titles, and the sidebar entries (when shown) carry an in-development mark. Usage is a Dashboard section; `/usage` redirects to `/?section=usage`. Backups are a Settings tab; `/backups` redirects to `/settings?tab=backups`. Install / enable / uninstall for plugin packs is still a [proposal](../proposals/plugin-management.md); the current page is read-only.

The compatibility paths `/adapter` and `/router` replace-navigate to `/routes`. They are recovery paths for existing links, not current navigation labels.

## 2. Application shell

### 2.1 Standard shell

The standard shell has an 8px canvas gutter (`pageEdge.canvas`), a rounded sidebar panel, a rounded main panel, and a top bar. The main column uses the edge-column pattern with a shared horizontal inset (`pageEdge.inset`, currently 8px). Non-chat pages put the page title on the left of the top bar as one line: the page name in the title size and primary color, then a short description in the meta size and secondary color. The notification control stays on the right. Chat has no top bar and owns its session name. A standard page is composed in this order:

```text
TopBar (title + metadata | notification)
  -> chrome / chromeRow (tabs, filters, Agent strip; page commands on the right of the same row)
  -> lead (environment status or one Notice)
  -> stack / blocks (main content)
  -> PageSection / ruled section where a real boundary is needed
```

The page title is a single line: name, then short description. Distinguish them with type size and color, not a second row. Do not repeat the same explanation in a card immediately below the title. Do not keep a second title block in the page body.

### 2.2 Full-height workbench

Chat, Skills, Projects, Plugins, Connections, Routes, and Settings use `fullBleed` and manage their own vertical scrolling. Full-height does not create a third content width: Chat messages use the reading column; Skills, Projects, Plugins, Connections, and the Settings backups tab use the edge column with a split preview surface. Page-level commands stay in the list column, on the right of the same row as tabs or filters. They do not occupy a row of their own. The workbench list and the preview column share the same `pageEdge.inset` top and bottom so both edges line up. The page title itself stays in the top bar.

### 2.3 Settings

Settings uses the workbench header and four page tabs; the tab row stays at the top-left of the workbench header. Preferences, This computer, and About center their content on the reading column. Backups is a left-right workbench: the list is on the left, a file inspect panel opens on the right.

| Tab | Query | Contents |
|---|---|---|
| Preferences | `?tab=preferences` | Language, theme, startup, close-to-tray, Routes visibility, Plugins visibility, skill source, usage interval |
| This computer | `?tab=local` | Data directory, log level, retention, log directory |
| Backups | `?tab=backups` | Agent configuration snapshots; keep-copies switch; restore/delete; file inspect |
| About | `?tab=about` | Version, update check, repository, and read-only credential-storage notes |

Invalid or old tab values replace to the nearest current tab. Tab changes use `replace` so normal navigation history does not fill with panel changes. The backups keep-copies switch (`keepLiveFileCopies`, default on) copies each Agent's live files into the backup directory on switch/import; turning it off stops piling historical copies, but the current switch still keeps one copy for rollback. Manual backups are unaffected. The backup list identity stays a short label (email or key tail). The file preview shows the snapshot as stored.

## 3. Shared page behavior

### 3.1 Agent filtering

Use `AgentTabStrip` where content is naturally scoped by Agent: Connections, Skills, Projects, Plugins, and Backups. Installed Agents appear first; hidden Agents do not occupy the default strip unless they have recoverable data. Do not turn every page into an Agent-first two-level navigation.

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

Connections is a global login list in a full-height workbench split. It is not a list of generated route providers and it does not expose internal binding implementation names.

- The top `AgentTabStrip` filters the list. Do not add a second row of “official / API key / unknown” filter chips.
- The add menu is **导入授权** / **官方登录** / **添加 API Key**. Official login and API Key are stored as separate rows. WorkBuddy custom models and ZCode catalog providers split into one login per directory row; desktop package logins are not imported.
- OAuth rows use an identity/person icon; API key rows use a key icon. The icon has an accessible label and a short hint.
- Selecting a row opens the right-hand detail: related config files (copyable, open-directory), package, expiry, timeline, and the full endpoint. The list uses masked labels; the file preview shows the stored snapshot.
- The official-login wait page does not show internal status or login file paths; failure keeps **重试** as the primary action.
- The row actions are **分享** and **路由**. The destination action opens the shared ConnectFlow dialog with source and target context fixed by the entry point.
- ConnectFlow explains one of four outcomes: **直连**, **用这份登录**, **本机路由**, or **当前不支持**. The explanation is a user outcome, not a protocol number.
- A disabled destination retains the reason and offers the appropriate recovery path. Missing data and a genuinely empty login list are different states.

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

The detail panel is a focused dialog or side surface opened from the list. It shows route identity, loopback address and port, downstream surface, upstream summary, last health result, default-pool members, and the listed models the resolver currently serves. It never shows the local token value or refresh credentials.

Official `native_endpoint` / `config_sync` rows are not auto-enrolled. When `plan()` still allows a local-bridge write, the detail offers **交给本机网关**. Connections remains the login list; Routes does not become a second place to add credentials.

The primary runtime actions are start, stop, retry, and remove/unbind where the product flow permits them. A stop or unbind confirmation explains listener impact and whether the current local configuration will be restored. A failed unbind remains retryable; it must not fall back to force deletion.

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

## 8. Skills, Projects, and Plugins

Skills, Projects, and Plugins are full-height workbenches with a left inventory and an optional right preview.

### Skills

- User skills, Project skills, and Market are page-level tabs. Filtering and Agent scope stay in the chrome row.
- User skills list the shared library plus this-tool-only skills, with the enablement matrix.
- Project skills use a dropdown of workspaces already identified on the Projects page. After a project is selected, skills can be added or deleted for that workspace (canonical folder `.agents/skills`).
- A skill name opens the preview; Enter is equivalent. Checkbox selection is only for batch operations and never opens the preview.
- The preview identity is separate from checkbox selection. It remains open when filters hide the selected skill, with a short source label in the header.
- The list keeps the name and at most one line of description. Absolute paths move to the preview footer or an explicit open-directory action.
- The matrix represents supported/unavailable/unknown states without blanking the page. A missing skill directory is a partial state, not a global error.

### Projects

- The left tree is still a stack of collapsible project cards. Sessions under a project align in columns (title, file name, time, size, icon actions) without row dividers. Title opens the right-hand excerpt preview; the file-name field reveals the record in the file manager. Page actions (summarize, delete, refresh) stay in the list column, left of the separator, and travel with it while resizing.
- Search covers project and session names. Delete and summarize are session actions and require confirmation where supported.
- A project/session can bootstrap a new Chat conversation through the documented session storage handoff. It does not silently edit the original Agent log.
- Agent capabilities such as transcript support are explicit. Unsupported actions are hidden or disabled with a hint.

### Plugins

- Left column lists installed plugin / extension packs (Claude and Grok today). Clicking a row opens the right-hand details pane.
- Details show name, marketplace, version, scope, enabled, trusted, path, and components. Bundled MCP is a component, not a list row.
- Loading, empty, and error states stay in the list column. There is no install button. Hiding the sidebar item does not disable `/plugins`.

## 9. Agents and MCP

### Agents

Agents is the lifecycle surface: installed state, runtime readiness, install/update, and environment remediation. A missing runtime is shown before Agent installation, with repair steps and a re-detect action. Do not offer a successful installation action while its prerequisite environment is known to be missing.

### MCP

MCP is a read-only inventory of known **MCP server** configuration files. It lists Agent, server, transport, source path, and enabled status. Parse errors, missing files, and an empty inventory each get their own recoverable state. Inventory does not imply that editing or injection is supported, and it is not the plugin/extension pack manager. The current page is a standard single-column table. Plugin / extension packs live on `/plugins`.

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
