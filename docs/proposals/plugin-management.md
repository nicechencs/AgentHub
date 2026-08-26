---
title: 插件（extension / plugin）管理
type: proposal
status: proposed
owner: maintainers
updated: 2026-08-26
---

# 插件（extension / plugin）管理

> Status: proposed
>
> 产品对象是各家 **plugin / extension 包**，不是 MCP server。`/mcp` 保持只读 MCP 清单。在 owner、兼容计划、失败行为和测试批准之前，不得把本页写成当前功能，也不得新增未实现的能力矩阵 Full。

调研日期：2026-08-26。对照的是各家官方 CLI/文档与同类桌面管理器，不是实施承诺。

## 1. 当前基线

- AgentHub **没有**插件页。工作区是 Chat / Agents / Skills / MCP / Projects。MCP 在 Skills 与 Projects 之间，单栏只读表。
- 路由页已是左右分栏；设置「显示路由页面」可藏入口、不禁用 `/routes`。
- Skills 已由 `SkillService` 管理。MCP 由 `list_mcp_inventory` 只读扫描。二者都不是插件包。
- 无 `Capability::Plugins`。`Capability::Mcp` 仍是 Planned，且只约束 MCP 写入，不约束插件。
- 厂商侧已经存在完整插件生命周期（见 [Agent 插件表面](../reference/agent-plugin-surfaces.md)）：Claude `claude plugin`、Codex `codex plugin`、Grok `grok plugin`、Pi `pi install`。

## 2. 候选目标

新增与 Routes 同构的 **插件工作台**（建议路径 `/plugins`）：

1. 左右两栏：左为已安装/可用列表，右为包详情（组件清单、范围、版本、路径）。
2. 设置 → 偏好控制侧栏「插件」开关；关闭只藏入口，不禁用页面。
3. 打开时工作区顺序为 `Chat → Agents → Skills → MCP → Projects → 插件`（插件在 **Projects 下方**）。MCP 项保持原意，不改名。

管理动作对齐厂商：浏览、安装、启用/停用、更新、卸载。AgentHub **不**运行插件代码，**不**当 MCP host，**不**合并各家市场为一个商店。

## 3. 同类怎么管（2026-08 对照）

三类产品，只抄第一类的纪律：

| 模式 | 代表 | 真源 | AgentHub |
|---|---|---|---|
| **A. 写各家 live / 调官方 CLI** | Claude `/plugin`、Codex `/plugins`、Grok plugin TUI、`claude-code-marketplace`（全部委托 `claude plugin`）、Pi `pi install` | 各客户端自己的 cache + enabled 列表 | **采用**：插件页是这一类 |
| **B. 本地网关，装一次同步到所有客户端** | mcpx、mcp-mux、Brightwing | 管理器自己的 registry + proxy | **不采用**：那是 MCP 网关，且常伴随密钥另存 |
| **C. 自己当 host，启停 stdio 进程** | Cline MCP 面板、Goose Extensions、simple-mcp-manager | 本进程拉起 server | **不采用**：AgentHub 不是 MCP runtime；Goose 把 MCP 叫 extension，用词不要学 |

各家 **plugin 包** 的共同点（A 类里要对齐的）：

| 能力 | Claude | Codex | Grok | Pi |
|---|---|---|---|---|
| 安装 | `plugin install name@market` | `codex plugin add` / `/plugins` | `grok plugin install` | `pi install npm:\|git:` |
| 卸载 | `plugin uninstall` | `codex plugin remove` | `grok plugin uninstall` | `pi remove` |
| 启用/停用 | `enabledPlugins`；停用不等于卸载 | config 里 toggle；Space | `plugin enable/disable`；`[plugins] enabled/disabled` | 装上即加载；`/reload` |
| 更新 | `plugin update`；市场 `marketplace update`；可开机自动更 | marketplace upgrade；cache 按 version 分目录 | `plugin update [name]`；市场 `marketplace update` | `pi update --extensions`；钉版本跳过 |
| 检测更新 | 刷新 marketplace catalog，再比已装版本 | 市场 Ctrl+U / upgrade | 市场 `r` 刷新、`u` 更新插件 | `pi update` / 第三方 `/zmarketplace updates` |
| 目录 | `~/.claude/plugins/cache`；数据 `plugins/data/{id}/` | `~/.codex/plugins/cache/$market/$plugin/$ver/` | `~/.grok/plugins/` | Pi settings 里的 package 列表 + npm/git 缓存 |
| 信任 | 安装即加载组件；MCP 随插件启停 | 市场策略 `authentication` | **enable ≠ trust**：hooks/MCP/LSP 要 `--trust` | 扩展可执行任意代码，安装前审查 |
| 范围 | user / project / local / managed | 全局 cache + 仓库 marketplace | cli / project / user | user settings；`-e` 一次性 |

