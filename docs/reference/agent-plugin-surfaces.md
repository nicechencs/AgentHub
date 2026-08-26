---
title: Agent 插件表面
description: 各内置 Agent 的 MCP、厂商 Plugin 与技能目录、安装/卸载/更新方式。这是厂商表面对照，不是 AgentHub 已接线的管理 API。
type: reference
audience: contributor
status: current
updated: 2026-08-26
---

# Agent 插件表面

本页对照八个内置 Agent 在**厂商侧**如何安装、卸载、存放和更新扩展。来源是各家公开文档、本机可观察路径，以及 AgentHub adapter / inventory 已编码的路径。AgentHub 当前只对 MCP 做只读扫描、对 Skills 做完整管理；厂商 Plugin 市场未接线。能力等级以 [能力参考](capabilities.md) 为准。

术语：

- **MCP server**：Agent 作为客户端连接的外部工具进程或 URL。
- **Plugin 包**：可安装的发行单元，常常内含 MCP、skills、commands、hooks。
- **技能投影**：AgentHub 把 `~/.agents/skills/<id>` 链到或复制到 Agent 自己的 skills 目录。

未验证的单元格写「未验证」，不要据此把 `Capability::Mcp` 改成 Full。

## 总览

| Agent | MCP 配置（用户级） | MCP 管理命令 | Plugin 包 | 技能目录（AgentHub 投影） | `Capability::Mcp` | `Capability::Skills` |
|---|---|---|---|---|---|---|
| Claude | `~/.claude.json` 的 `mcpServers`；`<claude-home>/settings.json` | `claude mcp add/list`；会话 `/mcp` | `/plugin` 市场；`enabledPlugins`；数据 `~/.claude/plugins/` | `~/.claude/skills` | Planned | Full |
| Codex | `~/.codex/config.toml` 的 `[mcp_servers.<name>]` | `codex mcp add/list`；TUI `/mcp` | `/plugins` 与 `codex plugin`；缓存 `~/.codex/plugins/cache/` | `~/.codex/skills` | Planned | Full |
| Grok | `~/.grok/config.toml` 的 `[mcp_servers.<name>]` | `grok mcp add/list/remove/doctor` | `grok plugin` / marketplace；`~/.grok/plugins/` | `~/.grok/skills` | Planned | Full |
| Cursor | `~/.cursor/mcp.json` 的 `mcpServers` | IDE MCP 设置；改 JSON 后重载 | 未验证独立 CLI 市场 | `~/.cursor/skills-cursor` | Planned | Full |
| Pi | `~/.pi/agent/mcp.json`（或 `$PI_CODING_AGENT_DIR`） | 扩展/适配器读取该文件；热更因发行而异 | `pi install` 装的是 Pi 扩展，不是 MCP server | `~/.pi/agent/skills` | Planned | Full |
| WorkBuddy | `<config>/.mcp.json` | 未验证稳定 CLI | 未验证 | `<config>/skills` | Planned | Full |
| Kimi | 无已验证契约；inventory 只探 `mcp.json` | 未验证 | 未验证 | 无（Skills Unsupported） | Planned | Unsupported |
| DSH | 无已验证 MCP 契约；inventory 只探 JSON | 未验证 | Cordis 插件是 DeepSeek Harness 自己的插件树，**不是** MCP | `~/.dsh/skills` | Planned | Full |

## Claude Code

**MCP**

- 用户级：`~/.claude.json` → `mcpServers`。`CLAUDE_CONFIG_DIR` 下的 `settings.json` 也可能含 MCP，inventory 两边都扫。
- 项目级：仓库 `.mcp.json`（AgentHub 不扫）。
- 安装：`claude mcp add …` 或手写 JSON。stdio 用 `command`/`args`/`env`，远程用 `url` + `type`（sse / http）。
- 卸载：从对应 JSON 删除该键，或 CLI 的 remove 等价操作。
- 更新：配置里的 `npx -y pkg@latest` 每次拉包；钉版本则改 args。没有独立的「检测 MCP 更新」协议。
- Plugin 内 MCP：插件根 `.mcp.json` 或 `plugin.json` 的 `mcpServers`。启用插件后由 Claude 拉起，不走 `/mcp add`。可在 `/mcp` 里关掉某个 plugin server 而不卸插件。

