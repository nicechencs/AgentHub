---
title: 插件与 MCP 管理
type: proposal
status: proposed
owner: maintainers
updated: 2026-08-26
---

# 插件与 MCP 管理

> Status: proposed
>
> 这是候选模块，不是实施承诺。在 owner、兼容计划、失败行为和测试批准之前，不得把本页复制进当前功能列表，也不得把 `Capability::Mcp` 改成 Full。

## 1. 当前基线

- Skills 已由 `SkillService` 管理：共享源 `~/.agents/skills/`，投影到各 Agent 技能目录。见 [插件、MCP 与技能](../concepts/plugins-and-mcp.md)。
- MCP 页 `/mcp` 只调用 `list_mcp_inventory`。扫描位置和缺口见 [MCP inventory](../reference/mcp-inventory.md)。
- 全部内置 Agent 的 `Capability::Mcp` 都是 Planned（待验证接入）。inventory 明确不 `require(Mcp)`。
- 厂商 Plugin 市场（Claude `/plugin`、Codex `/plugins`、Grok `plugin`）未接线。
- 侧栏工作区固定包含 MCP，位于 Skills 与 Projects 之间。设置里没有 MCP/插件入口开关。页面是单栏表格，行内展开 snippet。
- 路由页已是左右分栏，且设置「显示路由页面」可隐藏管理区入口、不禁用 `/routes`。

## 2. 候选目标

在 AgentHub 里提供一个与 Routes 同构的**插件工作台**：只管理各 Agent 已验证的 MCP live 配置（以及后续单独切片的厂商 Plugin 包），让用户能看见、打开详情、在受测 Agent 上添加/移除/开关，而不把 AgentHub 做成第二套 MCP runtime。

产品对象默认是 **MCP server 条目**。页面中文可称「插件」，英文 `Plugins`，说明里保留 MCP。厂商 Plugin 包（marketplace bundle）不是第一刀。

## 3. 建议边界

### 做

- 补齐只读 inventory，使其与已验证的用户级 live 文件一致（至少补 Grok `[mcp_servers]` TOML）。
- `/mcp` 改为左右分栏工作台：左列表、右详情（脱敏 snippet、来源路径、打开目录）。
- 设置 → 偏好增加「显示插件页面」，持久化方式对齐 `routesNavVisible`。关闭只藏侧栏入口，不禁用页面、不改 `/mcp`。
- 侧栏工作区在开关打开时把插件放在 **Projects 下方**：`Chat → Agents → Skills → Projects → 插件`。
- 每个 Agent 一个稀疏 **MCP 端口**（paths + parse + 可选 write），注册在 `integrations/agents/<key>/`，平台 service 不再 `match AgentId` 堆新分支。今天的 `source_locations` 集中 match 是过渡实现，写入面不得再扩大这种 match。
- 写入走显式 command（plan/apply 风格的「预览 → 确认 → 写 live → 刷新」），写前备份该 Agent 的 live 文件。失败可重试，不 force 删。
- 密钥不进 UI；env/headers 只允许引用已有环境变量名或显示「已设置 / 未设置」。

### 不做

- 不把 MCP server 跑在 AgentHub 进程里，不做公网网关。
- 不把 Skills 市场、Claude/Codex/Grok plugin marketplace 合成一个商店。
- 不扫描未信任的项目级 MCP 文件（stdio 会拉起进程），除非单独做「仅展示、默认不加载」切片并写明信任模型。
- 不把 DSH Cordis 插件、Pi `pi install` 扩展、Cursor IDE 私有库伪装成 MCP。
- 不为 Kimi 或未验证 Agent 伪造 Full。
- 不把凭据落盘加密或国产 OAuth 转 API 绑进本模块。
- 不引入动态插件 ABI、第二套领域库、微服务。

## 4. 模块形状（自顶向下）

```text
页面 /mcp（WorkbenchSplitPage）
  → lib/api/mcp façade
  → backend.mcp port
  → Tauri commands
  → core McpInventoryService / 未来 McpWriteService
       ↳ AgentMcpContribution（每 Agent 稀疏端口）
       ↳ live 文件 + 脱敏
       ↳ BackupService（写前快照）
```

| 层 | 职责 |
|---|---|
| UI | 列表、筛选、详情、设置开关、侧栏顺序。不解析 TOML/JSON |
| contracts | inventory DTO、write plan/apply DTO、错误码 |
| core service | 编排扫描、预览 diff、备份、写入、刷新 |
| contribution | 该 Agent 的路径、格式、upsert/remove、可选 CLI 探测 |
| 厂商 CLI | 可选诊断（如 `grok mcp doctor`）；不能当唯一真源，live 文件才是真源 |