VS Code / Cursor IDE 的扩展市场是 IDE 插件，不是 `cursor-agent` CLI 的包系统。AgentHub 的 Cursor 适配器是 CLI：**不要**把 VS Code Marketplace 当成 Cursor Agent 插件源。

Cline / Continue Hub / 官方 `registry.modelcontextprotocol.io` 管的是 **MCP 发现**，留给 `/mcp` 以后的提案，不进本插件页。

**从同类抄过来的纪律**

1. 启用/停用与卸载分开（Claude、Grok、Codex、VS Code 都如此）。
2. 写前备份、失败可回滚（mcp-manager 的 apply 流程；Skills 已有同类纪律）。
3. 能调官方 CLI 就不要自己改 cache 目录（`claude-code-marketplace` 只做 UI，操作全部 `claude plugin`）。
4. 详情展示包内组件（skills / commands / agents / hooks / 附带 MCP），而不是把附带 MCP 提升成插件列表的主键。
5. 更新检测 = 刷新市场目录 + 比较已装版本，不是 `mcp doctor` 连通性。
6. 默认不扫项目级未信任目录里的插件源。

**明确不抄**

- 自建跨 Agent 插件商店或 Smithery 一键托管。
- 本地 MCP 网关 / 加密凭据库（产品边界：不做落盘加密）。
- 在 AgentHub 进程里启动插件或 MCP。
- 把 Goose「extension = MCP」的叫法引进 UI。

## 4. 建议边界

### 做

- 新路由 `/plugins`，全高 `WorkbenchSplitPage`。
- 设置 `pluginsNavVisible`，对齐 `routesNavVisible`；默认显示。
- 侧栏工作区在开关打开时把插件放在 Projects **下面**。
- 每家一个稀疏 **Plugin 端口**：list / details / 可选 install、enable、disable、update、uninstall。优先封装官方 CLI 的 `--json` 输出。
- 新增能力键须等实现 PR 才加进 `Capability`（穷尽 match）。未接线的 Agent 标 Unsupported 或 Planned，带原因。
- 右栏只读展示：名称、市场、版本、范围、启用、信任、路径、组件列表。无密钥。
- 破坏性动作（卸、装、更新）预览 → 确认 → 调 CLI → 刷新。失败可重试。

### 不做

- 不改 `/mcp` 的产品含义，不把它改名为插件。
- 不把 Skills 市场、MCP registry、各家 plugin marketplace 合成一个目录。
- 不给 Cursor Agent、Kimi、WorkBuddy 伪造插件商店。
- 不把 DSH Cordis 树、Pi `pi install` 扩展、Claude plugin 当成同一种包格式硬转。
- 不为插件引入 AgentHub 自己的 `~/.agents/plugins` 真源（那是 Skills 模型）。
- 不把凭据加密或国产 OAuth 绑进来。

## 5. 模块形状

```text
页面 /plugins（WorkbenchSplitPage）
  → lib/api/plugins façade
  → backend.plugins port
  → Tauri commands
  → core PluginInventoryService / PluginApplyService
       ↳ AgentPluginContribution（每 Agent）
            优先：官方 CLI --json
            其次：已验证的 live 文件（enabledPlugins、cache 清单）
       ↳ BackupService（改 live 设置前快照）
```

Live 文件仍是各 Agent 的。AgentHub 只编排与展示。CLI 不可用时 fail-closed，不手改 cache 冒充已装。

## 6. 按 Agent 成熟度

| 优先级 | Agent | 只读 list 依据 | 写入 |
|---|---|---|---|
| P0 | Claude | `claude plugin list --json`；fallback `~/.claude/plugins/` + `enabledPlugins` | 封装 `claude plugin install/uninstall/update` 与 enable 设置 |
| P0 | Grok | `grok plugin list --json`；`~/.grok/plugins/` | `grok plugin install/uninstall/update/enable/disable` |
| P1 | Codex | `codex plugin list --json`；`~/.codex/plugins/cache/` | `codex plugin add/remove`；enable 走 config |
| P1 | Pi | `pi list` / settings 里的 packages | `pi install` / `pi remove` / `pi update --extensions` |
| P2 | DSH | 仅当 Cordis 插件清单有稳定只读形状 | 默认 Unsupported，不映射成 Claude 式 marketplace |
| 关闭 | Cursor / Kimi / WorkBuddy | 无已验证 CLI 插件系统 | 保持 Unsupported |

