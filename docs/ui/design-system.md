---
title: UI 设计系统
type: reference
status: current
owner: maintainers
updated: 2026-08-30
---

# UI Design System

> Status: current contract
> 
> This document defines the visual language, component choices, interaction states, and accessibility rules for the current AgentHub UI. It is the source of truth for reusable UI decisions. Product behavior and page composition live in [page-patterns.md](page-patterns.md).

## 1. Product language

- The user-facing navigation label is **Routes** in English and **路由** in Chinese. The canonical path is `/routes`.
- **本机路由** describes the loopback forwarding method in explanatory copy. It is not an alternative navigation label.
- The product surface says **登录**. Internal names such as Ticket, Binding, Adapter, and `local_bridge` may remain in implementation and diagnostic references, but do not leak into ordinary UI copy.
- Navigation keeps stable product names in English: Dashboard, Chat, Agents, Skills, MCP, Projects, Plugins, Connections, Routes, and Settings. Page content is Chinese-first with the English name available through the locale dictionary.
- The UI never describes a generated provider as a second account or a second wallet. Explain the user outcome: direct connection, using this login, or local routing.

## 2. Design principles

1. **Quiet desktop tool.** Use neutral surfaces and restrained emphasis. Agent brand colors are for small identity marks and charts, not large backgrounds.
2. **One primary action.** A page has at most one `Button variant="default"` action. Secondary work uses `secondary`, `outline`, or `ghost`.
3. **Actionable empty states.** Empty data normally provides one clear next step. The healthy empty state on Routes is intentionally informational because most connections do not require a running local route.
4. **Progressive disclosure.** Keep the main column scannable. Put explanations in `Hint`, `Tip`, a short `Notice`, a first-run callout, or a confirmation dialog according to their importance.
5. **Partial failure is normal.** One unavailable Agent or parser must not blank the rest of a page. Render the unaffected data and mark the affected block.
6. **Danger before execution.** Explain impact, backup behavior, and running-process consequences before a destructive or live-configuration action.
7. **Secrets are never ordinary text.** Use `SecretInput` with masked display. Do not add credential-at-rest encryption work to UI scope.

## 3. Tokens and geometry

The runtime source of truth is `src/styles/tokens.ts`. CSS variables are injected by the application; business pages must use semantic tokens rather than new literal values.

### 3.1 Type scale

Only three semantic text roles are allowed for new UI:

| Role | Token/class | Size | Use |
|---|---|---:|---|
| Title | `text-title` | 16px | Page titles, empty-state headline, metric value, document H1 |
| Body | `text-body` | 13px | Body copy, buttons, list names, section labels, menus |
| Meta | `text-meta` | 12px | Table headings, paths, timestamps, badges, hints, diagnostic text |

Existing `text-lg`/`text-xl`, `text-sm`/`text-base`, and `text-xs`/`text-2xs` aliases are compatibility names at the same pixel sizes. Do not introduce a fourth type scale or arbitrary `text-[Npx]` values. Use weight, not a new size, to distinguish section titles.

### 3.2 Surfaces and color

Use semantic surface roles:

| Role | Token | Use |
|---|---|---|
| Canvas | `bg-canvas` | Main work area and quiet page background |
| Panel | `bg-panel` | Sidebar, page panel, preview panel, dialog content |
| Subtle | `bg-subtle` | Toolbars, table headings, secondary strips |
| Hover | `bg-hover` | Pointer hover on an enabled item |
| Active | `bg-active` | Current page item, current preview target, current connection |
| Overlay | panel plus shadow | Menus, popovers, dialogs, and toasts |

The product accent is `--accent` (`bg-accent` / `text-accent` / `ring-accent`). Default is indigo. Settings exposes a small palette (purple / blue / teal / rose / orange) that writes `html[data-accent]` and only changes `--accent`. Use it for focus, links, checked switches, the in-app mark, and the one primary action. Do not hardcode the indigo hex, do not use an Agent color as a page background, and do not substitute an Agent color for semantic status colors. The running window (taskbar button), tray icon, and Windows Desktop / Start-menu shortcuts that already point at this app follow the same mark. The installer package icon stays the default indigo asset.

