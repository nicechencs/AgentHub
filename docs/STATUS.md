---
title: AgentHub 当前实现状态
type: status
status: current
owner: maintainers
updated: 2026-08-27
---

# 当前实现状态

本页记录当前工作区的实现事实，不是路线图。发布包可能落后于工作区；需要判断某个版本时，以对应 Release 和源码为准。

## 产品表面

- 桌面端由 Tauri v2 承载，前端使用 React，核心业务和 CLI 使用 Rust。
- 当前界面包含 Dashboard、Agents、Connections、Routes、Skills、MCP、Chat、Projects、Plugins 和 Settings。
- Connections 是跨工具的登录列表；Dashboard 和 Connections 是日常连接入口；Routes 只管理本机路由运行时。
- 当前内置适配包括 Claude Code、Codex、Kimi、Grok、Pi、WorkBuddy、Cursor Agent 和 DeepSeek Harness。
- CLI 提供 doctor、env、agent、provider、account、skill、usage、backup、run、config 等命令；参数以 CLI 帮助和源码为准。

## Backend 边界

- `src/lib/backend/tauri/` 是唯一可以调用 Tauri `invoke` 的前端目录。
- `src/lib/backend/contracts/` 保存 DTO、接口和纯映射；`src/lib/api/` 是逐步迁移中的兼容 façade。
- `src/dev/mocks/` 只服务浏览器 mock 和测试。`pnpm dev:mock` 显式选择 mock；生产构建不会静默回退到 mock。
- `pnpm dev` 启动普通 Vite 前端开发服务器；`pnpm tauri:dev` 启动真实 Tauri 桌面端；`pnpm build` 强制走 Tauri adapter。

## 连接、路由与用量

- 登录的来源、目标和可行写入动作由 `plan` / `bind` / `unbind` 契约表达；领域实现仍保留 Ticket / TicketPort 等内部名称。
- 本机路由运行时在桌面进程内运行，面向兼容客户端提供 `/v1/messages`、`/v1/responses`、`/v1/chat/completions` 和 `GET /models` 等端点；领域背景见 [连接与路由](concepts/connections-and-routing.md)。
- Usage 只读解析本地 Agent 会话或日志；优先使用日志中的官方成本字段，否则使用离线内嵌价表估算。运行时不联网拉取价格，也不做汇率换算。
- Skills 使用共享目录 `~/.agents/skills/`；配置切换在修改前创建备份。
- MCP 页只读扫描已知 MCP server 配置；`Capability::Mcp` 对全部内置 Agent 仍为 Planned。见 [MCP inventory](reference/mcp-inventory.md)。
- 插件页 `/plugins` 只读列出 Claude / Grok 的 plugin / extension 包（优先官方 CLI JSON，否则读 live 目录）。没有安装按钮，也没有 `Capability::Plugins`。Codex / Pi 仍为 Planned；Cursor / Kimi / WorkBuddy / DSH 为 Unsupported。见 [插件、MCP 与技能](concepts/plugins-and-mcp.md)。

## 验证与发布

- 日常改动按 [AGENTS.md](../AGENTS.md) 的风险分级选择过滤测试；全量 `pnpm test`、完整 Rust crate 矩阵和生产 `pnpm build` 默认留给提交前或 CI。
- 前端检查包括 `pnpm typecheck`、`pnpm typecheck:test`、`pnpm test` 和 `pnpm build`；浏览器 DOM 冒烟用 `pnpm test:e2e:browser`，只覆盖 `dev:mock`，不覆盖 Tauri。贡献者流程见 [测试与验证](guides/testing-and-validation.md)。
- Rust 核心、CLI 和 GUI 分别由对应 crate 的 `cargo test --locked` 验证。
- `pnpm check:docs` 检查活跃 Markdown 的本地链接、标题锚点、元数据和已废弃路径标注。
- PR CI 运行前端类型检查、构建、测试、三个 Rust crate 测试，以及独立的 Playwright Chromium 浏览器冒烟 job；正式发布由 `release` 分支上的 `v*` tag 触发。
- 发布前必须同步 `package.json`、`Cargo.toml` workspace package 和 `src-tauri/tauri.conf.json` 的版本号。

## 已知边界

- `agenthub-adapterd` sidecar 目标架构尚未替代当前桌面进程内的路由运行时。
- 本机同口授权池已作为默认 Routes 能力打开：每个目标 Agent/surface 一个默认池，共用 loopback 入口和本机令牌；`GET /models` 与 dispatch 共用 resolver；默认 `priority_failover`；官方直连不自动入池。混合供应商复合路由和 Codex↔Grok 双向 Responses 仍是实验开关、默认关闭。现行契约见 [连接与路由](concepts/connections-and-routing.md) 和 [本机 Routes API](reference/local-route-api.md)；设计稿见 [本机同口授权池（归档）](archive/unified-loopback-pool.md)。
- 托盘低内存后台模式仍是未实施方案，不从它派生当前任务。
- `AdapterRouteService::plan()` 是 Adapter / route 的唯一产品决策者。`adapter-capability-contract.json` 是它对冻结入参的只读投影；Rust 测试在 JSON 与内核输出不一致时失败。browser mock 只按来源特征查表并维护内存状态；凭据可用性必须精确匹配；未命中 fail-closed 为 unsupported，不回退 classify。route / support / ruleId / gateKind / canApply 的产品正确性在 Rust；Vitest 覆盖查表、脱敏、内存 apply 和页面听从 plan。见 [Adapter 路线内核](architecture/adapter-route-kernel.md)。
- 不落地 sccache，也不把 `agenthub-core` 拆成多个 crate。CI 使用 `Swatinem/rust-cache`。Windows worktree 不得共享 `target/`。2026-08-25 的热缓存过滤测试约 3.5 秒、冷 worktree 首次编译依赖约 42 秒是历史快照，不是当前固定规模；过程见 [单一内核提案归档](archive/single-kernel-projections.md)。
- DeepSeek Harness 的 StructuredStream 仍是规划项；已落地部分以源码和集成文档为准。
- 插件包的启用/安装/更新仍是提案，不从 MCP inventory 推导。见 [插件管理](proposals/plugin-management.md)。MCP 写入同样未做，且是另一条线。
- Codex 安装、外部渠道 Chat 调用与连接/路由模块化审查见 [Codex 安装与模块化审查](status/codex-install-modularity-review.md)（2026-08-27）。
- 凭据落盘加密不在产品范围内；国产 OAuth 适配以及 OAuth 转 API 也不在产品范围内。它们不是当前 backlog。

## 真源优先级

1. 源码、测试和 `package.json` / Cargo 配置决定当前行为。
2. 本页记录跨模块的实现事实；领域契约以目标树中对应的参考文档为准。
3. 设计方案和路线图必须明确写出 `planned` 或 `historical`，不能覆盖当前实现事实。
4. 已完成的一次性方案进入 [archive/](archive/README.md)，不作为未完成任务派工。
