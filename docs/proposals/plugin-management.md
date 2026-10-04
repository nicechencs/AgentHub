---
title: 插件（extension / plugin）管理
type: proposal
status: proposed
owner: maintainers
updated: 2026-10-04
---

# 插件（extension / plugin）管理

> 状态：proposed。原提案中的四家列表、适用的启停、安装/卸载和更新切片均已落地；本文保留方案边界，现行行为以 [STATUS](../STATUS.md) 与 [页面模式](../ui/page-patterns.md) 为准。

产品对象是各家的 plugin / extension 包，不是 MCP server，也不是随应用交付的 Go 路由程序。`/plugins` 与[Go 路由替换](modularity.md#8-功能模块与-go-路由程序)不是同一对象。`/mcp` 保持只读 MCP 清单，不改名。

## 1. 当前基线

| 状态 | 内容 | 证据 |
| --- | --- | --- |
| 已落地 | Claude / Codex / Grok 列出已装包（优先官方 CLI JSON；官方命令缺失或无法启动时读本机配置或目录，执行或解析失败则报错）；Pi 列 `settings.json` 的 `packages` 并保存完整安装来源 | `crates/agenthub-core/src/services/plugin_inventory.rs` |
| 已落地 | `/plugins` 左右分栏；设置 → 功能「显示插件页面」只藏入口（新安装默认关），开关打开时排在「历史」（`/projects`）下 | `src/pages/plugins/`；`SidebarContext.tsx` 的 `pluginsNavVisible` |
| 已落地 | Claude / Codex / Grok 启用/停用，写前备份；Pi 没有包级启停 | `services/plugin_apply.rs`（`enable_plugin` / `disable_plugin`） |
| 已落地 | 四家安装/卸载：预览 → 确认 → 官方 CLI → 刷新；只有 Grok 要显式信任；Pi 卸载使用 inventory 保存的完整来源 | `plugin_apply.rs`（`preview_plugin_install`、`install_plugin`、`uninstall_plugin`） |
| 已落地 | Claude / Codex / Grok 刷新市场；Claude / Grok 更新用户范围的单包；Pi 全量更新符合条件的扩展，完整 npm 语义版本钉死的包由 Pi 跳过 | `plugin_apply.rs`（`refresh_plugin_marketplace`、`update_plugin`、`update_pi_plugins`） |
| 已落地 | 桌面命令层用四家离线 CLI 做完整写入生命周期探针，校验调用参数、live-write 锁、重新盘点及 CLI 失败后的配置还原；页面回调测试覆盖重复提交和成功/失败状态 | `scripts/plugin-probe/four-vendor-desktop-isolated.sh`；`src-tauri/src/plugin_write_probe.rs`；`src/pages/plugins/index.test.tsx` |
| 关闭 | Cursor、Kimi、WorkBuddy、ZCode、DSH、Kiro | 标 Unsupported，不伪造商店 |

各家是否支持以 [Agent 插件表面](../reference/agent-plugin-surfaces.md#厂商插件系统plugin--extension-包) 为准。

## 2. 已交付切片与剩余目标

### 已落地：更新（原 PR-5）

没有跨厂商协议。界面必须分开三件事：

1. **市场目录有新包**：`claude plugin marketplace update`、`grok plugin marketplace update`。
2. **已装包可升级**：`plugin update`、`pi update --extensions --no-approve`。只有 `npm:包@1.2.3` 这类完整语义版本会跳过；`@1.2` 和 git ref 仍参与更新。
3. **信任 / 健康**：Grok 未 trust 时 hooks/MCP 被挡，这不是版本问题。

不用 MCP `doctor` 或进程是否在跑代表插件更新。没有新版本 = 已是最新，不是错误。

### 已落地：Codex 与 Pi 单包安装写入（原 PR-6）

- Codex：`codex plugin list --json` 列表，`codex plugin add/remove`，启停走 `config.toml`；安装必须选择 `name@marketplace`，市场升级是整体动作。
- Pi：`pi install` / `pi remove`；完整安装来源随 inventory 返回并用于卸载。本地目录只接受绝对路径（`~/…` 由后端展开），避免隔离工作目录改变相对路径含义。符合条件的扩展继续用 `pi update --extensions --no-approve` 全量更新；不把 npm/git/本地包硬转成 Claude 的 `name@marketplace`。

## 3. 同类怎么管（2026-08 对照）

| 模式 | 代表 | AgentHub |
|---|---|---|
| A. 调各家官方 CLI、写各家本机配置 | Claude `/plugin`、Codex `/plugins`、Grok plugin、Pi `pi install`、`claude-code-marketplace` | **采用** |
| B. 本地网关，装一次同步到所有客户端 | mcpx、mcp-mux、Brightwing | 不采用：那是 MCP 网关，常伴随密钥另存 |
| C. 自己当 host 启停进程 | Cline MCP 面板、Goose Extensions | 不采用：AgentHub 不是 MCP runtime；不学 Goose「extension = MCP」的叫法 |

各家目录：Claude `~/.claude/plugins/`（启用看 `enabledPlugins`）、Codex `~/.codex/plugins/cache/`、Grok `~/.grok/plugins/`（enable ≠ trust，hooks/MCP 要 `--trust`）、Pi settings 里的 `packages`。

## 4. 约束

- 能调官方 CLI 就不自己改 cache 目录；CLI 不可用时 fail-closed。
- 启停与卸载分开；改本机配置前备份，失败还原配置文件。官方 CLI 已经改过的包目录或 cache 不保证回滚。
- 详情里列包内组件（skills、commands、agents、hooks、附带 MCP），附带 MCP 不当列表主键。
- 默认不扫项目级未信任目录里的插件源。
- 非 Tauri 生产页写入显示 unavailable。
- 新增 `Capability` 键要等实现 PR。

## 5. 最低检查

- 每个开放写入的 Agent 有 CLI 或本机文件 fixture，且点测过。
- 卸载、停用、更新行为与厂商一致。
- `/mcp` 文案与导航仍表示 MCP。
- 按改动运行相关的非 Rust 检查与 `pnpm check:docs`；Rust 行为以真实 CLI 或应用日志验证。

## 6. 非目标

- AgentHub 运行 `/plugins` 管理的厂商包代码，或当 MCP host。
- 把本页做成 AgentHub 官方扩展、动态插件 ABI 或插件商店 / SDK。
- 合并 Skills 市场、MCP registry、各家插件市场为一个商店；自建跨 Agent 商店。
- 本地 MCP 网关、加密凭据库、国产 OAuth。
- 把 VS Code / Cursor IDE 扩展市场当成 Cursor Agent 插件源。
- 引入 AgentHub 自己的 `~/.agents/plugins` 来源。
- MCP 页补 Grok TOML 等（原 PR-7）另开，不在本页。

## 7. 相关页面

- [模块化提案「功能模块与 Go 路由程序」](modularity.md#8-功能模块与-go-路由程序)：随应用交付的路由程序，与本页厂商插件不是同一对象。
- [插件、MCP 与技能](../concepts/plugins-and-mcp.md)
- [Agent 插件表面](../reference/agent-plugin-surfaces.md)
- [MCP inventory](../reference/mcp-inventory.md)
- [产品边界](../decisions/product-boundaries.md)
