---
title: UI 页面模式
type: reference
status: current
owner: maintainers
updated: 2026-10-04
---

# UI Page Patterns

This page is the source of truth for navigation, page shells, and per-page workflows. Visual and component rules live in [design-system.md](design-system.md). Per-Agent Chat channels and approval mechanics live in [Chat 与 Agent](../concepts/chat-and-agents.md); cross-page implementation facts live in [STATUS](../STATUS.md). Future sidecar, tray, and modularity options are [proposals](../proposals/README.md), not part of this contract.

Each page section lists its behavior, its **Agent touchpoints** (what a new Agent must be checked against, see [添加 Agent](../guides/adding-an-agent.md#5a-页面触点)), and what it is **not**.

## 1. Navigation

| Group | zh label | Path | Page |
|---|---|---|---|
| 工作区 | 对话 | `/chat` | Full-height conversation workbench |
| 工作区 | Agent | `/agents` | Agent catalog and lifecycle |
| 工作区 | 技能 | `/skills` | User skills, project skills, market |
| 工作区 | MCP | `/mcp` | MCP server inventory; write/enable for Claude / Codex / Grok / Cursor / WorkBuddy |
| 工作区 | 历史 | `/projects` | Local sessions by workspace (internal name: Projects) |
| 工作区 | 插件 | `/plugins` | Installed plugin / extension packs |
| 管理 | 总览 | `/` | Agent status, usage, and the connect dialog |
| 管理 | 连接 | `/connections` | General login list |
| 管理 | Sub2API | `/sub2api` | Sub2API site sign-in and key management |
| 管理 | 路由 | `/routes` | Local forwarding runtime and the connection pool |
| 管理 | 设置 | `/settings` | Preferences, features, this computer, backups, about |

Sidebar visibility (`src/lib/ui-preferences.ts`):

- 对话, Agent, 总览, and 设置 always show. The rest can be toggled in 设置 → 功能.
- New installs show everything except 插件 and Sub2API (`pluginsNavVisible` / `sub2apiNavVisible` default off). The first-run guide can turn 路由 / Sub2API on or off from the user's usage choice.
- Hiding an entry never disables its path. While the URL is inside `/routes*`, 路由 stays visible in the sidebar even if hidden, without changing the preference.
- 插件 carries an **开发中** badge.

Compatibility redirects (`src/App.tsx`) replace-navigate and keep safe query parameters. They are not current labels:

| Old path | Goes to |
|---|---|
| `/adapter`, `/router`, `/bridges` (legacy) | `/routes` |
| `/providers`, `/accounts` | `/connections` |
| `/usage` | `/?section=usage` |
| `/backups` | `/settings?tab=backups` |
| `/routes/sub2api` | `/sub2api` |

Routes secondary nav (shown on every `/routes*` path):

| zh label | Path | Role |
|---|---|---|
| 看板 | `/routes/board` | Endpoint overview, usage, and the single local-gateway start/stop. Bare `/routes` redirects here |
| 连接池 | `/routes/pool` | Logins used for local forwarding; `?profile=` opens detail |
| 入口 Key | `/routes/tokens` | Entry keys per endpoint; appear after the gateway starts |
| 监控 | `/routes/activity` | Recent request feed |

- Clicking 路由 in the primary sidebar collapses that sidebar when **Collapse sidebar on Routes** is on (in 设置 → 功能; default on; writes `agenthub:sidebar-collapsed`). Nothing else auto-expands or auto-collapses it.
- The secondary nav's top-right control collapses it (`agenthub:routes-nav-collapsed`); right-click offers the opposite action.
- The primary sidebar (expanded or icon rail) and the Routes rail share one selected / hover / collapse chrome.

## 2. Application shell

- The bottom `StatusBar` is application chrome: installed Agents on the left, local-forward status on the right (click opens the Routes board).
- Standard shell: 12px canvas gutter (`pageEdge.canvas`), a rounded sidebar panel, a rounded main panel, and a top bar. Page edges use `pageEdge.inset` (12px) from `src/components/layout/page-rhythm.ts`.
- Non-chat pages put a one-line title on the left of the top bar: page name in title size and primary color, then a short description in meta size and secondary color. Help and feedback sit on the right. There is no notification bell. Do not repeat the title or its explanation in the page body.
- Chat has no top bar and owns its session name.

A standard page is composed in this order:

```text
TopBar (title + description | help + feedback)
  -> chrome / chromeRow (tabs, filters, Agent strip; page commands on the right of the same row)
  -> lead (environment status or one Notice)
  -> stack / blocks (main content)
  -> PageSection where a real boundary is needed
```

Full-height workbench: Chat, Agents, Skills, Projects, Plugins, Connections, Sub2API, Routes, and Settings use `fullBleed` and own their vertical scroll. `fullBleed` is not a width system:

- Chat messages use the Chat content column (see [design-system.md §3.4](design-system.md#34-content-widths)).
- Dashboard, the Routes board, and Settings forms (except backups) use the overview column.
- Everything else uses the edge column, with a split preview where the page has one.
- Page commands sit on the right of the tabs/filter row, never on a row of their own. List and preview columns share the same top and bottom inset.

## 3. Shared behavior

- **Agent filtering.** Use `AgentTabStrip` where content is naturally scoped by Agent: Connections, Skills, Projects, Plugins, Backups. Installed Agents first; hidden Agents only appear when they have recoverable data. Do not make every page Agent-first.
- **Four states.** Every independently loaded page or block has loading, empty, error, and partial states. One failed parser or unavailable Agent never blanks the page and is never replaced by mock data.
- **Deep links.** Use current paths. A missing detail ID leaves the user on the list without a success toast.
- **After a write.** Refresh the owning page through its backend façade. If the write succeeded but the refresh failed, say so (e.g. 已切换，但列表刷新失败), never “未完成”.
- **Open detail.** Name-click tables open detail from the name; while a detail pane is open, clicking another row's empty area switches it; a closed pane stays closed. Details: [design-system.md §4.4](design-system.md#44-list-table-and-inspect).

## 4. Dashboard (总览)

The overview for installed Agents and usage. Not a second Connections or Routes workbench.

- Auto-fit cards for **installed** Agents only; the layout does not encode the Agent count. A card shows identity and readiness. Clicking an installed Agent's card opens the connect dialog when that Agent supports login management (`ConnectFlowDialog`, also reachable via `/?connect=`), which offers 直连 / 用这份登录 / 本机转发 / 当前不支持. A card whose install or environment is not ready links to Agents instead; an Agent without login management is not clickable.
- Usage filters (time, Agent, model) drive summary, trend, distribution, and details together. The trend switches between Agent (area) and Model (line); hover shows tokens then cost, the day's total, running total, and each series' share. Model options are the models present in the selected records. Filters survive leaving and returning within one app run.
- Usage collection is explicit and shows last/next sync. A compact parser-health block names the affected Agent. An empty usage state guides the first manual collection.

**Agent touchpoints:** Usage parsers per Agent; route `plan` / bind outcomes for the connect dialog; catalog + install/detect for card state.

**Not:** a login list, Routes start/stop, or Skills / Plugins / MCP / Sub2API management.

## 5. Connections (连接)

The general login list, in a full-height split. Logins created or imported here are Connections-managed, including ones later synced into the connection pool.

- `AgentTabStrip` filters the list. No second row of “official / API key” chips.
- **添加登录** menu: **官方登录** / **添加 API Key** (Cursor has no API Key form). **导入本机登录** appears only when 设置 → 偏好 → **自动导入本机登录** is off (default on). When import is shown, each menu row has a one-line description and import is highlighted if a login was found on this computer.
- Official logins and API Keys are separate rows. WorkBuddy custom models and ZCode catalog providers become one login per directory row. Desktop package logins are not imported.
- Official rows use a person icon, API Key rows a key icon, each with an accessible label.
- Click the login **name** to open detail: a quiet table without section titles or column headers, in this order: status, usage, where it connects and models, who is using it, related files and records. Record ID and import source sit under a collapsed **更多**. Labels are masked; the file preview shows the stored snapshot.
- The official-login wait page hides internal status and file paths; on failure **重试** is the primary action.
- The row menu only has **取消添加** (when the login is already written into a tool).
- Missing data and a genuinely empty list are different states. The recycle bin restores to Connections only.

**Agent touchpoints:** AccountSwitch / ApiKeyAccount / ConfigWrite when a login is written into a tool (取消添加 reverses it); per-Agent import/probe; occupancy (exclusive slot vs directory-append for WorkBuddy / ZCode).

**Not:** the connect dialog (Dashboard), pool enrollment (**从连接同步** on the pool page), **分享至连接池** / **用到其他工具** / **本机转发** actions, route-only `home=route_pool` logins, or gateway start/stop.

## 6. Routes (路由)

Runtime management for local loopback forwarding. Not a general connection editor. The `local_bridge` runtime runs inside the Tauri process; a separate sidecar is a proposal and not a UI assumption. When that host cannot be reached, Routes shows unavailable.

### 6.1 Board (看板)

- Four endpoint cards: Messages, Responses · Codex, Responses · Grok, Chat completions. Codex and Grok share `/v1/responses` on the wire; the cards split them and filter usage. They are not per-endpoint switches.
- Usage charts and **one** start/stop control for the shared local gateway.

**Agent touchpoints:** gateway readiness for Agents using entry keys; board usage is route-side telemetry, not Agent Usage parsers.

**Not:** a login list, entry-key management, or the request feed.

### 6.2 Connection pool (连接池)

- Lists official logins and API Keys used for local forwarding in a field-aligned table.
- **Who can join via 从连接同步** (`canSyncConnectionToPool` in `src/components/login-kernel/eligibility.ts`): every API Key regardless of owning Agent; official logins only for Claude / Codex / Grok; domestic official logins never. Logins already in the pool are skipped.
- The pool can also add its own official login / API Key. These use `home=route_pool`, may not appear in Connections, and their whole lifecycle stays in Routes.
- A login synced from Connections stays Connections-managed until the user **编辑** it here: saving copies it to a pool-owned row (the Connections original stays), then asks **同步到连接页？** to write models back.
- Removing a pool member never deletes the Connections login. Connections and the pool have separate recycle bins; each restores to its own page.
- Columns: login, type, and status always; connection count, usage window, last used, and priority only when some row has a value; enable switch last. Column widths are dragged from the header edge and remembered.
- Click the login **name** for detail (same headerless table as Connections; model lists longer than 8 show 8 plus expand). The enable switch does not open detail. `?profile=<id>` opens a row directly.

Runtime states are shown separately:

| State | Meaning | UI |
|---|---|---|
| Running | Listener and route available | Address, port, health |
| Starting / stopping | Lifecycle change in progress | Busy state, stable row, dismissal guarded |
| Degraded | Listener up, last upstream check failed | Warning plus retry/diagnostics |
| Stopped | Route saved but not running | Board shows start; leftover route cards may still offer start |
| Host unavailable | Runtime host unreachable | Explicit unavailable error; never “running”, never mock |
| Healthy empty | No local route configured | Informational empty state, no conversion CTA |

Never infer “running” from a saved database row when the host is unavailable, and never use an account or generated provider badge as route health.

**Agent touchpoints:** route `plan` / `bind` / `unbind`; pool membership feeds the default pool and `GET /models`; sync-back writes models through ConfigWrite when confirmed.

**Not:** gateway start/stop (board) or entry keys (入口 Key).

### 6.3 Route detail

- Opened from the pool. Shows route identity, loopback address and port, endpoint type, upstream summary, last health, default-pool members, and the models currently served. Never shows the entry key value or refresh credentials.
- Official `native_endpoint` / `config_sync` rows are not auto-enrolled. When `plan()` still allows a local-forward write, detail offers **改用本机转发**.
- Stop or unbind confirmation explains listener impact and whether the local configuration will be restored. A failed unbind stays retryable and never falls back to force-delete.

**Agent touchpoints:** `plan()` gate for enrollment; the model list is resolver output, not the ModelSelect capability.

### 6.4 Entry keys (入口 Key)

- Entry keys per endpoint; they appear after the gateway starts. Create, copy, or write a key into the matching installed Agent (API Key style).
- Endpoint type must match the Agent's surface (Messages / Responses · Codex / Responses · Grok / Chat completions).

**Agent touchpoints:** ApiKeyAccount / ConfigWrite / AccountSwitch when writing a key into an Agent.

**Not:** upstream login or Sub2API key management; gateway start/stop; pool edits.

### 6.5 Activity (监控)

- Recent requests across routes with filters. Opening a row shows the trace: stage timeline, inbound/outbound endpoints, and key.
- No bind/unbind or config writes. Does not replace Dashboard usage collection.

## 7. Sub2API

A separate site-management workbench, not a Routes subpage or a Connections replacement.

- Sign in to a Sub2API site (password; captcha / 2FA when required); session refresh; remembered accounts, with passwords in the desktop SQLite vault through the settings port (memory in mock).
- After sign-in: filter keys by group; create, edit, enable/disable, or delete keys; key values stay masked; import a usable key into an installed Agent.
- Site session and saved accounts live here, not in Connections.

**Agent touchpoints:** import uses ApiKeyAccount / ConfigWrite and the same import helpers as 入口 Key.

**Not:** gateway start or pool enrollment. The settings port's child-webview login exists but the current UI does not use it.

## 8. Chat

One conversation, one Agent: session rail, transcript, process pane, composer. The quality bar is [Chat 体验标杆](chat-experience-bar.md). Which channel each Agent uses, approval scope, and title sources are in [Chat 与 Agent](../concepts/chat-and-agents.md) and [STATUS](../STATUS.md).

Session rail and header:

- The rail has new conversation (**新建对话**, accent fill), search by title and working directory, day grouping, rename, and delete confirmation. Rows show the title only; working directory, draft, and time are on hover. Rail width is dragged and remembered (`agenthub:chat-rail-width`).
- A title starts as a short phrase from the first message. After a turn, the Agent's own stored title replaces it where the Agent keeps one. A hand-renamed title is never overwritten. AgentHub never asks the Agent for a title.
- The header shows Agent identity, working directory, how this chat connects (持续对话（通用接口） / 持续对话 / 原来的发送方式; the UI does not say “ACP”), auto-approval state, and connection context. Session settings repeat the connect label and, for continuous chats, a **本会话已一直允许** switch that can only be turned off.
- Hidden or unauthorized Agents stay visible with a reason but cannot be picked for a new send. A missing working directory is a blocker, not an automatic modal.

Composer:

- Blockers are checked in order: hidden Agent → environment not ready → missing authorization → unknown status → missing working directory. Only the first is shown, with a recovery action.
- Sending is per conversation; several conversations can generate at once. Switching conversations never cancels a run.
- Empty transcript: headline **开始对话** only; example chips above the composer only fill the draft (the note is on chip hover); placeholder **发消息…**. Queue-only limits and Enter / Shift+Enter hints live in hover titles, not permanent lines. The first-use toolbar keeps Agent, connection, model, thinking, images, and skills, but image/skill labels collapse to icons (accessible names stay) and the connection label truncates with a full-name hover.
- Enter sends, Shift+Enter adds a line; the footer names the shortcut once the transcript has content.
- One circular control bottom-right, never Stop and Send side by side. Idle: Send (disabled when empty). Generating with draft text and a real action (mid-turn inject, else queue): Send. Generating with an empty or blocked draft: Stop (square icon, danger style, same footprint). After click, or while the runtime is already cancelling, it shows **正在停止** disabled until the turn ends; a missed cancel re-enables it. Esc uses the same stop unless a dialog, menu, or preview owns Escape.
- Queued follow-ups show a count, preview, and clear action. Focus stays in the composer after send.
- Failed or stopped turns have no retry button; the user keeps sending in the same chat. Stopped replies never show a raw `cancelled` word.
- `/` lists run-now actions (new chat, copy latest reply); model, effort, and skill items appear only when the query matches. When a Grok / Kiro session is ready and the peer declared commands, `/` also lists them: picking one sends `/name` as a normal turn, or inserts `/name ` when arguments are required. History, search, settings, Agents, and Connections stay on the rail or header. There is no duplicate ⋮ menu next to the selected Agent.
- A compact shortcuts control opens the shortcut overview on hover or click; `?` opens the shortcuts dialog. Delete confirmations (sessions, backups) keep Enter-to-confirm and show a return-key icon.

Transcript and process:

- Continuous turns poll the focused conversation's snapshot about every 80ms and show **正在想** before the first character, then **正在写** with a caret. The body is the snapshot's `currentMessage`, not a client-side drip. A failed poll keeps the last view and after one second shows **没法更新这场对话** with Retry, without toasting each poll or falling back to the one-shot send.
- Each reply has a one-line process summary (正在读取 / 正在修改 / 正在执行). Clicking it opens the right-hand pane with 你说了, thinking, tools, pending allow/deny, turn usage, and run details (the same pane as Markdown preview). Tool names, statuses, and JSON sit in a per-step **细节** disclosure; commands, stderr, status events, and exit codes in **运行详情**.
- After a turn ends, a muted 输入 / 输出 footnote may appear under the reply when the protocol sent counts. Nothing while generating, no session totals, no fake usage bar.
- A plan list sits above the composer when the Agent published one: completed/total counts, statuses 待做 / 进行中 / 已完成 / 失败 (missing status counts as 待做), collapsible to counts plus the in-progress row. It is not a process timeline.
- Copy appears on completed user and Agent messages only.
- Chat outline (聊天大纲): when enabled (default; 设置 → 偏好 → 语言与外观, `agenthub:chat-outline-enabled`), with at least two user messages and a transcript panel at least 768px wide, a tick rail on the left jumps between prompts. Hover magnifies nearby ticks and shows a preview; the current turn is highlighted; a jump turns off stick-to-bottom.

Approval cards:

- **允许** / **拒绝** always; **一直允许** only when the request carries that option. Pending options are stored with the request, so a snapshot or restart shows the same buttons.
- After **一直允许**, later prompts of the same scope in this conversation are auto-accepted (not saved): a file-change grant covers later file changes; a command or tool grant covers only that exact call; a file grant never approves a command. The chat then shows **本会话已一直允许**, distinct from session auto-approve or Kiro **完全访问权限**.
- File-change cards are titled **修改文件** and show the path.
- Cursor is not on this surface. Full rules: [Chat 与 Agent · 允许 / 拒绝 / 一直允许](../concepts/chat-and-agents.md#允许-拒绝-一直允许).

**Agent touchpoints:** StructuredStream (and text fallbacks); DangerousMode / auto-approval where supported; SessionResume; connection context from the current login or 本机转发.

**Not:** editing Agent logs in place, Connections or Routes management, or installing Agents or skills.

## 9. Skills, Projects, and Plugins

Full-height workbenches with a left list and an optional right preview.

### Skills (技能)

- Page tabs: User skills, Project skills, Market. Filters and Agent scope stay in the chrome row.
- User skills list the shared library plus this-tool-only skills, with the enablement matrix. Install writes `~/.agents/skills/` only and does not enable the skill anywhere.
- Project skills pick a workspace already found on the History page; add or delete skills in that workspace's `.agents/skills`. The same install dialog is used.
- Install dialog: one Source field (folder, zip, or git URL; must contain `SKILL.md`). **Choose folder** opens the system folder picker; **Choose zip** opens the system file dialog (title **选择技能 zip**, ZIP filter). Cancelling leaves the field unchanged; an empty source shows a field error.
- The skill name (`ListNameButton`, or Enter) opens the preview. Checkboxes are for batch actions only. The preview stays open when filters hide its skill and shows a short source label.
- Rows keep the name and at most one line of description; absolute paths go to the preview footer or an open-directory action.
- The matrix shows supported / unavailable / unknown; a missing skill directory is a partial state, not a page error.

**Agent touchpoints:** Skills capability and the per-Agent matrix; project skills read/write `.agents/skills` and list existing per-Agent folders (e.g. `.claude/skills`). The market source is in 设置 → 偏好.

**Not:** plugin packs or MCP servers.

### Projects (历史)

- Collapsible project cards. Sessions align in columns (title, file name, time, size, icon actions) without row dividers. The title opens the excerpt preview; the file name reveals the record in the file manager.
- Page actions (summarize, delete, refresh) stay in the list column and move with it when resizing.
- Search covers project and session names. Summarize and delete are session actions with confirmation where supported.
- A session can start a new Chat conversation through the session-storage handoff; the original Agent log is never edited.

**Agent touchpoints:** ProjectHistory for list/preview; ProjectDelete where supported (ZCode / Kiro deletion stays in that tool; Cursor unsupported). Kiro lists both CLI and editor conversations. Unsupported actions are hidden or disabled with a hint.

**Not:** an IDE, a log editor, Skills market, or plugin management.

### Plugins (插件)

- Lists installed packs for Claude, Codex, Grok, and Pi. A row shows name, on-disk version when known, one line of description, and exception badges (disabled / untrusted / not installed / version mismatch). Clicking a row opens details.
- Details lead with pack components (bundled MCP is a component, not a row), then version, marketplace, scope, and path. Pi also shows its configured npm selector or git ref and how updates are judged.
- Claude, Codex, Grok, and Pi packs can be installed and uninstalled here. Claude, Codex, and Grok can be enabled or disabled (disable is not uninstall); Pi loads installed extensions and has no fake toggle. Claude and Grok user-scope packs can be updated one at a time. Marketplace refresh is available for Claude, Codex, and Grok but stays separate from package updates; Codex refresh is marketplace-wide. Pi has no marketplace list: install accepts npm, git, or absolute local-folder sources (`~/…` is expanded by the backend), removal reuses the exact scanned source, and eligible updates remain one all-pack action.
- If an official update command fails, AgentHub restores its configuration file only; package directories and caches already changed by that command are not guaranteed to roll back.
- Empty copy depends on the Agent filter: wired-but-empty, planned, or unsupported. Scan sources are not shown in the list.

**Agent touchpoints:** each Agent's official plugin CLI and supported local enable/disable config. There is no `Capability::Plugins`, and this is not `Capability::Mcp`.

**Not:** Codex one-pack update, Pi marketplace/package toggle, a dynamic plugin ABI, or a shared cross-Agent marketplace.

## 10. Agents and MCP

### Agents

- The lifecycle page: installed state, runtime readiness, install/update, hide, and environment repair, as a field table in a full-height split.
- Click the Agent **name** for detail; start / install stay labeled on the row; hide is in the row `⋯` menu, and a hidden row shows **取消隐藏**. Detail names that Agent's new-chat connect method. Uninstall in detail is `dangerOutline`; its confirmation uses `danger`.
- A missing runtime is shown before Agent install, with repair steps and re-detect. Never offer a successful install while a prerequisite is known to be missing.
- Leftover `~/.agenthub/npm` copies are labeled **启动后备，非安装位置**; installs go through official npm into `~/.npm-global`. An incomplete DeepSeek CLI (e.g. `~/.local/bin/dsh` without `@deepseek-ai/dsh-scope`) is not-ready and prompts the same npm install.

**Agent touchpoints:** catalog, install registry, and detect are the list source; `src/config/agents.ts` is display only. Install channels and runtime prerequisites come from the Agent's `install` / `lifecycle` ports. Soft-hidden Agents (e.g. Cursor) can be unhidden here.

**Not:** Chat, login management, or Routes.

### MCP

- A single-column table of known MCP server config files: Agent, server, transport, source path, enabled.
- Write dialog: local catalog → probe → write into a supported Agent. Writable rows have an enable toggle (Codex disable removes the entry; Grok disable sets `enabled = false`).
- Parse errors, missing files, and an empty inventory each have their own recoverable state.

**Agent touchpoints:** `Capability::Mcp` is Partial for Claude / Codex / Grok / Cursor / WorkBuddy (write/enable, no OAuth) and Planned elsewhere. Bundled MCP inside a plugin pack shows on Plugins.

**Not:** OAuth connectors, credential encryption, a remote marketplace, or plugin packs.

## 11. Settings (设置)

Five page tabs (`?tab=`), kept as a pill bar at the top-left of the workbench header. Invalid or old values replace to the nearest tab; tab changes use `replace` history.

| Tab | Query | Contents |
|---|---|---|
| 偏好 | `preferences` | Groups: 语言与外观 (language, theme, accent, chat outline); 启动与关闭; 连接 (自动导入本机登录); 路由 (duplicate-key tip, same-URL update); 技能 (market source); 用量 (collection interval) |
| 功能 | `features` | Which optional sidebar pages show; whether clicking 路由 collapses the sidebar |
| 本机 | `local` | Data directory, log level, retention, log directory |
| 备份 | `backups` | Agent config snapshots with `AgentTabStrip`; keep-copies switch; restore/delete; file inspect on the right |
| 关于 | `about` | Version, update check, repository, read-only notes on how login information is shown |

- Preferences, Features, This computer, and About use the overview column. Backups is a list-and-inspect split.
- Keep-copies (`keepLiveFileCopies`, default on) copies each Agent's live files into the backup directory on switch or import. Turning it off stops piling copies; a switch still keeps one copy for rollback. Manual backups are unaffected. Backup identity stays a short label (email or key tail).

**Agent touchpoints:** LiveBackup and per-Agent snapshot identity.

**Not:** a login list, route runtime, Agent install, plugin install, credential encryption, or Sub2API sessions.

## 12. Responsive and interaction rules

- Use stable grid tracks (`auto-fit` / `minmax`) for cards, tables, toolbars, and preview panes.
- On narrow windows, wrap labels and metadata instead of shrinking type or overlapping. Icon actions move into an overflow menu when a row cannot fit.
- A split preview has a focusable separator: keyboard moves it in fixed steps, double-click restores the default width, dragging never selects page text.
- Escape closes the topmost dialog/menu/popover before a preview. Focus order: content, row actions, separator, preview tools, document body.
- Page copy lives in the page or the locale dictionary (`src/lib/i18n/locales/`). No implementation phase labels in visible UI.

## 13. Implementation references

- Layout and routing: `src/App.tsx`, `src/components/layout/`, `src/pages/`.
- Shared components: `src/components/ui/`, `src/components/shared/`, [design-system.md](design-system.md).
- Backend access: `src/lib/api/`, `src/lib/backend/contracts/`, `src/lib/backend/tauri/`.

When code and this page disagree, verify the implementation and update this page in the same change. Do not revive a completed redesign document as a task list.