**Plugin 包**

- 安装：`/plugin install {name}@{marketplace}`，例如 `mcp-server-dev@claude-plugins-official`。
- 范围：user → `~/.claude/settings.json` 的 `enabledPlugins`；project → `.claude/settings.json`；local → `.claude/settings.local.json`。
- 目录：安装树在 Claude 插件根；持久数据 `~/.claude/plugins/data/{id}/`，更新插件时保留。
- 更新：市场/插件系统；用户 `enabledPlugins` 在更新后保持。
- 检测更新：厂商市场刷新，不是 AgentHub doctor。

**技能**

- 投影根：`<claude-home>/skills`。AgentHub 共享源仍是 `~/.agents/skills/`。

## Codex

**MCP**

- 用户级：`~/.codex/config.toml`，表名必须是 `mcp_servers`（不是 `mcpServers`）。
- 项目级：受信任项目里的 `.codex/config.toml`（AgentHub 不扫）。
- 安装：`codex mcp add <name> -- <command>` 或手写 `[mcp_servers.name]`。HTTP 用 `url` + `bearer_token_env_var` 等。
- 卸载：删表或 CLI remove。
- 更新：改 command/args 或包版本；无统一 MCP 升级 API。
- TUI：`/mcp` 查看已配置 server。

**Plugin 包**

- TUI `/plugins`；CLI `codex plugin add|list|remove` 以及 `codex plugin marketplace …`。
- 安装缓存：`~/.codex/plugins/cache/$MARKETPLACE/$PLUGIN/$VERSION/`。本机还有 `~/.codex/plugins/.remote-plugin-install-staging`。
- 市场索引：官方目录 + 仓库 `$REPO/.agents/plugins/marketplace.json` + 个人 `~/.agents/plugins/marketplace.json`。
- 启用写在 `config.toml`；Space 切换 enabled。市场升级是 marketplace upgrade，不是单条 MCP doctor。

**技能**

- 投影根：`~/.codex/skills`。

## Grok Build

**MCP**

- 用户级：`~/.grok/config.toml` 的 `[mcp_servers.<name>]`（`$GROK_HOME` 可改根）。
- 项目级：`.grok/config.toml` 只贡献 `[mcp_servers]`、`[plugins]`、`[permission]`、`[mcp] max_output_bytes`。优先级：cwd > repo root > user。
- 安装：`grok mcp add …`（stdio 在 `--` 后跟命令；HTTP 用 `--transport http`）。也可手写 TOML。
- 卸载：`grok mcp remove <name>`。
- 诊断：`grok mcp list`、`grok mcp doctor [name]`（可 `--json`）。
- 密钥：`${VAR}` 展开；OAuth token 在 `~/.grok/mcp_credentials.json`。
- **AgentHub inventory 今天不读这份 TOML**，只探 `~/.grok/mcp.json`。

**Plugin 包**

- `grok plugin marketplace add|list|update|remove`。
- `grok plugin install|uninstall|update|enable|disable|list|details`。未加 `--trust` 的 install 会停在确认。
- 用户插件树：`~/.grok/plugins/`；项目：`.grok/plugins/`。
- TUI：`Ctrl+L` 或 `/plugins`，含 Hooks / Plugins / Marketplace / Skills / MCP Servers 五个页。

**技能**

- 投影根：`~/.grok/skills`。也可由 plugin 附带 skills。

## Cursor Agent

**MCP**