附带 MCP 只在详情里列为组件。增删 MCP 条目继续走以后的 MCP 提案，不在本模块。

## 7. 更新怎么检测

没有跨厂商协议。插件页只做三档，且必须在 UI 上分开：

1. **市场目录过期**：`claude plugin marketplace update`、`grok plugin marketplace update`、Codex marketplace upgrade。这是「有新包可装」，不是「已装包已升级」。
2. **已装包可升级**：`plugin update` / `pi update --extensions`。钉死版本的 Pi npm spec 应显示「已钉死」，不要当失败。
3. **健康/信任**：Grok 未 trust 则 hooks/MCP 为 blocked；这不是版本问题。

禁止用 MCP `doctor` 或进程是否在跑来代表插件更新。

## 8. 行动任务（可独立合入）

每个 PR 只做一列范围。合入 `dev`，不碰 `release`。未列的文件不要改。

### PR-0 文档纠偏（本提案）

- **做完标准**：现行文档把「插件」定义为 extension/plugin 包；`/mcp` 不再被叫成插件页；`pnpm check:docs` 通过。
- **不包含**：代码、能力键、新路由。

### PR-1 只读 inventory（Claude + Grok）

- **做**：core 扫描/CLI 列表；脱敏路径；fixture。前端可暂不接。
- **测试**：无 CLI 时 fail-closed；假 JSON 列表；不把 MCP `mcpServers` 算作插件行。
- **点测**：本机已装 Claude/Grok 时 CLI 列表与目录一致。
- **不做**：写入、Codex/Pi、UI。

### PR-2 插件页壳 + 导航

- **做**：`/plugins` 左右分栏；设置开关；侧栏在 Projects 下；空/加载/错误。接 PR-1 的只读数据。
- **测试**：`filterWorkspaceNavItems`；settings 文案；layout 用 `WorkbenchSplitPage`。
- **点测**：开关开/关、顺序在 Projects 下、点行出详情、`/mcp` 仍在且名称仍是 MCP。
- **不做**：安装按钮。

### PR-3 启用/停用（不卸载）

- **做**：对 PR-1 已接线 Agent 调官方 enable/disable；写前备份 settings/config。
- **测试**：round-trip；CLI 失败不改文件。
- **点测**：停用后厂商 CLI 仍 list 得到、状态为 disabled；MCP 页不因此丢无关 server。

### PR-4 安装 / 卸载（单市场狗粮）

- **做**：先 Claude 官方市场或 Grok 本地/git 源之一。预览组件清单 → 确认 → CLI → 刷新。
- **测试**：信任/确认失败停在预览；卸载不删 `plugins/data` 除非用户勾选（Grok `--keep-data`）。
- **点测**：装一个无密钥官方示例包，再卸掉。
- **不做**：自建市场、跨 Agent 复制包。

### PR-5 更新

- **做**：`marketplace update` 与 `plugin update` 分成两个动作。钉版本显示清楚。
- **测试**：无新版本 = 已是最新，不是错误。
- **点测**：刷新市场后列表变化；更新已装包后版本号变。

### PR-6 Codex + Pi 端口

- **做**：同样的 list/enable/install 端口，schema 各写各的。
- **不做**：把 Pi npm 包硬转成 Claude `name@marketplace`。

### PR-7（另开，可不上）MCP 页补 Grok TOML 等

与插件页解耦。需要时从 [MCP inventory](../reference/mcp-inventory.md) 缺口单开 PR。

## 9. 决策门槛

插件页升为 current 之前：

- 至少一家 Agent 的 list/enable/disable 有 CLI 或 live-file fixture，且点测过。
- 卸载与停用行为与厂商一致。
- `/mcp` 文案与导航仍表示 MCP。
- 非 Tauri 生产页对写入 unavailable。
- 相关 cargo/vitest/`pnpm check:docs` 通过。

## 10. 相关页面

- [插件、MCP 与技能](../concepts/plugins-and-mcp.md)
- [Agent 插件表面](../reference/agent-plugin-surfaces.md)
- [MCP inventory](../reference/mcp-inventory.md)
- [UI 页面模式](../ui/page-patterns.md)
- [产品边界](../decisions/product-boundaries.md)
---