Status colors are semantic: `success`, `warning`, `danger`, and `info`. A status must also have text or an icon; color alone is insufficient.

### 3.3 Spacing, radius, and elevation

- Spacing follows the 4/8/12/16/24/32px ladder.
- Controls use `rounded-btn` (6px); cards, panels, and the application shell use `rounded-card` (8px); composers and user bubbles use `rounded-composer` (12px). Chips, avatars, switches, and progress tracks may use `rounded-full`.
- Do not add `rounded-lg`, `rounded-2xl`, or arbitrary radius values.
- `shadow-xs` is for a card, `shadow-sm` for a light raised control, `shadow-md` for menus/toasts, and `shadow-lg` for dialogs. Buttons do not gain a shadow on hover or press.
- The application canvas has an 8px outer gutter. The shell columns are rounded panels. Keep the shell and page surfaces distinct without stacking multiple borders around the same content.

### 3.4 Content widths

There are two content systems only:

| System | Token/pattern | Use |
|---|---|---|
| Reading column | `pageRhythm.readingColumn` (`mx-auto w-full max-w-3xl`) | Chat transcript/composer, Settings form, long-form reading |
| Edge column | `pageRhythm.pageShell` / `workbenchX` from `pageEdge.inset` (currently 8px) | Tables, lists, dashboards, split workbenches, Routes. Change `pageEdge.inset` in `src/components/layout/page-rhythm.ts` to retune every page edge. When a split pane is open, the scrolling list uses `workbenchXSplit` (`inset` left; canvas gutter pad + margin on the right) so the scrollbar is not flush against the separator. |

Do not introduce page-private `max-w-*` values, a third content width, or a second left-aligned reading width. `fullBleed` describes height and scrolling behavior, not a third width system.

## 4. Component rules

### 4.1 Base components

The only base component family is `src/components/ui/` using the existing shadcn/Radix and CVA setup. Do not add another UI library.

| Need | Component | Rule |
|---|---|---|
| Primary or secondary command | `Button` | Use `default`, `secondary`, `outline`, `ghost`, `danger`, or `dangerOutline` by action weight |
| Text entry | `Input` | Use the standard height and focus ring; do not hand-build search inputs |
| Secret/token entry | `SecretInput` | Mask by default; eye control toggles visibility |
| Choice list | `Select` | Use the shared trigger and semantic labels |
| Page-level sections | `Tabs` | Use for Settings and other page navigation |
| Page-local filtering | `SegmentedControl` | Use for a small set of filters, not page navigation |
| Agent filtering | `AgentTabStrip` | Use the shared medium-density strip and Agent identity marks |
| Overlay | `Dialog`, `DropdownMenu`, `ContextMenu` | Use the existing focus and dismissal behavior |
| Management table | `TableShell` | Default variant is the card shell; business pages do not hand-write workbench classes |
| Management row | `ListRow` | Use active background and optional leading bar; do not nest another card |
| Search | `SearchField` | Use the shared icon, height, and focus behavior |

Use lucide icons for familiar icon-only actions. Every unfamiliar icon button needs a `Hint`. A text button is appropriate when the command itself is the thing the user must scan, such as “添加登录” or “重试”.

### 4.2 Surfaces and selection

- A standalone content block uses `Card default`; a toolbar or nested block uses `plain` or `subtle` to avoid double framing.
- A management list row may have a card edge. A workbench rail or transcript row uses a page-owned active background and does not become a card.
- Active preview and checkbox selection are separate states. Preview uses `bg-active`; batch selection uses the checkbox and toolbar. Never paint a whole selected table row with accent.
- Tabs, segmented controls, and AgentTabStrip share the same gray track and raised active item. Keep their roles distinct.

### 4.3 Action hierarchy

| Page role | Component treatment |
|---|---|
| Page's one primary outcome | `default` / accent |
| Safe secondary action | `secondary` or `outline` |
| Toolbar, cancel, or low-weight action | `ghost` |
| Destructive confirmation | `danger` inside the confirmation dialog |
| Destructive entry before confirmation | `dangerOutline` or `ghost` |