Skills 的共享源模型**不**套用到 MCP：MCP 没有 `~/.agents/mcp/` 真源。一条 MCP 配置属于某一个 Agent 的 live 文件。跨 Agent 复用是「按格式复制条目」的显式动作，不是投影链接。

## 5. 写入策略（按 Agent 成熟度）

第一刀只对 **live 文件形状已有测试** 的 Agent 开放 apply：

| 优先级 | Agent | 写什么 | 依据 |
|---|---|---|---|
| P0 只读补齐 | Grok | 读取 `config.toml` `[mcp_servers]` | 官方契约已明确，scanner 现在漏了 |
| P1 写入 | Claude | upsert/remove `mcpServers` 键 | JSON 形状已有 inventory 测试 |
| P1 写入 | Codex | upsert/remove `[mcp_servers.name]` | TOML 形状已有 inventory 测试 |
| P1 写入 | Cursor | upsert/remove `mcpServers` | JSON 与 Claude 同类 |
| P2 写入 | Grok | upsert/remove `[mcp_servers]` | 与 Codex 同类 TOML |
| P2 写入 | Pi | upsert `mcp.json` | 路径已探，需锁定发行版 schema fixture |
| P3 | WorkBuddy | `.mcp.json` | 需 round-trip 测试 |
| 关闭 | Kimi、DSH MCP | 保持探测或 Unsupported | 无稳定契约 |
| 另开提案 | Claude/Codex/Grok Plugin 包 | marketplace install/update | 与 MCP 条目不同生命周期 |

Apply 成功后只刷新 inventory，不启停 MCP 进程；进程由目标 Agent 下次会话拉起。Grok `mcp doctor` 可以作为可选「检查」动作，失败时显示厂商输出的脱敏摘要。

## 6. UI 切片（与路由对齐）

1. `/mcp` 进入 `isWorkbenchSplit`：紧凑页头 + Agent 条 + 左列表 + 右 `InspectSurface`。
2. 行点击或「详情」打开右栏；去掉行内 `DetailsToggle` 展开。
3. 右栏：名称、启用状态、传输、命令/地址、来源路径、打开目录、脱敏 snippet。无密钥。
4. 空/加载/错误留在左栏。
5. 设置偏好：「显示插件页面」，文案对齐路由（关入口不关页面）。
6. 默认显示。打开时侧栏在 Projects 下。
7. 路径保持 `/mcp`。中文导航「插件」，英文 `Plugins`。

在写入面落地前，工作台仍是只读详情。不要先做灰掉的「添加」按钮。

## 7. 候选切片

不是排期，是评估顺序：

### A. 文档与盘点真源

现行概念/参考页（本提案的前置）保持与源码一致。补 Grok TOML 扫描和测试。不改能力矩阵。

### B. 只读工作台

分栏 UI、设置开关、侧栏顺序、page-patterns 更新为 current。仍只读。

### C. 单 Agent 写入狗粮

先选 Claude 或 Codex 一条 JSON/TOML upsert+remove：预览 diff、写前备份、失败重试、inventory 刷新。契约测试锁 round-trip 与脱敏。

### D. 端口化

把 `source_locations` 的集中 match 收成 contribution。新 Agent 按 [添加 Agent](../guides/adding-an-agent.md) 注册 mcp 端口；未注册则只有探测或隐藏写入。

### E. 厂商 Plugin 包（可选，另开）

仅当 C/D 稳定且用户明确要商店：只包 Claude 或 Grok 之一的 marketplace，fail-closed 其余。不得阻塞 MCP 条目管理。

## 8. 决策门槛

提升为 current 之前必须同时成立：

- inventory 对每个开放写入的 Agent 有脱敏 fixture 和 round-trip 测试。
- 写失败不留下半份 JSON/TOML；备份可恢复。
- `Capability::Mcp` 仅对**已开放写入**的 Agent 升 Partial/Full，其余保持 Planned/Unsupported。
- 非 Tauri 生产页明确 unavailable，不静默 mock 写入。
- `pnpm check:docs` 与相关 cargo/vitest 过滤测试通过。
- 点测：开/关侧栏开关、Projects 下入口、打开详情、对一个已接线 Agent 添加再删除一条无密钥 stdio server。

## 9. 相关页面

- [插件、MCP 与技能](../concepts/plugins-and-mcp.md)
- [MCP inventory](../reference/mcp-inventory.md)
- [Agent 插件表面](../reference/agent-plugin-surfaces.md)
- [UI 页面模式](../ui/page-patterns.md)
- [添加 Agent](../guides/adding-an-agent.md)
- [产品边界](../decisions/product-boundaries.md)
---