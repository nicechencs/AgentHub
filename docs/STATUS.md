---
title: AgentHub 当前实现状态
type: status
status: current
owner: maintainers
updated: 2026-09-03
---

# 当前实现状态

本页记录当前工作区的实现事实，不是路线图。发布包可能落后于工作区；需要判断某个版本时，以对应 Release 和源码为准。

## 产品表面

- 桌面端由 Tauri v2 承载，前端使用 React，核心业务和 CLI 使用 Rust。
- 当前界面包含 Dashboard、Agents、Connections、Routes、Skills、MCP、Chat、Projects、Plugins 和 Settings。
- Connections 是跨工具的登录列表。接到某个工具从 Dashboard「连接/切换」；连接页行入口是「分享至连接池」，登录仍留在连接页。**产品决策：所有 API Key 都可以分享（含 WorkBuddy / ZCode 等上配置的）；国产官方登录不能分享**，见 [产品边界](decisions/product-boundaries.md)。Routes 管理本机路由运行时，连接池也可以添加官方登录 / API Key（仅用于连接池，可不出现在连接页），并可用「从连接同步」一次加入多份登录。在连接池里编辑从连接页分享来的官方登录并保存时，会先复制成连接池自己的一份（连接页那份还在），再问要不要把模型写回连接页。连接页与连接池相互独立，回收站也分开。登录按登录方式分行保存（官方登录与 API Key 分开），记下关键词和整份配置；详情列出记下的配置文件（可复制、打开所在目录），并补充套餐、有效期、时间线与完整端点。WorkBuddy 自定义模型和 ZCode 供应商按目录拆成多条登录，桌面套餐登录不导入；WorkBuddy 写入只认 `/v1/chat/completions`。
- 当前内置适配包括 Claude Code、Codex、Kimi、Grok、Pi、WorkBuddy、ZCode 和 DeepSeek Harness。**Cursor Agent 适配器仍在代码中，但 dev 线通过 store-stamp 默认软隐藏**（Agents 管理页可取消隐藏）；待登录写入、路由目标与结构化输出等兼容问题修复后再重新开放。
- CLI 提供 doctor、env、agent、provider、account、skill、usage、backup、run、config 等命令；参数以 CLI 帮助和源码为准。

## Backend 边界

- `src/lib/backend/tauri/` 是唯一可以调用 Tauri `invoke` 的前端目录。
- `src/lib/backend/contracts/` 保存 DTO、接口和纯映射；`src/lib/api/` 是逐步迁移中的兼容 façade。
- `src/dev/mocks/` 只服务浏览器 mock 和测试。`pnpm dev:mock` 显式选择 mock；生产构建不会静默回退到 mock。
- `pnpm dev` 启动普通 Vite 前端开发服务器；`pnpm tauri:dev` 启动真实 Tauri 桌面端；`pnpm build` 强制走 Tauri adapter。

## 连接、路由与用量

- 登录的来源、目标和可行写入动作由 `plan` / `bind` / `unbind` 契约表达；领域实现仍保留 Ticket / TicketPort 等内部名称。
- 本机路由运行时在桌面进程内运行，面向兼容客户端提供 `/v1/messages`、`/v1/responses`、`/v1/chat/completions` 和 `GET /models` 等端点。Codex 与 Grok 都走 Responses 口，具体格式跟这条路由一起保存，由本机令牌选中，不根据请求正文猜测。接到 Codex / Grok 时写入的是本机令牌（按 API Key 方式）和 Responses 接口，不是上游官方登录。领域背景见 [连接与路由](concepts/connections-and-routing.md)。
- Usage 只读解析本地 Agent 会话或日志；优先使用日志中的官方成本字段，否则使用离线内嵌价表估算。运行时不联网拉取价格，也不做汇率换算。总览趋势可按 Agent 或模型切换；悬停同时看 token 和费用。Grok 用量把 `grok-4.6` 与 `grok-4.6-build`（以及 `[grok]` / `xai/` 前缀）当成同一个公开模型。
- Skills 页分用户技能、项目技能和市场。用户技能仍用共享目录 `~/.agents/skills/`，并可启用到各工具；项目技能从项目页已识别的工作区下拉选择，读写该项目的 `.agents/skills/`（列表也会带上 `.claude/skills` 等已有目录）。配置切换在修改前创建备份。
- MCP 页只读扫描已知 MCP server 配置；`Capability::Mcp` 对全部内置 Agent 仍为 Planned。见 [MCP inventory](reference/mcp-inventory.md)。
- 插件页 `/plugins` 只读列出 Claude / Grok 的 plugin / extension 包（优先官方 CLI JSON，否则读 live 目录）。没有安装按钮，也没有 `Capability::Plugins`。Codex / Pi 仍为 Planned；Cursor / Kimi / WorkBuddy / DSH / ZCode 为 Unsupported。见 [插件、MCP 与技能](concepts/plugins-and-mcp.md)。