The stop action for an active operation is `dangerOutline`, while the final delete or irreversible confirmation uses `danger`. Do not create a global switch-confirmation component; each page owns its dialog because the explanation differs.

## 5. Information and feedback

### 5.1 Information levels

| Level | Meaning | Channel |
|---|---|---|
| L0 | Required to operate | Label, heading, column name, primary action |
| L1 | Current object state | Short status text, badge, or row metadata |
| L2 | Why an action is disabled or how a control works | `Hint` or `Tip` |
| L3 | Product model or first-use teaching | Empty state, dismissible callout, or help surface |
| L4 | Path, ID, or diagnostic detail | Preview footer, copy action, or diagnostic dialog |

L3 must not occupy the default main column on a dense management page. L4 must not be repeated as a row-level native tooltip.

### 5.2 Tooltip and copy rules

- The global tooltip delay is 200ms. Keep `Hint` and `Tip` on the same timing.
- `Button title` is routed through `Hint`; do not use native browser `title` as teaching copy in pages, layout, or shared components.
- Use `Tip` for truncated text and complete paths. Use a `Notice` for a page-level condition that needs an action. Use `Toast` for a short result, with a title of six Chinese characters or fewer where practical.
- Do not put stack traces, full paths, or implementation terminology in a toast title. A copy-diagnostics action may expose them in a dedicated surface.
- Use the product terms “登录”, “分享”, “路由”, and “本机路由” consistently. Avoid “票”, “钱包”, and implementation phase numbers in ordinary user copy.

### 5.3 Confirmation content

Before switching a live connection or restoring a backup, show the relevant backfill summary, backup location, and running-process warning. Busy confirmation dialogs cannot be dismissed while the mutation is in flight. Recovery and error copy must say whether the live write happened, rather than guessing from a refresh failure.

## 6. States and accessibility

Every page or independently loaded block covers four states:

| State | Required treatment |
|---|---|
| Loading | Same-density skeleton (`ListSkeleton`, `TableSkeleton`, `CardGridSkeleton`, or `Skeleton`); avoid a lone spinner |
| Empty | `EmptyState` with a clear next step, except healthy Routes runtime empty state |
| Error | `ErrorState` with readable summary, retry, and optional copy-diagnostics action |
| Partial | Keep healthy blocks visible; mark only the unavailable Agent/data block |

Additional rules:

- Focus-visible controls use a 2px accent ring. Do not replace focus with a background color.
- Icon-only actions remain keyboard reachable and have an accessible name. Disabled controls explain why through a wrapper hint.
- Dialogs use the shared focus trap and busy-dismissal guard. Escape closes the topmost overlay first.
- Loading content sets `aria-busy` on its container when a stable region is being replaced. Skeletons do not enter the tab order.
- Status cannot rely on color only; pair it with text, icon shape, or an accessible label.
- Preserve stable dimensions for toolbars, grids, list rows, and split panes so loading or long text cannot shift neighboring controls.
- Respect reduced-motion preferences for panel transitions and progress effects.

## 7. Implementation boundaries

- Pages call `lib/api` or the backend façade. Only `lib/backend/tauri/` may call `invoke`; UI components do not select mock versus Tauri at runtime.
- Browser mock data is for `dev:mock` and tests. A non-Tauri production surface reports unavailable rather than silently substituting mock data.
- Production writes use the product `plan`/`bind`/`unbind` flow. Runtime control for local routing remains behind the backend/control contract.
- Tests live beside the relevant module as `*.test.ts(x)`; production files do not carry test-only reset helpers.
- This design-system document is not a roadmap. Historical visual comparisons and completed Phase records belong in the archive.

## 8. Review checklist

- Is the page using `/routes`, Routes, and 路由 in current-facing copy?
- Is there at most one accent primary action?
- Did the change use semantic type, surface, spacing, radius, and focus tokens?
- Are loading, empty, error, and partial states covered?
- Are active preview and batch selection distinct?
- Are paths and implementation details kept behind `Tip`, diagnostics, or a preview footer?
- Does the UI avoid native teaching tooltips and nested cards?
- Can the main workflow be completed with keyboard and without color-only status cues?