- 全局：`~/.cursor/mcp.json`，根键 `mcpServers`。
- 项目：`<repo>/.cursor/mcp.json`（AgentHub 不扫）。
- 安装/卸载：改 JSON 或 IDE「MCP 设置」；保存后 Reload / 重启。
- 传输：stdio 的 `command`/`args`/`env`，远程 `url` + `headers`。插值：`${env:NAME}`、`${userHome}`、`${workspaceFolder}`。
- 更新：改包版本或 URL；无独立 marketplace 升级流（相对 Claude/Codex/Grok）。
- inventory 另外探测 `<cursor-home>/mcp.json`，与全局 `~/.cursor/mcp.json` 在默认 home 时是同一文件。

**技能**

- 投影根：`~/.cursor/skills-cursor`（目录名不是 `skills`）。

## Pi

**MCP**

- 已观察/文档化的用户文件：`~/.pi/agent/mcp.json`（`PI_CODING_AGENT_DIR` 可改）。形状多为 `mcpServers`。
- 发行版差异大：有的用内置 MCP，有的用 `pi-mcp-adapter` 等扩展；热加载、`/mcp` 命令、import cursor/claude 配置都不是所有 Pi 构建都有。
- AgentHub 同时探 `mcp.json` 与 `.mcp.json`。项目级 `.pi/mcp.json` 不扫。
- `pi install npm:…` 装的是 **Pi 扩展**，不要当成 MCP server 安装。

**技能**

- 投影根：`~/.pi/agent/skills`。

## WorkBuddy

**MCP**

- inventory 只认 `<config>/.mcp.json`。`WORKBUDDY_CONFIG_DIR` / `CODEBUDDY_CONFIG_DIR` 可改 config 根。
- 管理 CLI、更新检测：**未验证**。写入前必须先有 round-trip 测试。

**技能**

- 投影根：`<config>/skills`。

## Kimi

- Skills：Unsupported，「Kimi 不支持技能目录」。
- MCP：无已验证路径。inventory 探测 `<kimi-home>/mcp.json` 与 `.mcp.json` 以便 UI 显示未发现，不表示格式已确认。
- 不要为填矩阵伪造 Full。

## DeepSeek Harness (DSH)

- Skills：`~/.dsh/skills`，Full。
- MCP：无已验证契约；inventory 只探 JSON。
- Cordis / `cordis.patch.yml` 里的「插件」是 Harness 自己的 LLM 插件行，**不是** MCP server，不能映射到 MCP 页的添加/卸载。

## AgentHub 技能管理（对照）

Skills 已有完整写入面，MCP 管理应抄它的纪律，而不是抄它的目录模型（技能有共享真源；MCP live 文件属于各 Agent）。

| 动作 | Skills 今天怎么做 |
|---|---|
| 安装 | 本地路径 / zip / git → staging → 校验 `SKILL.md` → 提交到 `~/.agents/skills/<id>` → reconcile 投影 |
| 卸载 | 先拆各 Agent 投影，再删共享源和 lock |
| 更新 | git 源 clone 到 staging，禁止对 live 树 `git pull`，校验后原子替换 |
| 市场 | `skillMarketSource`：auto / skills.sh / skillhub.cn；与 Claude/Codex/Grok plugin marketplace 不是同一套 |
| 锁 | `.skill-lock.json` + per-skill / root lock |

## 更新检测（厂商侧）

没有跨 Agent 的统一「MCP 有新版本」协议。实际出现的是四类：

1. **每次启动拉包**：`npx -y pkg@latest` / `uvx` 浮动标签。
2. **钉死版本**：改 config 里的包名或 git ref。
3. **Plugin 市场刷新**：Claude / Codex / Grok 的 marketplace update；升级的是包，可能连带 MCP。
4. **连通性诊断**：Grok `mcp doctor`、Codex/Claude `/mcp` 状态。这是 handshake，不是版本号。

AgentHub 若做检测，应先分清「配置变更 / 进程可达 / 包版本」，不要用 doctor 输出冒充 marketplace 升级。

## 相关页面

- [插件、MCP 与技能](../concepts/plugins-and-mcp.md)
- [MCP inventory](mcp-inventory.md)
- [插件管理提案](../proposals/plugin-management.md)
- [能力参考](capabilities.md)
---