## 验证与发布

- 日常改动按 [AGENTS.md](../AGENTS.md) 的风险分级选择过滤测试；全量 `pnpm test`、完整 Rust crate 矩阵和生产 `pnpm build` 默认留给提交前或 CI。
- 前端检查包括 `pnpm typecheck`、`pnpm typecheck:test`、`pnpm test` 和 `pnpm build`；浏览器 DOM 冒烟用 `pnpm test:e2e:browser`，只覆盖 `dev:mock`，不覆盖 Tauri。贡献者流程见 [测试与验证](guides/testing-and-validation.md)。
- Rust 核心、CLI 和 GUI 分别由对应 crate 的 `cargo test --locked` 验证。
- `pnpm check:docs` 检查活跃 Markdown 的本地链接、标题锚点、元数据和已废弃路径标注。
- PR CI 运行前端类型检查、构建、测试、三个 Rust crate 测试，以及独立的 Playwright Chromium 浏览器冒烟 job；正式发布由 **`dev` 上推送的 `v*` tag** 触发，且 tag 指向的提交必须已在 `release` 上（先合 `dev` → `release`，再在 `dev` 打 tag）。
- 发布前以 **`package.json` 为版本真源**；`pnpm release:sync-version` 同步 `Cargo.toml` 与 `Cargo.lock`，`tauri.conf.json` 引用 `../package.json`。

## 已知边界

- `agenthub-adapterd` sidecar 目标架构尚未替代当前桌面进程内的路由运行时。
- 本机同口授权池已作为默认 Routes 能力打开：每个目标 Agent/surface 一个默认池，共用 loopback 入口和本机令牌；`GET /models` 与 dispatch 共用 resolver；默认 `priority_failover`；官方直连不自动入池。混合供应商复合路由和 Codex↔Grok 双向 Responses 仍是实验开关、默认关闭。已保存的本机入口和 Responses 格式（Codex 或 Grok）必须与当前端点一致，否则准备启动时失败，不会悄悄改成直通。现行契约见 [连接与路由](concepts/connections-and-routing.md) 和 [本机 Routes API](reference/local-route-api.md)；设计稿见 [本机同口授权池（归档）](archive/unified-loopback-pool.md)。
- 托盘低内存后台模式仍是未实施方案，不从它派生当前任务。
- `AdapterRouteService::plan()` 是 Adapter / route 的唯一产品决策者。`adapter-capability-contract.json` 是它对冻结入参的只读投影；Rust 测试在 JSON 与内核输出不一致时失败。browser mock 只按来源特征查表并维护内存状态；凭据可用性必须精确匹配；未命中 fail-closed 为 unsupported，不回退 classify。route / support / ruleId / gateKind / canApply 的产品正确性在 Rust；Vitest 覆盖查表、脱敏、内存 apply 和页面听从 plan。见 [Adapter 路线内核](architecture/adapter-route-kernel.md)。
- 不落地 sccache，也不把 `agenthub-core` 拆成多个 crate。CI 使用 `Swatinem/rust-cache`。Windows worktree 不得共享 `target/`。2026-08-25 的热缓存过滤测试约 3.5 秒、冷 worktree 首次编译依赖约 42 秒是历史快照，不是当前固定规模；过程见 [单一内核提案归档](archive/single-kernel-projections.md)。
- DeepSeek Harness 的 StructuredStream 仍是规划项；已落地部分以源码和集成文档为准。
- 插件包的启用/安装/更新仍是提案，不从 MCP inventory 推导。见 [插件管理](proposals/plugin-management.md)。MCP 写入同样未做，且是另一条线。
- Codex 安装、外部渠道 Chat 调用与连接/路由模块化审查见 [Codex 安装与模块化审查](status/codex-install-modularity-review.md)（2026-08-27）。
- npm 渠道安装写到检测会扫的用户前缀（`~/.npm-global`，Windows 为 `%APPDATA%\npm`）。`~/.agenthub` 以及其中的 `npm` 只是遗留，不是安装目标，也不是启动路径。
- WorkBuddy 本机安装只打开官网安装页，界面给中文指引，不当成「安装失败」。真失败时「重试」是主按钮；失败面板先显示诊断，不把 npm 下载进度当正文。
- ZCode 本机安装同样只打开官网；API Key 按目录追加写入 `~/.zcode/v2/config.json` 的一条供应商（官方槽或自定义行），不替换其它条目；套餐登录不导入；自定义行必须带模型名单。Chat 优先 PATH 上的 `zcode` CLI，只有桌面安装时不会虚构一条捆绑命令。Projects 只读任务索引，预览从命令行会话库读取对话正文；删除按钮禁用，提示到 ZCode 里删除。用量从命令行 `model_usage` 采集。
- WorkBuddy 用量读取 `projects/**/*.jsonl` 里的 `providerData.usage`（以及旧的 `message.usage` 形状）。
- Kimi 切换写出带模型表的完整 `~/.kimi-code/config.toml`，使 `kimi-k2` 能用；旧登录再切换也会补上模型表。供应商 `type` 按地址补全（官方 Moonshot 为 `kimi`，Messages / Responses / 补全各写对应协议）。数据根认 `KIMI_CODE_HOME`。技能：共享库会被 Kimi 自己读取，不再投影一份。对话失败用中文。
- Cursor 没有稳定的本机登录文件可写。切换失败给出中文说明，不静默。保存第二张登录不会因同一把钥匙悄悄把第一张送进回收站。**dev 线默认软隐藏 Cursor Agent**（`agent_visibility.json` store-stamp）；兼容修复完成前不在侧栏、连接、Chat 等页面展示，Agents 管理页可取消隐藏。
- 「使用官方服务」默认勾选不禁用智能识别。高级编辑器不回显明文钥匙。同一工具切换成功 toast 说明已写入本机配置；接到本机路由则仍说已切换。备份标题是「切换前自动 / 手动 + 时间」。设置里的安全备份默认在切换/导入时保留本机配置副本（可关闭自动堆积；当次切换仍留一份以便失败回滚）；卡片左右分栏，点开在右侧展示打码后的文件内容。
- 官方登录等待页不显示内部状态或登录文件路径；失败时「重试」是主按钮。Windows 上子进程统一无窗启动。
- GUI 日志：智能识别 `gui`/`recognize`，勾选官方 `gui`/`use_official`，删进回收站 `core.provider`/`recycle`，切换写本机路径 `core.provider`/`switch_write`。连接页切换、Dashboard 连接流程和路由页成功失败另记 `gui`/`switch`·`bind`·`route_*`·`bridge_*`；核心绑定记 `core.adapter`/`bind`·`unbind`。只记 last4，不写明文钥匙。见 [日志参考](reference/logging.md)。
- 凭据落盘加密不在产品范围内；国产 OAuth 适配以及 OAuth 转 API 也不在产品范围内。它们不是当前 backlog。产品上所有 API Key 都可以分享至连接池并接到其他工具，国产官方登录不能分享。**当前实现仍按所属 Agent 白名单入池**（claude / codex / grok / kimi / dsh）；WorkBuddy / ZCode / Pi / Cursor 上的 API Key 在连接页会被禁用。这与产品决策不一致，不是「这些 Key 不该分享」。见 [产品边界](decisions/product-boundaries.md)。

## 真源优先级

1. 源码、测试和 `package.json` / Cargo 配置决定当前行为。
2. 本页记录跨模块的实现事实；领域契约以目标树中对应的参考文档为准。
3. 设计方案和路线图必须明确写出 `planned` 或 `historical`，不能覆盖当前实现事实。
4. 已完成的一次性方案进入 [archive/](archive/README.md)，不作为未完成任务派工。
